// SPDX-License-Identifier: GPL-3.0-or-later
//! Multi-device sync through the folder backend, including attacks by the storage.

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::many_single_char_names)]

mod common;

use common::{add_host, labels, new_vault, replica};
use core_model::Host;
use std::path::{Path, PathBuf};
use sync::backends::FolderBackend;
use sync::segment::{SegmentKind, SegmentName, segments_prefix};
use sync::{CompactOptions, Issue, SyncBackend, SyncEngine, SyncState};
use vault::Vault;

struct Device {
    vault: Vault,
    state: SyncState,
}

impl Device {
    fn new(vault: Vault) -> Self {
        let state = SyncState::new(vault.vault_id());
        Self { vault, state }
    }

    async fn sync(&mut self, b: &FolderBackend) -> sync::SyncReport {
        SyncEngine::default()
            .sync(&mut self.vault, b, &mut self.state)
            .await
            .unwrap()
    }
}

/// Paths of all segment files of `device`, sorted by counter.
async fn segment_files(
    b: &FolderBackend,
    v: &Vault,
    device: &Vault,
) -> Vec<(SegmentName, PathBuf)> {
    let mut out: Vec<(SegmentName, PathBuf)> = b
        .list(&segments_prefix(v.vault_id()))
        .await
        .unwrap()
        .into_iter()
        .filter_map(|o| SegmentName::parse(v.vault_id(), &o.name))
        .filter(|s| s.device == device.device_id())
        .map(|s| {
            let p = b.root().join(&s.name);
            (s, p)
        })
        .collect();
    out.sort_by_key(|(s, _)| s.counter);
    out
}

fn setup() -> (tempfile::TempDir, FolderBackend) {
    let dir = tempfile::tempdir().unwrap();
    let backend = FolderBackend::new(dir.path().join("storage"));
    (dir, backend)
}

fn dir(t: &tempfile::TempDir) -> &Path {
    t.path()
}

#[tokio::test]
async fn three_devices_with_concurrent_edits_converge() {
    let (tmp, b) = setup();
    let mut a = Device::new(new_vault(dir(&tmp), "a.db"));
    let mut web = add_host(&mut a.vault, "web");
    let r = a.sync(&b).await;
    assert!(r.is_clean());
    assert_eq!(r.uploaded.as_ref().unwrap().counter, 1);

    let mut bdev = Device::new(replica(&a.vault, dir(&tmp), "b.db"));
    let mut c = Device::new(replica(&a.vault, dir(&tmp), "c.db"));
    let r = bdev.sync(&b).await;
    assert!(r.is_clean(), "{:?}", r.issues);
    assert_eq!(labels(&bdev.vault), vec!["web"]);
    assert!(r.uploaded.is_none(), "nothing local to upload");
    c.sync(&b).await;

    // Concurrent edits on all three devices, including the same record.
    web.config.ssh.port = Some(2222);
    a.vault.put(&web).unwrap();
    let mut web_b: Host = bdev.vault.get(&web.id.to_string()).unwrap().unwrap();
    web_b.tags = vec!["prod".into()];
    bdev.vault.put(&web_b).unwrap();
    add_host(&mut bdev.vault, "db");
    add_host(&mut c.vault, "cache");

    for _ in 0..2 {
        a.sync(&b).await;
        bdev.sync(&b).await;
        c.sync(&b).await;
    }
    let ha: Vec<Host> = a.vault.list().unwrap();
    for d in [&bdev, &c] {
        let mut x: Vec<Host> = d.vault.list().unwrap();
        let mut y = ha.clone();
        x.sort_by_key(|h| h.id);
        y.sort_by_key(|h| h.id);
        assert_eq!(x, y);
    }
    assert_eq!(labels(&a.vault), vec!["cache", "db", "web"]);
    let merged: Host = c.vault.get(&web.id.to_string()).unwrap().unwrap();
    assert_eq!(merged.config.ssh.port, Some(2222));
    assert_eq!(merged.tags, vec!["prod".to_owned()]);

    // The storage holds no plaintext.
    for (_, p) in segment_files(&b, &a.vault, &bdev.vault).await {
        let raw = std::fs::read(p).unwrap();
        assert!(!raw.windows(5).any(|w| w == b"cache" || w == b"10.0."));
    }
}

#[tokio::test]
async fn state_roundtrips_through_device_local_settings() {
    let (tmp, b) = setup();
    let mut a = Device::new(new_vault(dir(&tmp), "a.db"));
    add_host(&mut a.vault, "web");
    a.sync(&b).await;
    a.state.save(&mut a.vault, "folder-1").unwrap();
    let loaded = SyncState::load(&a.vault, "folder-1").unwrap();
    assert_eq!(loaded, a.state);
    // Saving the state must not create replicated ops.
    let (ops, _) = a.vault.local_ops_since(a.state.cursor).unwrap();
    assert!(ops.is_empty());
    assert_eq!(
        SyncState::load(&a.vault, "other").unwrap(),
        SyncState::new(a.vault.vault_id())
    );
    // A state of another vault is refused.
    let mut other = Device::new(new_vault(dir(&tmp), "o.db"));
    let err = SyncEngine::default()
        .sync(&mut other.vault, &b, &mut a.state.clone())
        .await
        .unwrap_err();
    assert!(matches!(err, sync::SyncError::StateMismatch { .. }));
}

