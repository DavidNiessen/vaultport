// SPDX-License-Identifier: GPL-3.0-or-later
//! Transfer queue tests on the local file system.

#![allow(clippy::many_single_char_names)]

use super::*;
use crate::fs::{FileReader, FileWriter, FsKind};
use crate::local::Local;
use std::path::Path;
use std::sync::atomic::AtomicUsize;
use std::time::SystemTime;

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn data(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 31 % 251) as u8).collect()
}

fn local() -> Arc<dyn FileSystem> {
    Arc::new(Local)
}

fn req(src: &Path, dst: &Path, options: TransferOptions) -> TransferRequest {
    TransferRequest {
        source: local(),
        source_path: s(src),
        destination: local(),
        destination_path: s(dst),
        options,
    }
}

fn drain(rx: &mut mpsc::UnboundedReceiver<TransferEvent>) -> Vec<TransferEvent> {
    let mut v = Vec::new();
    while let Ok(e) = rx.try_recv() {
        v.push(e);
    }
    v
}

#[tokio::test]
async fn single_file_with_progress_and_mtime() {
    let d = tempfile::tempdir().expect("tmp");
    let src = d.path().join("src.bin");
    let dst = d.path().join("dst.bin");
    std::fs::write(&src, data(1_000_000)).expect("write");
    let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_500_000_000);
    Local.set_times(&s(&src), t, None).await.expect("times");

    let (q, mut rx) = TransferQueue::new(2, None);
    let id = q.enqueue(req(
        &src,
        &dst,
        TransferOptions {
            chunk_size: 64 * 1024,
            ..TransferOptions::default()
        },
    ));
    assert_eq!(q.wait(id).await, Some(TransferState::Completed));
    assert_eq!(std::fs::read(&dst).expect("read"), data(1_000_000));
    assert_eq!(
        std::fs::metadata(&dst)
            .expect("meta")
            .modified()
            .expect("mtime"),
        t
    );
    assert!(!d.path().join("dst.bin.sshc-part").exists());

    let events = drain(&mut rx);
    assert!(matches!(events[0], TransferEvent::Queued { .. }));
    assert!(events.iter().any(|e| matches!(
        e,
        TransferEvent::Started {
            bytes_total: 1_000_000,
            files_total: 1,
            ..
        }
    )));
    let last_progress = events
        .iter()
        .filter_map(|e| match e {
            TransferEvent::Progress(p) => Some(p.clone()),
            _ => None,
        })
        .next_back()
        .expect("progress");
    assert_eq!(last_progress.bytes_done, 1_000_000);
    assert_eq!(last_progress.files_done, 1);
    assert!(matches!(
        events.last(),
        Some(TransferEvent::Completed {
            bytes: 1_000_000,
            files: 1,
            ..
        })
    ));
}

#[tokio::test]
async fn directory_recursion() {
    let d = tempfile::tempdir().expect("tmp");
    let src = d.path().join("tree");
    std::fs::create_dir_all(src.join("a/b/c")).expect("mkdir");
    std::fs::create_dir_all(src.join("empty")).expect("mkdir");
    std::fs::write(src.join("top.txt"), b"top").expect("w");
    std::fs::write(src.join("a/one.txt"), b"one").expect("w");
    std::fs::write(src.join("a/b/c/deep.txt"), data(70_000)).expect("w");
    #[cfg(unix)]
    std::os::unix::fs::symlink("/etc", src.join("link")).expect("symlink");
    let dst = d.path().join("copy");

    let (q, mut rx) = TransferQueue::new(1, None);
    let id = q.enqueue(req(&src, &dst, TransferOptions::default()));
    assert_eq!(q.wait(id).await, Some(TransferState::Completed));
    assert_eq!(std::fs::read(dst.join("top.txt")).expect("r"), b"top");
    assert_eq!(std::fs::read(dst.join("a/one.txt")).expect("r"), b"one");
    assert_eq!(
        std::fs::read(dst.join("a/b/c/deep.txt")).expect("r"),
        data(70_000)
    );
    assert!(dst.join("empty").is_dir());
    assert!(!dst.join("link").exists());
    let events = drain(&mut rx);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransferEvent::Completed { files: 3, .. }))
    );
    #[cfg(unix)]
    assert!(events.iter().any(|e| matches!(
        e,
        TransferEvent::FileSkipped { reason, .. } if reason == "symbolic link"
    )));
}

