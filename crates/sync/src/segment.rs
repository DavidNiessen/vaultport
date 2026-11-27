// SPDX-License-Identifier: GPL-3.0-or-later
//! Segment files: the unit of replication.
//!
//! # Layout
//!
//! ```text
//! vault-<vault-uuid>/segments/<device-hex16>/<first:016x>-<last:016x>-<random16>.seg
//! ```
//!
//! `last` is the segment **counter**: every device numbers its segments 1, 2, 3, ... on a
//! backend. A *delta* segment has `first == last`; a *snapshot* segment (written by
//! compaction) has `first == 0` and contains every op the device knew, so it supersedes all
//! older segments of that device. The random suffix makes names collision-free.
//!
//! # File format
//!
//! ```text
//! "SSHSEG01" || XChaCha20-Poly1305(segment key, aad, json(SegmentBody))
//! aad = "ssh-client/segment/v1|<vault-id>|<device-hex>|<counter>|<delta|snapshot>"
//! ```
//!
//! The segment key is an HKDF subkey of the vault key. The AAD is reconstructed from the
//! *file name*, so renaming a segment (to another device, counter or kind) makes it fail
//! authentication. The body repeats these fields, holds the hash of the previous segment of
//! the same device (`prev`, SHA-256 of the complete previous file) forming a per-device hash
//! chain, and the already-sealed vault ops. Snapshots additionally record which head of every
//! other device they include (`covers`).

use crate::error::SyncError;
use crate::state::Head;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use uuid::Uuid;
use vault::hlc::DeviceId;
use vault::{SealedOp, SymmetricKey};

const MAGIC: &[u8; 8] = b"SSHSEG01";
/// Body format version.
pub const SEGMENT_FORMAT: u32 = 1;

/// Kind of a segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SegmentKind {
    Delta,
    Snapshot,
}

impl SegmentKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Delta => "delta",
            Self::Snapshot => "snapshot",
        }
    }
}

/// Parsed segment file name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentName {
    pub name: String,
    pub device: DeviceId,
    pub counter: u64,
    pub kind: SegmentKind,
}

/// The root prefix of a vault on a backend.
#[must_use]
pub fn vault_prefix(vault_id: Uuid) -> String {
    format!("vault-{vault_id}/")
}

/// The prefix under which all segments of a vault live.
#[must_use]
pub fn segments_prefix(vault_id: Uuid) -> String {
    format!("vault-{vault_id}/segments/")
}

fn parse_hex16(s: &str) -> Option<u64> {
    (s.len() == 16
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()))
    .then(|| u64::from_str_radix(s, 16).ok())
    .flatten()
}

impl SegmentName {
    /// Builds a new name with a random suffix.
    pub fn new(
        vault_id: Uuid,
        device: DeviceId,
        counter: u64,
        kind: SegmentKind,
    ) -> Result<Self, SyncError> {
        let rand = hex::encode(crate::keys::random::<8>()?);
        let first = match kind {
            SegmentKind::Delta => counter,
            SegmentKind::Snapshot => 0,
        };
        Ok(Self {
            name: format!(
                "{}{device}/{first:016x}-{counter:016x}-{rand}.seg",
                segments_prefix(vault_id)
            ),
            device,
            counter,
            kind,
        })
    }

    /// Parses an object name; `None` for anything that is not a well-formed segment name of
    /// this vault.
    #[must_use]
    pub fn parse(vault_id: Uuid, name: &str) -> Option<Self> {
        let rest = name.strip_prefix(&segments_prefix(vault_id))?;
        let (dev, file) = rest.split_once('/')?;
        let device = DeviceId(parse_hex16(dev)?);
        let stem = file.strip_suffix(".seg")?;
        let mut parts = stem.split('-');
        let (Some(first), Some(last), Some(rand), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return None;
        };
        let first = parse_hex16(first)?;
        let counter = parse_hex16(last)?;
        if rand.is_empty() || rand.len() > 32 || !rand.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let kind = match first {
            0 if counter > 0 => SegmentKind::Snapshot,
            f if f == counter && counter > 0 => SegmentKind::Delta,
            _ => return None,
        };
        Some(Self {
            name: name.to_owned(),
            device,
            counter,
            kind,
        })
    }

    fn aad(&self, vault_id: Uuid) -> String {
        format!(
            "ssh-client/segment/v1|{vault_id}|{}|{}|{}",
            self.device,
            self.counter,
            self.kind.as_str()
        )
    }
}

