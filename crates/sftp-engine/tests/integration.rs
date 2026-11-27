// SPDX-License-Identifier: GPL-3.0-or-later
//! SFTP integration tests against the docker OpenSSH servers (see `docker/test-servers`).
//!
//! Run with `cargo test -p sftp-engine --features integration`; skipped without docker.

#![cfg(feature = "integration")]
#![allow(clippy::print_stdout, clippy::unwrap_used)]

#[path = "../../ssh-engine/tests/support/mod.rs"]
mod support;

use sftp_engine::{
    EditSession, EntryKind, FileSystem, Local, Remote, RemoveLimits, TransferEvent,
    TransferOptions, TransferQueue, TransferRequest, TransferState, WriteMode,
};
use ssh_engine::known_hosts::parse_known_hosts_line;
use ssh_engine::{AuthMethod, ConnectOptions, Connection, MemoryKnownHosts};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use support::{ALICE_PASSWORD, Env, TestUi};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn connect_a(env: &Env) -> Connection {
    let line = format!("[{}]:{} {}", env.a.host, env.a.port, env.host_key_a());
    let kh = Arc::new(MemoryKnownHosts::new(vec![
        parse_known_hosts_line(&line).unwrap(),
    ]));
    ssh_engine::connect(env.alice_password(), Arc::new(TestUi::accept()), kh)
        .await
        .unwrap()
}

async fn connect_b_via_a(env: &Env) -> Connection {
    let mut opts = ConnectOptions::new(
        env.hop_b("alice")
            .with_auth(AuthMethod::password(ALICE_PASSWORD)),
    );
    opts.hops.insert(
        0,
        env.hop_a("alice")
            .with_auth(AuthMethod::password(ALICE_PASSWORD)),
    );
    ssh_engine::connect(
        opts,
        Arc::new(TestUi::accept()),
        Arc::new(MemoryKnownHosts::default()),
    )
    .await
    .unwrap()
}

async fn remote(conn: &Connection) -> Arc<dyn FileSystem> {
    Arc::new(Remote::new(conn.sftp().await.unwrap()))
}

fn pattern(len: usize, seed: u8) -> Vec<u8> {
    let mut x: u32 = u32::from(seed).wrapping_mul(2_654_435_761) | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}

/// Unique scratch directory in alice's home.
async fn scratch(fs: &dyn FileSystem, tag: &str) -> String {
    let home = fs.home().await.unwrap();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    let dir = fs.join(&home, &format!("{tag}-{nanos}")).unwrap();
    fs.mkdir(&dir).await.unwrap();
    dir
}

async fn write_remote(fs: &dyn FileSystem, path: &str, data: &[u8]) {
    let mut w = fs.open_write(path, WriteMode::Truncate).await.unwrap();
    w.write_all(data).await.unwrap();
    w.shutdown().await.unwrap();
}

async fn read_all(fs: &dyn FileSystem, path: &str) -> Vec<u8> {
    let mut r = fs.open_read(path, 0).await.unwrap();
    let mut v = Vec::new();
    r.read_to_end(&mut v).await.unwrap();
    v
}

#[tokio::test]
async fn listing_metadata_and_basic_operations() {
    let env = require_env!();
    let conn = connect_a(env).await;
    let fs = remote(&conn).await;
    assert_eq!(fs.home().await.unwrap(), "/home/alice");
    let dir = scratch(&*fs, "ops").await;
    write_remote(&*fs, &fs.join(&dir, "a.txt").unwrap(), b"hello").await;
    write_remote(&*fs, &fs.join(&dir, ".hidden").unwrap(), b"").await;
    fs.create_dir_all(&format!("{dir}/x/y/z")).await.unwrap();
    conn.exec(&format!(
        "ln -s a.txt {dir}/link; ln -s /nonexistent {dir}/dangling"
    ))
    .await
    .unwrap();

    let mut list = fs.list(&dir).await.unwrap();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    let names: Vec<_> = list.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec![".hidden", "a.txt", "dangling", "link", "x"]);
    let a = &list[1];
    assert_eq!((a.kind, a.size), (EntryKind::File, 5));
    assert!(a.modified.is_some());
    assert_eq!(a.uid, Some(1000));
    assert!(list[0].hidden);
    assert_eq!(list[2].kind, EntryKind::Symlink);
    assert_eq!(list[2].link_target_kind, None);
    assert_eq!(list[3].link_target_kind, Some(EntryKind::File));
    assert!(list[4].mode_string().starts_with('d'));
    assert_eq!(fs.read_link(&list[3].path).await.unwrap(), "a.txt");
    assert_eq!(fs.stat(&list[3].path).await.unwrap().kind, EntryKind::File);

    let f = fs.join(&dir, "a.txt").unwrap();
    fs.chmod(&f, 0o600).await.unwrap();
    assert_eq!(fs.stat(&f).await.unwrap().mode_string(), "-rw-------");
    let t = UNIX_EPOCH + Duration::from_secs(1_234_567_890);
    fs.set_times(&f, t, None).await.unwrap();
    assert_eq!(fs.stat(&f).await.unwrap().modified, Some(t));

    let g = fs.join(&dir, "b.txt").unwrap();
    fs.rename(&f, &g).await.unwrap();
    write_remote(&*fs, &f, b"replacement").await;
    fs.rename_replace(&f, &g).await.unwrap();
    assert_eq!(read_all(&*fs, &g).await, b"replacement");
    assert!(!fs.exists(&f).await.unwrap());
    assert!(matches!(
        fs.open_write(&g, WriteMode::CreateNew).await,
        Err(sftp_engine::FsError::AlreadyExists(_))
    ));
    assert!(matches!(
        fs.stat(&format!("{dir}/missing")).await,
        Err(sftp_engine::FsError::NotFound(_))
    ));
    fs.remove_dir_recursive(&dir, RemoveLimits::default())
        .await
        .unwrap();
    assert!(!fs.exists(&dir).await.unwrap());
}

