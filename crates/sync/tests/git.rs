// SPDX-License-Identifier: GPL-3.0-or-later
//! Git backend on local repositories, exchanging through a file-system transport.

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used)]

mod common;

use common::{add_host, labels, new_vault, replica};
use sync::backends::{GitBackend, LocalRepoTransport};
use sync::segment::segments_prefix;
use sync::{CompactOptions, Precondition, SyncBackend, SyncEngine, SyncError, SyncState};

#[tokio::test]
async fn git_backend_operations() {
    let tmp = tempfile::tempdir().unwrap();
    let g = GitBackend::init(tmp.path().join("repo.git")).unwrap();
    assert!(g.tip().await.unwrap().is_none());
    assert!(g.list("").await.unwrap().is_empty());
    let etag = g
        .put("v/s/a.seg", b"one", Some(Precondition::IfNoneMatch))
        .await
        .unwrap();
    assert!(matches!(
        g.put("v/s/a.seg", b"two", Some(Precondition::IfNoneMatch))
            .await,
        Err(SyncError::PreconditionFailed(_))
    ));
    assert!(matches!(
        g.put(
            "v/s/a.seg",
            b"two",
            Some(Precondition::IfMatch("00".into()))
        )
        .await,
        Err(SyncError::PreconditionFailed(_))
    ));
    g.put("v/s/a.seg", b"two", Some(Precondition::IfMatch(etag)))
        .await
        .unwrap();
    g.put("v/t.seg", b"three", None).await.unwrap();
    g.put("w/u.seg", b"four", None).await.unwrap();
    let list = g.list("v/").await.unwrap();
    let names: Vec<&str> = list.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, vec!["v/s/a.seg", "v/t.seg"]);
    assert_eq!(list[0].size, 3);
    assert_eq!(g.get("v/s/a.seg").await.unwrap(), b"two");
    assert!(matches!(g.get("v/zz").await, Err(SyncError::NotFound(_))));
    g.delete("v/t.seg").await.unwrap();
    g.delete("v/t.seg").await.unwrap();
    assert_eq!(g.list("").await.unwrap().len(), 2);
    assert!(g.tip().await.unwrap().is_some());

    // Reopening sees the same data; branch names are validated.
    let again = GitBackend::open(g.path()).unwrap();
    assert_eq!(again.list("").await.unwrap().len(), 2);
    assert!(
        GitBackend::open(g.path())
            .unwrap()
            .with_branch("../x")
            .is_err()
    );
    let other = GitBackend::open(g.path())
        .unwrap()
        .with_branch("other")
        .unwrap();
    assert!(other.list("").await.unwrap().is_empty());
}

#[tokio::test]
async fn two_devices_sync_through_a_git_remote() {
    let tmp = tempfile::tempdir().unwrap();
    let remote = LocalRepoTransport::new(tmp.path().join("remote.git")).unwrap();
    let ga = GitBackend::init(tmp.path().join("a.git")).unwrap();
    let gb = GitBackend::init(tmp.path().join("b.git")).unwrap();
    let engine = SyncEngine::default();

    let mut a = new_vault(tmp.path(), "a.db");
    let mut b = replica(&a, tmp.path(), "b.db");
    let (mut sa, mut sb) = (SyncState::new(a.vault_id()), SyncState::new(b.vault_id()));

    add_host(&mut a, "a0");
    engine.sync(&mut a, &ga, &mut sa).await.unwrap();
    let r = ga.exchange(&remote).await.unwrap();
    assert!(r.pushed && !r.received);

    let r = gb.exchange(&remote).await.unwrap();
    assert!(r.received && !r.merged);
    engine.sync(&mut b, &gb, &mut sb).await.unwrap();
    assert_eq!(labels(&b), vec!["a0"]);

    // Both devices commit concurrently; histories diverge and are merged.
    add_host(&mut a, "a1");
    engine.sync(&mut a, &ga, &mut sa).await.unwrap();
    add_host(&mut b, "b1");
    engine.sync(&mut b, &gb, &mut sb).await.unwrap();
    assert!(gb.exchange(&remote).await.unwrap().pushed);
    let r = ga.exchange(&remote).await.unwrap();
    assert!(r.merged && r.pushed, "{r:?}");
    gb.exchange(&remote).await.unwrap();
    for _ in 0..2 {
        engine.sync(&mut a, &ga, &mut sa).await.unwrap();
        engine.sync(&mut b, &gb, &mut sb).await.unwrap();
    }
    assert_eq!(labels(&a), vec!["a0", "a1", "b1"]);
    assert_eq!(labels(&b), labels(&a));
    ga.exchange(&remote).await.unwrap();
    gb.exchange(&remote).await.unwrap();
    assert_eq!(ga.tip().await.unwrap(), gb.tip().await.unwrap());

    // A compacts while B makes a concurrent change: the merge keeps B's new segment and
    // still applies A's deletions.
    let before = ga.list(&segments_prefix(a.vault_id())).await.unwrap().len();
    let r = engine
        .compact(
            &mut a,
            &ga,
            &mut sa,
            CompactOptions {
                prune_foreign: true,
            },
        )
        .await
        .unwrap();
    assert!(r.sync.is_clean(), "{:?}", r.sync.issues);
    assert!(!r.deleted.is_empty());
    add_host(&mut b, "b2");
    engine.sync(&mut b, &gb, &mut sb).await.unwrap();
    gb.exchange(&remote).await.unwrap();
    let r = ga.exchange(&remote).await.unwrap();
    assert!(r.merged, "{r:?}");
    gb.exchange(&remote).await.unwrap();
    let after = gb.list(&segments_prefix(a.vault_id())).await.unwrap().len();
    assert_eq!(after, 2, "snapshot + B's new segment (was {before})");

    let r = engine.sync(&mut a, &ga, &mut sa).await.unwrap();
    assert!(r.is_clean(), "{:?}", r.issues);
    let r = engine.sync(&mut b, &gb, &mut sb).await.unwrap();
    assert!(r.is_clean(), "{:?}", r.issues);
    assert_eq!(labels(&a), vec!["a0", "a1", "b1", "b2"]);
    assert_eq!(labels(&b), labels(&a));
}