#[tokio::test]
async fn conflict_policies() {
    let d = tempfile::tempdir().expect("tmp");
    let src = d.path().join("f.txt");
    let dst = d.path().join("out.txt");
    std::fs::write(&src, b"new").expect("w");

    for (policy, expect_dst, expect_copy) in [
        (ConflictPolicy::Skip, "old", None),
        (ConflictPolicy::KeepBoth, "old", Some("out (1).txt")),
        (ConflictPolicy::Overwrite, "new", None),
        // Ask without a resolver behaves like Skip.
        (ConflictPolicy::Ask, "old", None),
    ] {
        std::fs::write(&dst, b"old").expect("w");
        let _ = std::fs::remove_file(d.path().join("out (1).txt"));
        let (q, _rx) = TransferQueue::new(1, None);
        let id = q.enqueue(req(
            &src,
            &dst,
            TransferOptions {
                conflict: policy,
                ..TransferOptions::default()
            },
        ));
        assert_eq!(
            q.wait(id).await,
            Some(TransferState::Completed),
            "{policy:?}"
        );
        assert_eq!(
            std::fs::read(&dst).expect("r"),
            expect_dst.as_bytes(),
            "{policy:?}"
        );
        if let Some(copy) = expect_copy {
            assert_eq!(std::fs::read(d.path().join(copy)).expect("r"), b"new");
        }
    }
}

struct Resolver {
    calls: AtomicUsize,
    decision: ConflictDecision,
}

#[async_trait]
impl ConflictResolver for Resolver {
    async fn resolve(&self, c: &Conflict) -> (ConflictDecision, bool) {
        assert_eq!(c.destination.size, 3);
        self.calls.fetch_add(1, Ordering::SeqCst);
        (self.decision, true)
    }
}

#[tokio::test]
async fn ask_resolver_apply_to_all() {
    let d = tempfile::tempdir().expect("tmp");
    let src = d.path().join("src");
    let dst = d.path().join("dst");
    std::fs::create_dir_all(&src).expect("mkdir");
    std::fs::create_dir_all(&dst).expect("mkdir");
    for n in ["a", "b", "c"] {
        std::fs::write(src.join(n), b"new!").expect("w");
        std::fs::write(dst.join(n), b"old").expect("w");
    }
    let r = Arc::new(Resolver {
        calls: AtomicUsize::new(0),
        decision: ConflictDecision::Overwrite,
    });
    let (q, _rx) = TransferQueue::new(1, Some(r.clone()));
    let id = q.enqueue(req(&src, &dst, TransferOptions::default()));
    assert_eq!(q.wait(id).await, Some(TransferState::Completed));
    assert_eq!(
        r.calls.load(Ordering::SeqCst),
        1,
        "apply-to-all must ask once"
    );
    for n in ["a", "b", "c"] {
        assert_eq!(std::fs::read(dst.join(n)).expect("r"), b"new!");
    }
    // Cancel decision cancels the transfer.
    for n in ["a", "b", "c"] {
        std::fs::write(dst.join(n), b"old").expect("w");
    }
    let r = Arc::new(Resolver {
        calls: AtomicUsize::new(0),
        decision: ConflictDecision::Cancel,
    });
    let (q, _rx) = TransferQueue::new(1, Some(r));
    let id = q.enqueue(req(&src, &dst, TransferOptions::default()));
    assert_eq!(q.wait(id).await, Some(TransferState::Cancelled));
}

