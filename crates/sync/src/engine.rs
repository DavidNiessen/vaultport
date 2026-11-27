// SPDX-License-Identifier: GPL-3.0-or-later
//! The sync engine.
//!
//! # Sync
//!
//! [`SyncEngine::sync`] lists the vault's segments, downloads every segment it has not seen
//! yet (from all devices, verifying each device's hash chain), applies the contained ops with
//! [`Vault::apply_remote`], then uploads the device's new local ops as one new delta segment.
//! Ops are a CRDT, so the order in which segments of different devices are applied does not
//! matter.
//!
//! # Integrity
//!
//! The storage is untrusted. It cannot read or forge segments (AEAD with a vault-key
//! subkey, AAD bound to vault, device, counter and kind), but it can hide, delete or replay
//! files. Per device the engine keeps the newest verified `(counter, hash)` in
//! [`SyncState`] and reports, without applying anything suspicious:
//!
//! * [`Issue::Gap`]: a segment is missing in the middle of a device's chain (truncation, or a
//!   file that has not propagated yet in an eventually consistent folder);
//! * [`Issue::Rollback`]: the storage shows an older state of a device than already seen;
//! * [`Issue::Tampered`]: a segment fails authentication or validation;
//! * [`Issue::Fork`]: two different segments claim the same position in a device's chain
//!   (e.g. a device restored from an old backup).
//!
//! Processing of the affected device stops at the problem; other devices continue. If this
//! device's own uploads were rolled back, the next upload is a full snapshot that re-seeds
//! the storage.
//!
//! Tail truncation (hiding the newest segments of a device) cannot be detected by a device
//! that has never seen them; a device that has seen them reports a rollback.
//!
//! # Compaction
//!
//! [`SyncEngine::compact`] first syncs, then writes a *snapshot* segment containing
//! [`Vault::all_syncable_ops`] (all ops of all devices this device has applied) and records,
//! for every other device, the head it includes (`covers`). Only after the snapshot is stored
//! does it delete segments:
//!
//! * this device's older segments are always deleted — the snapshot contains all their ops
//!   and readers accept a snapshot regardless of gaps before it;
//! * other devices' segments are deleted only with [`CompactOptions::prune_foreign`], only
//!   if the preceding sync reported no issue at all, and only those whose counter is
//!   `<=` the head covered by the snapshot just written. Readers that later find such a gap
//!   first apply the snapshot, which advances their head for that device to the covered
//!   `(counter, hash)`, so the chain stays verifiable. A snapshot is itself only deleted by
//!   another device's compaction when that device's newer snapshot covers it, which implies it
//!   applied (and therefore re-includes) all of its ops.

use crate::backend::{Precondition, SyncBackend};
use crate::error::SyncError;
use crate::keys::{Purpose, now_ms, subkey, vault_key};
use crate::segment::{
    self, SEGMENT_FORMAT, SegmentBody, SegmentKind, SegmentName, file_hash, segments_prefix,
};
use crate::state::{Head, SyncState};
use std::collections::{BTreeMap, HashSet};
use std::fmt;
use uuid::Uuid;
use vault::hlc::DeviceId;
use vault::{ChangeEvent, SealedOp, SymmetricKey, Vault, VaultError};

/// Segments larger than this are not downloaded.
pub const MAX_SEGMENT_BYTES: u64 = 256 * 1024 * 1024;

/// An integrity problem found while syncing.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Issue {
    Gap {
        device: DeviceId,
        expected: u64,
        found: u64,
    },
    Rollback {
        device: DeviceId,
        expected: u64,
        visible: u64,
    },
    Tampered {
        name: String,
        reason: String,
    },
    Fork {
        device: DeviceId,
        counter: u64,
    },
}

