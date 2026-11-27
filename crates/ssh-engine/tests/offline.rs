// SPDX-License-Identifier: GPL-3.0-or-later
//! Connection-level tests that need no SSH server.

#![allow(clippy::unwrap_used)]

use async_trait::async_trait;
use ssh_engine::{
    CloseCategory, ConnectOptions, Hop, HostKeyCheck, HostKeyDecision, Interaction,
    MemoryKnownHosts, SshError, connect,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;

struct Ui;

#[async_trait]
impl Interaction for Ui {
    async fn verify_host_key(&self, _: &HostKeyCheck) -> HostKeyDecision {
        HostKeyDecision::Reject
    }
}

#[tokio::test]
async fn silent_server_times_out_as_network_error() {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    // Accept and never answer.
    let _keep = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((s, _)) = l.accept().await {
            held.push(s);
        }
    });
    let mut opts = ConnectOptions::new(Hop::new("127.0.0.1", port, "u"));
    opts.connect_timeout = Duration::from_millis(500);
    let start = Instant::now();
    let err = connect(opts, Arc::new(Ui), Arc::new(MemoryKnownHosts::default()))
        .await
        .unwrap_err();
    assert!(matches!(err, SshError::Timeout(_)), "{err:?}");
    assert!(err.is_retryable());
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn refused_port_is_a_network_error() {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    drop(l);
    let opts = ConnectOptions::new(Hop::new("127.0.0.1", port, "u"));
    let err = connect(opts, Arc::new(Ui), Arc::new(MemoryKnownHosts::default()))
        .await
        .unwrap_err();
    assert_eq!(err.category(), CloseCategory::Network, "{err:?}");
}

#[tokio::test]
async fn garbage_server_is_a_protocol_error() {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        let (mut s, _) = l.accept().await.unwrap();
        let _ = s.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
    });
    let opts = ConnectOptions::new(Hop::new("127.0.0.1", port, "u"));
    let err = connect(opts, Arc::new(Ui), Arc::new(MemoryKnownHosts::default()))
        .await
        .unwrap_err();
    assert!(matches!(err, SshError::Protocol(_)), "{err:?}");
}