#[tokio::test]
async fn resumes_verified_part_files_only() {
    let d = tempfile::tempdir().expect("tmp");
    let src = d.path().join("big.bin");
    let dst = d.path().join("big.copy");
    let part = d.path().join("big.copy.sshc-part");
    let content = data(300_000);
    std::fs::write(&src, &content).expect("w");

    // A matching prefix is continued.
    std::fs::write(&part, &content[..120_000]).expect("w");
    let (q, mut rx) = TransferQueue::new(1, None);
    let id = q.enqueue(req(&src, &dst, TransferOptions::default()));
    assert_eq!(q.wait(id).await, Some(TransferState::Completed));
    assert_eq!(std::fs::read(&dst).expect("r"), content);
    let events = drain(&mut rx);
    assert!(events.iter().any(|e| matches!(
        e,
        TransferEvent::FileResumed {
            offset: 120_000,
            ..
        }
    )));

    // A part file that does not match the source is rewritten from scratch.
    std::fs::remove_file(&dst).expect("rm");
    let mut wrong = content[..120_000].to_vec();
    wrong[119_999] ^= 0xff;
    std::fs::write(&part, &wrong).expect("w");
    let (q, mut rx) = TransferQueue::new(1, None);
    let id = q.enqueue(req(&src, &dst, TransferOptions::default()));
    assert_eq!(q.wait(id).await, Some(TransferState::Completed));
    assert_eq!(std::fs::read(&dst).expect("r"), content);
    assert!(
        !drain(&mut rx)
            .iter()
            .any(|e| matches!(e, TransferEvent::FileResumed { .. }))
    );

    // Resume disabled: always from scratch.
    std::fs::remove_file(&dst).expect("rm");
    std::fs::write(&part, &content[..1000]).expect("w");
    let (q, mut rx) = TransferQueue::new(1, None);
    let id = q.enqueue(req(
        &src,
        &dst,
        TransferOptions {
            resume: false,
            ..TransferOptions::default()
        },
    ));
    assert_eq!(q.wait(id).await, Some(TransferState::Completed));
    assert_eq!(std::fs::read(&dst).expect("r"), content);
    assert!(
        !drain(&mut rx)
            .iter()
            .any(|e| matches!(e, TransferEvent::FileResumed { .. }))
    );
}

#[tokio::test]
async fn pause_resume_cancel_and_concurrency_limit() {
    let d = tempfile::tempdir().expect("tmp");
    let src = d.path().join("f.bin");
    std::fs::write(&src, data(200_000)).expect("w");
    let (q, mut rx) = TransferQueue::new(1, None);

    let a = q.enqueue(req(
        &src,
        &d.path().join("a.bin"),
        TransferOptions::default(),
    ));
    assert!(q.pause(a));
    let b = q.enqueue(req(
        &src,
        &d.path().join("b.bin"),
        TransferOptions::default(),
    ));
    // `a` holds the only slot while paused; `b` waits.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(q.state(a), Some(TransferState::Paused));
    assert_eq!(q.state(b), Some(TransferState::Queued));
    assert!(q.resume(a));
    assert_eq!(q.wait(a).await, Some(TransferState::Completed));
    assert_eq!(q.wait(b).await, Some(TransferState::Completed));
    assert!(!q.pause(a), "finished transfers cannot be paused");
    let events = drain(&mut rx);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransferEvent::Paused { id } if *id == a))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransferEvent::Resumed { id } if *id == a))
    );

    // Cancel while paused.
    let c = q.enqueue(req(
        &src,
        &d.path().join("c.bin"),
        TransferOptions::default(),
    ));
    assert!(q.pause(c));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(q.cancel(c));
    assert_eq!(q.wait(c).await, Some(TransferState::Cancelled));
    assert!(!d.path().join("c.bin").exists());
    q.prune_finished();
    assert_eq!(q.state(a), None);
}

#[tokio::test]
async fn missing_source_fails() {
    let d = tempfile::tempdir().expect("tmp");
    let (q, mut rx) = TransferQueue::new(1, None);
    let id = q.enqueue(req(
        &d.path().join("nope"),
        &d.path().join("x"),
        TransferOptions::default(),
    ));
    assert!(matches!(q.wait(id).await, Some(TransferState::Failed(_))));
    assert!(
        drain(&mut rx)
            .iter()
            .any(|e| matches!(e, TransferEvent::Failed { .. }))
    );
}

/// A "server" that answers directory listings with traversal names.
#[derive(Debug)]
struct EvilFs;