impl Issue {
    /// The device the issue concerns, if known.
    #[must_use]
    pub fn device(&self) -> Option<DeviceId> {
        match self {
            Self::Gap { device, .. }
            | Self::Rollback { device, .. }
            | Self::Fork { device, .. } => Some(*device),
            Self::Tampered { .. } => None,
        }
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gap {
                device,
                expected,
                found,
            } => write!(
                f,
                "device {device}: segment {expected} is missing (next available: {found})"
            ),
            Self::Rollback {
                device,
                expected,
                visible,
            } => write!(
                f,
                "device {device}: storage shows segment {visible} but {expected} was already seen (rollback)"
            ),
            Self::Tampered { name, reason } => write!(f, "segment {name} rejected: {reason}"),
            Self::Fork { device, counter } => {
                write!(
                    f,
                    "device {device}: conflicting segments at position {counter}"
                )
            }
        }
    }
}

/// A segment written by this device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uploaded {
    pub name: String,
    pub kind: SegmentKind,
    pub counter: u64,
    pub ops: usize,
}

/// Outcome of a sync run.
#[derive(Debug, Clone, Default)]
pub struct SyncReport {
    /// The segment uploaded by this run, if any.
    pub uploaded: Option<Uploaded>,
    /// Segments downloaded and applied.
    pub segments_applied: usize,
    /// Ops contained in those segments (including already known ones).
    pub ops_received: usize,
    /// Visible changes, for refreshing the UI.
    pub changes: Vec<ChangeEvent>,
    /// Integrity problems (see the module docs).
    pub issues: Vec<Issue>,
    /// Objects that were ignored (conflict copies, foreign files).
    pub ignored: Vec<String>,
    /// Set when the upload was skipped because this device's own chain has a problem.
    pub upload_blocked: bool,
}

impl SyncReport {
    /// `true` if no integrity issue was found.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }
}

/// Options for [`SyncEngine::compact`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompactOptions {
    /// Also delete other devices' segments covered by the new snapshot (see module docs).
    pub prune_foreign: bool,
}

/// Outcome of a compaction.
#[derive(Debug, Clone, Default)]
pub struct CompactReport {
    /// The sync performed before compacting.
    pub sync: SyncReport,
    /// The snapshot written (`None` if compaction was refused because of issues with this
    /// device's own chain).
    pub snapshot: Option<Uploaded>,
    /// Objects deleted.
    pub deleted: Vec<String>,
}

/// Options for [`SyncEngine`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncOptions {
    /// Upload local changes.
    pub upload: bool,
    /// Download and apply remote changes.
    pub download: bool,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            upload: true,
            download: true,
        }
    }
}

/// Syncs a vault through a [`SyncBackend`].
#[derive(Debug, Clone, Default)]
pub struct SyncEngine {
    pub options: SyncOptions,
}

struct Listing {
    by_device: BTreeMap<DeviceId, Vec<(SegmentName, u64)>>,
    names: HashSet<String>,
}

impl Listing {
    fn max_counter(&self, device: DeviceId) -> u64 {
        self.by_device
            .get(&device)
            .and_then(|v| v.iter().map(|(s, _)| s.counter).max())
            .unwrap_or(0)
    }
}

struct Ctx<'a> {
    key: SymmetricKey,
    vault_id: Uuid,
    backend: &'a dyn SyncBackend,
}

fn parse_device(hex: &str) -> Option<DeviceId> {
    (hex.len() == 16)
        .then(|| u64::from_str_radix(hex, 16).ok())
        .flatten()
        .map(DeviceId)
}

/// Whether a vault error while applying remote ops means the *data* is bad (as opposed to a
/// local failure such as a full disk).
fn is_data_error(e: &VaultError) -> bool {
    matches!(
        e,
        VaultError::Authentication
            | VaultError::Corrupt(_)
            | VaultError::Serde(_)
            | VaultError::Crypto(_)
    )
}

impl SyncEngine {
    #[must_use]
    pub fn new(options: SyncOptions) -> Self {
        Self { options }
    }