#[tokio::test]
async fn tampered_and_renamed_segments_are_rejected() {
    let (tmp, b) = setup();
    let mut a = Device::new(new_vault(dir(&tmp), "a.db"));
    add_host(&mut a.vault, "one");
    a.sync(&b).await;
    let mut bdev = Device::new(replica(&a.vault, dir(&tmp), "b.db"));

    let files = segment_files(&b, &a.vault, &a.vault).await;
    let (seg, path) = &files[0];
    let original = std::fs::read(path).unwrap();
    let mut bytes = original.clone();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0x01;
    std::fs::write(path, &bytes).unwrap();

    let r = bdev.sync(&b).await;
    assert!(
        matches!(&r.issues[..], [Issue::Tampered { name, .. }] if *name == seg.name),
        "{:?}",
        r.issues
    );
    assert!(labels(&bdev.vault).is_empty(), "nothing was applied");
    assert!(bdev.state.head(a.vault.device_id()).is_none());

    // A valid segment moved to another position fails authentication too.
    std::fs::write(path, &original).unwrap();
    let moved = SegmentName::new(
        a.vault.vault_id(),
        a.vault.device_id(),
        2,
        SegmentKind::Delta,
    )
    .unwrap();
    b.put(&moved.name, &original, None).await.unwrap();
    let r = bdev.sync(&b).await;
    assert_eq!(labels(&bdev.vault), vec!["one"], "segment 1 is fine again");
    assert!(
        matches!(&r.issues[..], [Issue::Tampered { name, .. }] if *name == moved.name),
        "{:?}",
        r.issues
    );
}

#[tokio::test]
async fn truncation_and_rollback_are_detected() {
    let (tmp, b) = setup();
    let mut a = Device::new(new_vault(dir(&tmp), "a.db"));
    let mut bdev = Device::new(replica(&a.vault, dir(&tmp), "b.db"));
    add_host(&mut a.vault, "one");
    a.sync(&b).await;
    bdev.sync(&b).await;
    add_host(&mut a.vault, "two");
    a.sync(&b).await;
    add_host(&mut a.vault, "three");
    a.sync(&b).await;

    let files = segment_files(&b, &a.vault, &a.vault).await;
    assert_eq!(files.len(), 3);
    let seg2 = std::fs::read(&files[1].1).unwrap();
    std::fs::remove_file(&files[1].1).unwrap();

    // Truncation in the middle of A's chain: B stops before the gap.
    let r = bdev.sync(&b).await;
    assert_eq!(
        r.issues,
        vec![Issue::Gap {
            device: a.vault.device_id(),
            expected: 2,
            found: 3
        }]
    );
    assert_eq!(labels(&bdev.vault), vec!["one"]);

    // Once the file reappears (e.g. it was just slow to propagate) everything is applied.
    std::fs::write(&files[1].1, &seg2).unwrap();
    let r = bdev.sync(&b).await;
    assert!(r.is_clean(), "{:?}", r.issues);
    assert_eq!(labels(&bdev.vault), vec!["one", "three", "two"]);

    // Rollback: the storage hides A's newest segment from a device that has seen it.
    std::fs::remove_file(&files[2].1).unwrap();
    let r = bdev.sync(&b).await;
    assert_eq!(
        r.issues,
        vec![Issue::Rollback {
            device: a.vault.device_id(),
            expected: 3,
            visible: 2
        }]
    );

    // A notices that its own uploads were rolled back and re-seeds with a snapshot.
    let r = a.sync(&b).await;
    assert!(r.issues.iter().any(|i| matches!(i, Issue::Rollback { .. })));
    let up = r.uploaded.unwrap();
    assert_eq!((up.kind, up.counter), (SegmentKind::Snapshot, 4));
    let r = bdev.sync(&b).await;
    assert!(r.is_clean(), "{:?}", r.issues);
}

