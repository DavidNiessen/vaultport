// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used)]

use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use tokio::time::timeout;

use super::*;
use crate::consts::{DO, IAC, SB, SE, WILL};

const WAIT: Duration = Duration::from_secs(10);
const NAWS: u8 = 31;
const ECHO: u8 = 1;

async fn next_event(events: &mut SessionEvents) -> Event {
    timeout(WAIT, events.recv()).await.unwrap().unwrap()
}

/// Reads from the fake server's side until `needle` has been seen.
async fn read_until<R: tokio::io::AsyncRead + Unpin>(r: &mut R, seen: &mut Vec<u8>, needle: &[u8]) {
    timeout(WAIT, async {
        let mut buf = [0u8; 1024];
        while !seen.windows(needle.len()).any(|w| w == needle) {
            let n = r.read(&mut buf).await.unwrap();
            assert!(n > 0, "client closed early; got {seen:?}");
            seen.extend_from_slice(&buf[..n]);
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn tcp_session_end_to_end() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (session, mut events) = Session::connect(addr, SessionConfig::default())
        .await
        .unwrap();
    let (mut server, _) = listener.accept().await.unwrap();
    let mut seen = Vec::new();

    // Active negotiation arrives first.
    read_until(&mut server, &mut seen, &[IAC, WILL, NAWS]).await;

    server
        .write_all(&[IAC, DO, NAWS, IAC, WILL, ECHO])
        .await
        .unwrap();
    server.write_all(b"login: ").await.unwrap();
    assert_eq!(
        next_event(&mut events).await,
        Event::Status(Status::OptionChanged {
            option: TelnetOption::NAWS,
            side: Side::Local,
            enabled: true
        })
    );
    assert_eq!(
        next_event(&mut events).await,
        Event::Status(Status::OptionChanged {
            option: TelnetOption::ECHO,
            side: Side::Remote,
            enabled: true
        })
    );
    assert_eq!(
        next_event(&mut events).await,
        Event::Output(b"login: ".to_vec())
    );
    read_until(
        &mut server,
        &mut seen,
        &[IAC, SB, NAWS, 0, 80, 0, 24, IAC, SE],
    )
    .await;

    session.resize(120, 40).await.unwrap();
    read_until(
        &mut server,
        &mut seen,
        &[IAC, SB, NAWS, 0, 120, 0, 40, IAC, SE],
    )
    .await;

    session.write(b"root\r").await.unwrap();
    read_until(&mut server, &mut seen, b"root\r\n").await;

    drop(server);
    assert_eq!(
        next_event(&mut events).await,
        Event::Closed {
            reason: CloseReason::RemoteClosed
        }
    );
    assert!(matches!(session.write(b"x").await, Err(Error::Closed)));
}

#[tokio::test]
async fn local_close_over_any_stream() {
    let (client, mut server) = tokio::io::duplex(4096);
    let (session, mut events) = Session::from_stream(
        client,
        SessionConfig {
            codec: Config {
                active_negotiation: false,
                ..Config::default()
            },
            ..SessionConfig::default()
        },
    );
    server.write_all(b"hi\xff\xff").await.unwrap();
    assert_eq!(
        next_event(&mut events).await,
        Event::Output(b"hi\xff".to_vec())
    );
    session.write(b"bye\xff").await.unwrap();
    session.close().await.unwrap();
    let mut rest = Vec::new();
    timeout(WAIT, server.read_to_end(&mut rest))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rest, b"bye\xff\xff");
    assert_eq!(
        next_event(&mut events).await,
        Event::Closed {
            reason: CloseReason::LocalClosed
        }
    );
    assert!(events.recv().await.is_none());
}

#[tokio::test]
async fn connect_failure_is_reported() {
    // TEST-NET-1 is never routed: either the timeout fires or the OS rejects the route.
    let cfg = SessionConfig {
        connect_timeout: Duration::from_millis(200),
        ..SessionConfig::default()
    };
    let r = Session::connect("192.0.2.1:23", cfg).await;
    assert!(matches!(r, Err(Error::ConnectTimeout | Error::Io(_))));
}