    async fn list(ctx: &Ctx<'_>, report: &mut SyncReport) -> Result<Listing, SyncError> {
        let objects = ctx.backend.list(&segments_prefix(ctx.vault_id)).await?;
        let mut listing = Listing {
            by_device: BTreeMap::new(),
            names: HashSet::new(),
        };
        for o in objects {
            if crate::backend::is_conflict_copy(&o.name) {
                tracing::warn!(name = %o.name, "ignoring conflict copy");
                report.ignored.push(o.name);
                continue;
            }
            let Some(seg) = SegmentName::parse(ctx.vault_id, &o.name) else {
                tracing::warn!(name = %o.name, "ignoring unexpected object");
                report.ignored.push(o.name);
                continue;
            };
            listing.names.insert(o.name.clone());
            listing
                .by_device
                .entry(seg.device)
                .or_default()
                .push((seg, o.size));
        }
        for segs in listing.by_device.values_mut() {
            segs.sort_by(|a, b| a.0.counter.cmp(&b.0.counter).then(a.0.name.cmp(&b.0.name)));
        }
        Ok(listing)
    }

    /// Downloads and applies new segments of one device. Returns `true` if a snapshot
    /// advanced the head of another device.
    #[allow(clippy::too_many_arguments)]
    async fn read_device(
        ctx: &Ctx<'_>,
        vault: &mut Vault,
        state: &mut SyncState,
        device: DeviceId,
        segs: &[(SegmentName, u64)],
        report: &mut SyncReport,
        issues: &mut Vec<Issue>,
    ) -> Result<bool, SyncError> {
        if let Some(Head {
            counter,
            name: Some(seen),
            ..
        }) = state.head(device)
        {
            let mut at = segs
                .iter()
                .filter(|(s, _)| s.counter == *counter)
                .peekable();
            if at.peek().is_some() && !at.any(|(s, _)| &s.name == seen) {
                issues.push(Issue::Fork {
                    device,
                    counter: *counter,
                });
                return Ok(false);
            }
        }
        let start = state.head(device).map_or(0, |h| h.counter);
        let mut pending: Vec<&(SegmentName, u64)> =
            segs.iter().filter(|(s, _)| s.counter > start).collect();
        if let Some(w) = pending
            .windows(2)
            .find(|w| w[0].0.counter == w[1].0.counter)
        {
            issues.push(Issue::Fork {
                device,
                counter: w[0].0.counter,
            });
            return Ok(false);
        }
        // Everything before the newest snapshot is contained in it.
        if let Some(i) = pending
            .iter()
            .rposition(|(s, _)| s.kind == SegmentKind::Snapshot)
        {
            pending.drain(..i);
        }

        let mut advanced_others = false;
        for (seg, size) in pending {
            let head = state.head(device).cloned();
            let current = head.as_ref().map_or(0, |h| h.counter);
            if seg.counter <= current {
                continue;
            }
            let contiguous = seg.counter == current + 1;
            if seg.kind == SegmentKind::Delta && !contiguous {
                issues.push(Issue::Gap {
                    device,
                    expected: current + 1,
                    found: seg.counter,
                });
                break;
            }
            if *size > MAX_SEGMENT_BYTES {
                issues.push(Issue::Tampered {
                    name: seg.name.clone(),
                    reason: "segment too large".into(),
                });
                break;
            }
            let bytes = match ctx.backend.get(&seg.name).await {
                Ok(b) => b,
                Err(SyncError::NotFound(_)) => {
                    // Deleted between list and get (concurrent compaction); retry next time.
                    tracing::info!(name = %seg.name, "segment vanished during sync");
                    break;
                }
                Err(e) => return Err(e),
            };
            let body = match segment::open(&ctx.key, ctx.vault_id, seg, &bytes) {
                Ok(b) => b,
                Err(e) => {
                    issues.push(Issue::Tampered {
                        name: seg.name.clone(),
                        reason: e.to_string(),
                    });
                    break;
                }
            };
            if contiguous && body.prev != head.as_ref().map(|h| h.hash.clone()) {
                issues.push(Issue::Fork {
                    device,
                    counter: seg.counter,
                });
                break;
            }
            match vault.apply_remote(&body.ops) {
                Ok(events) => report.changes.extend(events),
                Err(e) if is_data_error(&e) => {
                    issues.push(Issue::Tampered {
                        name: seg.name.clone(),
                        reason: e.to_string(),
                    });
                    break;
                }
                Err(e) => return Err(e.into()),
            }
            report.segments_applied += 1;
            report.ops_received += body.ops.len();
            if body.kind == SegmentKind::Snapshot {
                let mut coverage = BTreeMap::new();
                for (dev_hex, covered) in &body.covers {
                    let Some(dev) = parse_device(dev_hex) else {
                        continue;
                    };
                    if dev == device {
                        continue;
                    }
                    coverage.insert(dev.to_string(), covered.counter);
                    match state.head(dev) {
                        Some(cur) if cur.counter > covered.counter => {}
                        Some(cur) if cur.counter == covered.counter => {
                            if cur.hash != covered.hash {
                                issues.push(Issue::Fork {
                                    device: dev,
                                    counter: covered.counter,
                                });
                            }
                        }
                        _ => {
                            state.set_head(dev, covered.clone());
                            advanced_others = true;
                        }
                    }
                }
                state.coverage.insert(seg.name.clone(), coverage);
            }
            state.set_head(
                device,
                Head {
                    counter: seg.counter,
                    hash: file_hash(&bytes),
                    name: Some(seg.name.clone()),
                },
            );
        }
        Ok(advanced_others)
    }