#[async_trait]
impl FileSystem for EvilFs {
    fn kind(&self) -> FsKind {
        FsKind::Remote
    }
    fn join(&self, dir: &str, name: &str) -> Result<String, FsError> {
        // A careless implementation that trusts names.
        Ok(format!("{dir}/{name}"))
    }
    fn parent(&self, path: &str) -> Option<String> {
        Local.parent(path)
    }
    fn file_name(&self, path: &str) -> Option<String> {
        Local.file_name(path)
    }
    async fn list(&self, path: &str) -> Result<Vec<Entry>, FsError> {
        let mut v = Local.list(path).await?;
        for evil in [
            "../escape.txt",
            "../../escape2.txt",
            "/tmp/abs.txt",
            "..",
            "a\\..\\b",
        ] {
            let mut e = v[0].clone();
            e.name = evil.to_owned();
            v.push(e);
        }
        Ok(v)
    }
    async fn stat(&self, path: &str) -> Result<Entry, FsError> {
        Local.stat(path).await
    }
    async fn lstat(&self, path: &str) -> Result<Entry, FsError> {
        Local.lstat(path).await
    }
    async fn canonicalize(&self, path: &str) -> Result<String, FsError> {
        Local.canonicalize(path).await
    }
    async fn home(&self) -> Result<String, FsError> {
        Local.home().await
    }
    async fn mkdir(&self, path: &str) -> Result<(), FsError> {
        Local.mkdir(path).await
    }
    async fn remove_file(&self, path: &str) -> Result<(), FsError> {
        Local.remove_file(path).await
    }
    async fn remove_dir(&self, path: &str) -> Result<(), FsError> {
        Local.remove_dir(path).await
    }
    async fn rename(&self, from: &str, to: &str) -> Result<(), FsError> {
        Local.rename(from, to).await
    }
    async fn chmod(&self, path: &str, mode: u32) -> Result<(), FsError> {
        Local.chmod(path, mode).await
    }
    async fn set_times(
        &self,
        path: &str,
        modified: SystemTime,
        accessed: Option<SystemTime>,
    ) -> Result<(), FsError> {
        Local.set_times(path, modified, accessed).await
    }
    async fn read_link(&self, path: &str) -> Result<String, FsError> {
        Local.read_link(path).await
    }
    async fn open_read(&self, path: &str, offset: u64) -> Result<FileReader, FsError> {
        Local.open_read(path, offset).await
    }
    async fn open_write(&self, path: &str, mode: WriteMode) -> Result<FileWriter, FsError> {
        Local.open_write(path, mode).await
    }
}

#[tokio::test]
async fn malicious_entry_names_never_escape_the_destination() {
    let d = tempfile::tempdir().expect("tmp");
    let remote_dir = d.path().join("remote/dir");
    std::fs::create_dir_all(&remote_dir).expect("mkdir");
    std::fs::write(remote_dir.join("legit.txt"), b"ok").expect("w");
    let downloads = d.path().join("downloads/a/b");
    std::fs::create_dir_all(&downloads).expect("mkdir");

    let (q, mut rx) = TransferQueue::new(1, None);
    let id = q.enqueue(TransferRequest {
        source: Arc::new(EvilFs),
        source_path: s(&remote_dir),
        destination: local(),
        destination_path: s(&downloads.join("dir")),
        options: TransferOptions::default(),
    });
    assert_eq!(q.wait(id).await, Some(TransferState::Completed));
    assert_eq!(
        std::fs::read(downloads.join("dir/legit.txt")).expect("r"),
        b"ok"
    );
    assert!(!downloads.join("escape.txt").exists());
    assert!(!d.path().join("downloads/a/escape2.txt").exists());
    let skipped: Vec<String> = drain(&mut rx)
        .into_iter()
        .filter_map(|e| match e {
            TransferEvent::FileSkipped { path, reason, .. } => Some(format!("{path}: {reason}")),
            _ => None,
        })
        .collect();
    assert_eq!(skipped.len(), 5, "{skipped:?}");
    assert!(
        skipped.iter().all(|s| s.contains("unsafe path")),
        "{skipped:?}"
    );
}

#[test]
fn rate_meter() {
    let mut m = RateMeter::new();
    let t = Instant::now();
    assert!(m.add(t, 0).abs() < f64::EPSILON);
    let r = m.add(t + Duration::from_secs(2), 2_000_000);
    assert!((r - 1_000_000.0).abs() < 1.0);
}