/// Decrypted content of a segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentBody {
    pub format: u32,
    pub vault_id: Uuid,
    pub device: DeviceId,
    pub counter: u64,
    pub kind: SegmentKind,
    /// SHA-256 (hex) of the previous segment file of this device; `None` for counter 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev: Option<String>,
    /// Snapshots: the head of every *other* device whose ops are included.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub covers: BTreeMap<String, Head>,
    pub created_at: i64,
    pub ops: Vec<SealedOp>,
}

/// SHA-256 (hex) of a segment file.
#[must_use]
pub fn file_hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Serializes and encrypts a segment.
pub fn seal(
    key: &SymmetricKey,
    name: &SegmentName,
    body: &SegmentBody,
) -> Result<Vec<u8>, SyncError> {
    let json = zeroize::Zeroizing::new(
        serde_json::to_vec(body).map_err(|e| SyncError::Format(e.to_string()))?,
    );
    let sealed = key.seal(name.aad(body.vault_id).as_bytes(), &json)?;
    let mut out = Vec::with_capacity(MAGIC.len() + sealed.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// Decrypts and validates a segment against its (parsed) name.
pub fn open(
    key: &SymmetricKey,
    vault_id: Uuid,
    name: &SegmentName,
    bytes: &[u8],
) -> Result<SegmentBody, SyncError> {
    let sealed = bytes
        .strip_prefix(MAGIC.as_slice())
        .ok_or_else(|| SyncError::Format("not a segment file".into()))?;
    let plain = key.open(name.aad(vault_id).as_bytes(), sealed)?;
    let body: SegmentBody =
        serde_json::from_slice(&plain).map_err(|e| SyncError::Format(e.to_string()))?;
    if body.format != SEGMENT_FORMAT {
        return Err(SyncError::Format(format!(
            "unsupported segment format {}",
            body.format
        )));
    }
    if body.vault_id != vault_id
        || body.device != name.device
        || body.counter != name.counter
        || body.kind != name.kind
    {
        return Err(SyncError::Format(
            "segment header does not match its name".into(),
        ));
    }
    if body.kind == SegmentKind::Delta && !body.covers.is_empty() {
        return Err(SyncError::Format("delta segment with coverage".into()));
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn names_roundtrip_and_reject_garbage() {
        let vid = Uuid::new_v4();
        let dev = DeviceId(0xabc);
        let delta = SegmentName::new(vid, dev, 7, SegmentKind::Delta).unwrap();
        assert_eq!(SegmentName::parse(vid, &delta.name), Some(delta.clone()));
        let snap = SegmentName::new(vid, dev, 9, SegmentKind::Snapshot).unwrap();
        assert_eq!(
            SegmentName::parse(vid, &snap.name).unwrap().kind,
            SegmentKind::Snapshot
        );
        assert!(SegmentName::parse(Uuid::new_v4(), &delta.name).is_none());
        let p = segments_prefix(vid);
        for bad in [
            format!("{p}0000000000000abc/0000000000000002-0000000000000003-aa.seg"),
            format!("{p}0000000000000abc/0000000000000000-0000000000000000-aa.seg"),
            format!("{p}abc/0000000000000001-0000000000000001-aa.seg"),
            format!("{p}0000000000000abc/0000000000000001-0000000000000001-aa.tmp"),
            format!("{p}0000000000000abc/0000000000000001-0000000000000001.seg"),
        ] {
            assert!(SegmentName::parse(vid, &bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn seal_open_and_rename_detection() {
        let v = Uuid::new_v4();
        let key = SymmetricKey::generate().unwrap();
        let n = SegmentName::new(v, DeviceId(1), 1, SegmentKind::Delta).unwrap();
        let body = SegmentBody {
            format: SEGMENT_FORMAT,
            vault_id: v,
            device: DeviceId(1),
            counter: 1,
            kind: SegmentKind::Delta,
            prev: None,
            covers: BTreeMap::new(),
            created_at: 0,
            ops: vec![],
        };
        let bytes = seal(&key, &n, &body).unwrap();
        assert_eq!(open(&key, v, &n, &bytes).unwrap().counter, 1);
        let renamed = SegmentName::new(v, DeviceId(1), 2, SegmentKind::Delta).unwrap();
        assert!(matches!(
            open(&key, v, &renamed, &bytes),
            Err(SyncError::Authentication)
        ));
        let mut tampered = bytes.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        assert!(open(&key, v, &n, &tampered).is_err());
        assert!(open(&key, v, &n, b"garbage").is_err());
    }
}