    async fn read_all(
        ctx: &Ctx<'_>,
        vault: &mut Vault,
        state: &mut SyncState,
        listing: &Listing,
        report: &mut SyncReport,
    ) -> Result<Vec<Issue>, SyncError> {
        let mut issues = Vec::new();
        // A snapshot of one device can fill gaps of another, so repeat until stable.
        for _ in 0..=listing.by_device.len() {
            issues.clear();
            let mut again = false;
            for (device, segs) in &listing.by_device {
                again |= Self::read_device(ctx, vault, state, *device, segs, report, &mut issues)
                    .await?;
            }
            if !again {
                break;
            }
        }
        // Rollback detection.
        state
            .coverage
            .retain(|name, _| listing.names.contains(name));
        for (dev_hex, head) in &state.heads {
            let Some(device) = parse_device(dev_hex) else {
                continue;
            };
            let visible = listing.max_counter(device);
            let covered = state
                .coverage
                .values()
                .filter_map(|m| m.get(dev_hex).copied())
                .max()
                .unwrap_or(0);
            if visible.max(covered) < head.counter {
                issues.push(Issue::Rollback {
                    device,
                    expected: head.counter,
                    visible,
                });
            }
        }
        Ok(issues)
    }

    async fn write_segment(
        ctx: &Ctx<'_>,
        vault: &Vault,
        state: &mut SyncState,
        kind: SegmentKind,
        ops: Vec<SealedOp>,
    ) -> Result<Uploaded, SyncError> {
        let own = vault.device_id();
        let head = state.head(own).cloned();
        let counter = head.as_ref().map_or(0, |h| h.counter) + 1;
        let covers: BTreeMap<String, Head> = match kind {
            SegmentKind::Delta => BTreeMap::new(),
            SegmentKind::Snapshot => state
                .heads
                .iter()
                .filter(|(k, _)| **k != own.to_string())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        };
        let name = SegmentName::new(ctx.vault_id, own, counter, kind)?;
        let n_ops = ops.len();
        let body = SegmentBody {
            format: SEGMENT_FORMAT,
            vault_id: ctx.vault_id,
            device: own,
            counter,
            kind,
            prev: head.map(|h| h.hash),
            covers,
            created_at: now_ms(),
            ops,
        };
        let bytes = segment::seal(&ctx.key, &name, &body)?;
        ctx.backend
            .put(&name.name, &bytes, Some(Precondition::IfNoneMatch))
            .await?;
        if kind == SegmentKind::Snapshot {
            state.coverage.insert(
                name.name.clone(),
                body.covers
                    .iter()
                    .map(|(k, v)| (k.clone(), v.counter))
                    .collect(),
            );
        }
        state.set_head(
            own,
            Head {
                counter,
                hash: file_hash(&bytes),
                name: Some(name.name.clone()),
            },
        );
        Ok(Uploaded {
            name: name.name,
            kind,
            counter,
            ops: n_ops,
        })
    }