#[tokio::test]
async fn compaction_keeps_data_and_prunes_safely() {
    let (tmp, b) = setup();
    let mut a = Device::new(new_vault(dir(&tmp), "a.db"));
    let mut bdev = Device::new(replica(&a.vault, dir(&tmp), "b.db"));
    for i in 0..3 {
        add_host(&mut a.vault, &format!("a{i}"));
        a.sync(&b).await;
        add_host(&mut bdev.vault, &format!("b{i}"));
        bdev.sync(&b).await;
    }
    a.sync(&b).await;
    let mut c = Device::new(replica(&a.vault, dir(&tmp), "c.db"));
    c.sync(&b).await;
    assert_eq!(segment_files(&b, &a.vault, &a.vault).await.len(), 3);

    // Own-only compaction.
    let r = SyncEngine::default()
        .compact(&mut a.vault, &b, &mut a.state, CompactOptions::default())
        .await
        .unwrap();
    let snap = r.snapshot.unwrap();
    assert_eq!(snap.kind, SegmentKind::Snapshot);
    assert_eq!(r.deleted.len(), 3);
    let files = segment_files(&b, &a.vault, &a.vault).await;
    assert_eq!(files.len(), 1);
    assert_eq!(segment_files(&b, &a.vault, &bdev.vault).await.len(), 3);

    // A device that was in sync before, and a brand-new device, both see everything.
    let r = c.sync(&b).await;
    assert!(r.is_clean(), "{:?}", r.issues);
    let mut d = Device::new(replica(&a.vault, dir(&tmp), "d.db"));
    let r = d.sync(&b).await;
    assert!(r.is_clean(), "{:?}", r.issues);
    let all = vec!["a0", "a1", "a2", "b0", "b1", "b2"];
    assert_eq!(labels(&d.vault), all);

    // Pruning other devices' segments covered by a new snapshot.
    let r = SyncEngine::default()
        .compact(
            &mut a.vault,
            &b,
            &mut a.state,
            CompactOptions {
                prune_foreign: true,
            },
        )
        .await
        .unwrap();
    assert!(r.sync.is_clean());
    assert!(segment_files(&b, &a.vault, &bdev.vault).await.is_empty());
    assert_eq!(segment_files(&b, &a.vault, &a.vault).await.len(), 1);

    // B keeps working: its next segment chains onto the pruned history, and every device
    // (old and new) verifies it through the snapshot's coverage.
    add_host(&mut bdev.vault, "b3");
    let r = bdev.sync(&b).await;
    assert!(r.is_clean(), "{:?}", r.issues);
    assert_eq!(r.uploaded.unwrap().counter, 4);
    for dev in [&mut a, &mut c, &mut d] {
        let r = dev.sync(&b).await;
        assert!(r.is_clean(), "{:?}", r.issues);
        assert!(labels(&dev.vault).contains(&"b3".to_owned()));
    }
    let mut e = Device::new(replica(&a.vault, dir(&tmp), "e.db"));
    let r = e.sync(&b).await;
    assert!(r.is_clean(), "{:?}", r.issues);
    assert_eq!(labels(&e.vault).len(), 7);
}

#[tokio::test]
async fn forks_are_reported() {
    let (tmp, b) = setup();
    let mut a = Device::new(new_vault(dir(&tmp), "a.db"));
    add_host(&mut a.vault, "one");
    a.sync(&b).await;
    let stale = a.state.clone();
    add_host(&mut a.vault, "two");
    a.sync(&b).await;
    let mut bdev = Device::new(replica(&a.vault, dir(&tmp), "b.db"));
    bdev.sync(&b).await;

    // Simulate a restored backup of A: same device id, stale state, writes another #2.
    std::fs::remove_file(&segment_files(&b, &a.vault, &a.vault).await[1].1).unwrap();
    let mut restored = stale;
    add_host(&mut a.vault, "three");
    let r = SyncEngine::default()
        .sync(&mut a.vault, &b, &mut restored)
        .await
        .unwrap();
    assert_eq!(r.uploaded.unwrap().counter, 2);
    let r = bdev.sync(&b).await;
    assert!(
        r.issues.contains(&Issue::Fork {
            device: a.vault.device_id(),
            counter: 2
        }),
        "{:?}",
        r.issues
    );
}

#[tokio::test]
async fn conflict_copies_and_foreign_files_are_ignored() {
    let (tmp, b) = setup();
    let mut a = Device::new(new_vault(dir(&tmp), "a.db"));
    add_host(&mut a.vault, "one");
    a.sync(&b).await;
    let files = segment_files(&b, &a.vault, &a.vault).await;
    let p = &files[0].1;
    let copy = p.with_file_name(format!(
        "{} (conflicted copy 2026-01-01).seg",
        p.file_stem().unwrap().to_str().unwrap()
    ));
    std::fs::copy(p, copy).unwrap();
    std::fs::write(p.with_file_name("notes.txt"), b"hello").unwrap();
    let mut bdev = Device::new(replica(&a.vault, dir(&tmp), "b.db"));
    let r = bdev.sync(&b).await;
    assert!(r.is_clean(), "{:?}", r.issues);
    assert_eq!(r.ignored.len(), 1, "{:?}", r.ignored);
    assert_eq!(labels(&bdev.vault), vec!["one"]);
}