#[tokio::test]
async fn recursive_delete_never_follows_symlinks() {
    let env = require_env!();
    let conn = connect_a(env).await;
    let fs = remote(&conn).await;
    let outside = scratch(&*fs, "outside").await;
    write_remote(&*fs, &format!("{outside}/precious"), b"keep me").await;
    let victim = scratch(&*fs, "victim").await;
    fs.create_dir_all(&format!("{victim}/a/b")).await.unwrap();
    write_remote(&*fs, &format!("{victim}/a/b/f"), b"x").await;
    conn.exec(&format!("ln -s {outside} {victim}/a/link-to-outside"))
        .await
        .unwrap();
    let stats = fs
        .remove_dir_recursive(&victim, RemoveLimits::default())
        .await
        .unwrap();
    assert_eq!((stats.files, stats.dirs), (2, 3));
    assert!(!fs.exists(&victim).await.unwrap());
    assert_eq!(
        read_all(&*fs, &format!("{outside}/precious")).await,
        b"keep me"
    );
}

#[tokio::test]
async fn upload_download_and_resume() {
    let env = require_env!();
    let conn = connect_a(env).await;
    let fs = remote(&conn).await;
    let dir = scratch(&*fs, "xfer").await;
    let tmp = tempfile::tempdir().unwrap();
    let content = pattern(5 * 1024 * 1024 + 123, 7);
    let local_src = tmp.path().join("big.bin");
    std::fs::write(&local_src, &content).unwrap();
    let mtime = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    Local
        .set_times(&local_src.to_string_lossy(), mtime, None)
        .await
        .unwrap();
    let local: Arc<dyn FileSystem> = Arc::new(Local);
    let remote_path = format!("{dir}/big.bin");

    let (q, mut rx) = TransferQueue::new(2, None);
    let up = q.enqueue(TransferRequest {
        source: local.clone(),
        source_path: local_src.to_string_lossy().into_owned(),
        destination: fs.clone(),
        destination_path: remote_path.clone(),
        options: TransferOptions {
            preserve_permissions: true,
            ..TransferOptions::default()
        },
    });
    assert_eq!(q.wait(up).await, Some(TransferState::Completed));
    let e = fs.stat(&remote_path).await.unwrap();
    assert_eq!(e.size, content.len() as u64);
    assert_eq!(e.modified, Some(mtime));
    let sum = conn
        .exec(&format!("sha256sum {remote_path} | cut -d' ' -f1"))
        .await
        .unwrap()
        .stdout_string();
    let local_sum = conn
        .exec_with(
            "sha256sum | cut -d' ' -f1",
            ssh_engine::ExecOptions {
                stdin: Some(content.clone()),
                ..ssh_engine::ExecOptions::default()
            },
        )
        .await
        .unwrap()
        .stdout_string();
    assert_eq!(sum, local_sum);

    // Download back.
    let local_dst = tmp.path().join("back.bin");
    let down = q.enqueue(TransferRequest {
        source: fs.clone(),
        source_path: remote_path.clone(),
        destination: local.clone(),
        destination_path: local_dst.to_string_lossy().into_owned(),
        options: TransferOptions::default(),
    });
    assert_eq!(q.wait(down).await, Some(TransferState::Completed));
    assert_eq!(std::fs::read(&local_dst).unwrap(), content);

    // Interrupted upload: a remote part file with the first 2 MiB is continued.
    let resumed_path = format!("{dir}/resumed.bin");
    write_remote(
        &*fs,
        &format!("{resumed_path}.sshc-part"),
        &content[..2 * 1024 * 1024],
    )
    .await;
    while rx.try_recv().is_ok() {}
    let id = q.enqueue(TransferRequest {
        source: local.clone(),
        source_path: local_src.to_string_lossy().into_owned(),
        destination: fs.clone(),
        destination_path: resumed_path.clone(),
        options: TransferOptions::default(),
    });
    assert_eq!(q.wait(id).await, Some(TransferState::Completed));
    let mut resumed = false;
    while let Ok(ev) = rx.try_recv() {
        if let TransferEvent::FileResumed { offset, .. } = ev {
            assert_eq!(offset, 2 * 1024 * 1024);
            resumed = true;
        }
    }
    assert!(resumed);
    assert_eq!(read_all(&*fs, &resumed_path).await, content);
    assert!(
        !fs.exists(&format!("{resumed_path}.sshc-part"))
            .await
            .unwrap()
    );

    // Interrupted download: same on the local side.
    let local_part = tmp.path().join("partial.bin.sshc-part");
    std::fs::write(&local_part, &content[..1_000_000]).unwrap();
    let id = q.enqueue(TransferRequest {
        source: fs.clone(),
        source_path: remote_path.clone(),
        destination: local.clone(),
        destination_path: tmp
            .path()
            .join("partial.bin")
            .to_string_lossy()
            .into_owned(),
        options: TransferOptions::default(),
    });
    assert_eq!(q.wait(id).await, Some(TransferState::Completed));
    assert_eq!(
        std::fs::read(tmp.path().join("partial.bin")).unwrap(),
        content
    );

    fs.remove_dir_recursive(&dir, RemoveLimits::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn directory_trees_and_remote_to_remote() {
    let env = require_env!();
    let conn_a = connect_a(env).await;
    let a = remote(&conn_a).await;
    let conn_b = connect_b_via_a(env).await;
    let b = remote(&conn_b).await;
    let tmp = tempfile::tempdir().unwrap();
    let tree = tmp.path().join("tree");
    std::fs::create_dir_all(tree.join("sub/deeper")).unwrap();
    std::fs::write(tree.join("one.txt"), b"1").unwrap();
    std::fs::write(tree.join("sub/two.bin"), pattern(300_000, 3)).unwrap();
    std::fs::write(tree.join("sub/deeper/three.txt"), b"three").unwrap();
    let local: Arc<dyn FileSystem> = Arc::new(Local);

    let dir_a = scratch(&*a, "tree-a").await;
    let (q, _rx) = TransferQueue::new(2, None);
    let up = q.enqueue(TransferRequest {
        source: local.clone(),
        source_path: tree.to_string_lossy().into_owned(),
        destination: a.clone(),
        destination_path: format!("{dir_a}/tree"),
        options: TransferOptions::default(),
    });
    assert_eq!(q.wait(up).await, Some(TransferState::Completed));
    assert_eq!(
        read_all(&*a, &format!("{dir_a}/tree/sub/deeper/three.txt")).await,
        b"three"
    );

    // a → b, streamed through this client (b is reached through a as jump host).
    let dir_b = scratch(&*b, "tree-b").await;
    let r2r = q.enqueue(TransferRequest {
        source: a.clone(),
        source_path: format!("{dir_a}/tree"),
        destination: b.clone(),
        destination_path: format!("{dir_b}/copy"),
        options: TransferOptions::default(),
    });
    assert_eq!(q.wait(r2r).await, Some(TransferState::Completed));
    assert_eq!(
        read_all(&*b, &format!("{dir_b}/copy/sub/two.bin")).await,
        pattern(300_000, 3)
    );
    let host_b = conn_b.exec("hostname").await.unwrap().stdout_string();
    assert_eq!(host_b.trim(), env.b.name);

    // Keep-both on an existing directory.
    let again = q.enqueue(TransferRequest {
        source: local.clone(),
        source_path: tree.to_string_lossy().into_owned(),
        destination: a.clone(),
        destination_path: format!("{dir_a}/tree"),
        options: TransferOptions {
            conflict: sftp_engine::ConflictPolicy::KeepBoth,
            ..TransferOptions::default()
        },
    });
    assert_eq!(q.wait(again).await, Some(TransferState::Completed));
    assert!(
        a.exists(&format!("{dir_a}/tree (1)/one.txt"))
            .await
            .unwrap()
    );

    a.remove_dir_recursive(&dir_a, RemoveLimits::default())
        .await
        .unwrap();
    b.remove_dir_recursive(&dir_b, RemoveLimits::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn edit_and_upload_remote_file() {
    let env = require_env!();
    let conn = connect_a(env).await;
    let fs = remote(&conn).await;
    let dir = scratch(&*fs, "edit").await;
    let path = format!("{dir}/app.conf");
    write_remote(&*fs, &path, b"port=80\n").await;
    fs.chmod(&path, 0o640).await.unwrap();

    let mut s = EditSession::open(fs.clone(), &path).await.unwrap();
    assert_eq!(std::fs::read(s.local_path()).unwrap(), b"port=80\n");
    std::fs::write(s.local_path(), b"port=8080\n").unwrap();
    assert!(s.is_modified().await.unwrap());
    s.upload().await.unwrap();
    assert_eq!(read_all(&*fs, &path).await, b"port=8080\n");
    assert_eq!(fs.stat(&path).await.unwrap().permissions, Some(0o640));

    // Concurrent remote change is detected.
    conn.exec(&format!("echo other=1 >> {path}")).await.unwrap();
    std::fs::write(s.local_path(), b"port=9090\n").unwrap();
    assert!(matches!(
        s.upload().await,
        Err(sftp_engine::FsError::RemoteChanged(_))
    ));
    s.close().unwrap();
    fs.remove_dir_recursive(&dir, RemoveLimits::default())
        .await
        .unwrap();
}