    /// Syncs `vault` through `backend`, updating `state`. The caller persists `state`
    /// afterwards (e.g. with [`SyncState::save`]) — also after an error, since the state is
    /// only ever advanced past work that has been completed.
    pub async fn sync(
        &self,
        vault: &mut Vault,
        backend: &dyn SyncBackend,
        state: &mut SyncState,
    ) -> Result<SyncReport, SyncError> {
        let vault_id = vault.vault_id();
        state.check_vault(vault_id)?;
        let vk = vault_key(vault)?;
        let ctx = Ctx {
            key: subkey(&vk, vault_id, Purpose::Segment),
            vault_id,
            backend,
        };
        drop(vk);
        let mut report = SyncReport::default();
        let listing = Self::list(&ctx, &mut report).await?;
        let own = vault.device_id();

        if self.options.download {
            report.issues = Self::read_all(&ctx, vault, state, &listing, &mut report).await?;
        } else {
            // Even without downloading, adopt our own segments we do not know yet so that we
            // never write a second segment at the same position.
            let mut issues = Vec::new();
            if let Some(segs) = listing.by_device.get(&own) {
                Self::read_device(&ctx, vault, state, own, segs, &mut report, &mut issues).await?;
            }
            report.issues = issues;
        }

        if self.options.upload {
            let own_rollback = report
                .issues
                .iter()
                .any(|i| matches!(i, Issue::Rollback { device, .. } if *device == own));
            let own_blocked = report
                .issues
                .iter()
                .any(|i| i.device() == Some(own) && !matches!(i, Issue::Rollback { .. }));
            if own_blocked {
                report.upload_blocked = true;
            } else {
                let (ops, cursor) = vault.local_ops_since(state.cursor)?;
                if own_rollback {
                    // Re-seed the storage with everything we have.
                    let all = vault.all_syncable_ops()?;
                    report.uploaded = Some(
                        Self::write_segment(&ctx, vault, state, SegmentKind::Snapshot, all).await?,
                    );
                    state.cursor = cursor;
                } else if !ops.is_empty() {
                    report.uploaded = Some(
                        Self::write_segment(&ctx, vault, state, SegmentKind::Delta, ops).await?,
                    );
                    state.cursor = cursor;
                }
            }
        }
        state.last_sync_at = Some(now_ms());
        Ok(report)
    }

    /// Syncs, then writes a snapshot and deletes the segments it supersedes (see the module
    /// docs for the exact rules).
    pub async fn compact(
        &self,
        vault: &mut Vault,
        backend: &dyn SyncBackend,
        state: &mut SyncState,
        options: CompactOptions,
    ) -> Result<CompactReport, SyncError> {
        let full = Self::new(SyncOptions::default());
        let sync = full.sync(vault, backend, state).await?;
        let own = vault.device_id();
        let mut out = CompactReport {
            sync,
            ..CompactReport::default()
        };
        if out.sync.upload_blocked || out.sync.issues.iter().any(|i| i.device() == Some(own)) {
            return Ok(out);
        }
        let vault_id = vault.vault_id();
        let vk = vault_key(vault)?;
        let ctx = Ctx {
            key: subkey(&vk, vault_id, Purpose::Segment),
            vault_id,
            backend,
        };
        drop(vk);
        let all = vault.all_syncable_ops()?;
        let (_, cursor) = vault.local_ops_since(state.cursor)?;
        let snap = Self::write_segment(&ctx, vault, state, SegmentKind::Snapshot, all).await?;
        state.cursor = cursor;
        let covered: BTreeMap<String, u64> =
            state.coverage.get(&snap.name).cloned().unwrap_or_default();

        let mut scratch = SyncReport::default();
        let listing = Self::list(&ctx, &mut scratch).await?;
        let prune_foreign = options.prune_foreign && out.sync.issues.is_empty();
        for (device, segs) in &listing.by_device {
            for (seg, _) in segs {
                let delete = if *device == own {
                    seg.counter < snap.counter
                } else {
                    prune_foreign
                        && covered
                            .get(&device.to_string())
                            .is_some_and(|c| seg.counter <= *c)
                };
                if delete {
                    backend.delete(&seg.name).await?;
                    out.deleted.push(seg.name.clone());
                }
            }
        }
        state
            .coverage
            .retain(|name, _| !out.deleted.iter().any(|d| d == name));
        out.snapshot = Some(snap);
        Ok(out)
    }
}
