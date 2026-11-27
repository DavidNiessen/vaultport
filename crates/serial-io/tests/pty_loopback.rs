// SPDX-License-Identifier: GPL-3.0-or-later
//! End-to-end test of a native serial session against a pseudo-terminal pair: the pty
//! slave is opened through `serialport` exactly like a real device node, and the test
//! plays the device on the master side.
//!
//! Linux only: a Linux pty accepts the termios calls `serialport` makes, so it stands in for
//! a device. macOS ptys reject the serial-only ioctls (`IOSSIOSPEED`, `TIOCEXCL`) with ENOTTY,
//! which says nothing about real serial ports there.

#![cfg(all(target_os = "linux", feature = "native"))]
#![allow(clippy::unwrap_used)]

use std::fs::File;
use std::io::{Read as _, Write as _};
use std::time::Duration;

use serial_io::{CloseReason, Error, Event, SerialConfig, Session};
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(5);

struct Pty {
    master: File,
    // Kept open so the master does not see a hang-up before the session opens the slave.
    slave: Option<std::os::fd::OwnedFd>,
    path: String,
}

fn pty() -> Pty {
    let pair = nix::pty::openpty(None, None).unwrap();
    let path = nix::unistd::ttyname(&pair.slave)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    Pty {
        master: File::from(pair.master),
        slave: Some(pair.slave),
        path,
    }
}

/// Reads from the pty master until `expected.len()` bytes arrived.
async fn read_master(master: &File, expected: usize) -> Vec<u8> {
    let mut master = master.try_clone().unwrap();
    timeout(
        WAIT,
        tokio::task::spawn_blocking(move || {
            let mut out = Vec::new();
            let mut buf = [0u8; 256];
            while out.len() < expected {
                let n = master.read(&mut buf).unwrap();
                out.extend_from_slice(&buf[..n]);
            }
            out
        }),
    )
    .await
    .unwrap()
    .unwrap()
}

async fn collect_output(events: &mut tokio::sync::mpsc::Receiver<Event>, len: usize) -> Vec<u8> {
    let mut out = Vec::new();
    while out.len() < len {
        match timeout(WAIT, events.recv()).await.unwrap().unwrap() {
            Event::Output(data) => out.extend_from_slice(&data),
            other @ Event::Closed { .. } => panic!("unexpected event {other:?}"),
        }
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loopback_through_pty() {
    let mut pty = pty();
    let config = SerialConfig::new(115_200);
    let (session, mut events) = Session::open(&pty.path, &config).await.unwrap();
    drop(pty.slave.take());

    // Device -> session. Binary-safe: raw mode, no CR/LF translation.
    let payload: Vec<u8> = (0..=255u8).chain(b"\r\nlogin: ".iter().copied()).collect();
    pty.master.write_all(&payload).unwrap();
    assert_eq!(collect_output(&mut events, payload.len()).await, payload);

    // Session -> device.
    session.write(b"root\r\n\x00\xff").await.unwrap();
    assert_eq!(read_master(&pty.master, 8).await, b"root\r\n\x00\xff");

    // A large write goes through completely.
    let big = vec![b'x'; 64 * 1024];
    let reader = {
        let master = pty.master.try_clone().unwrap();
        tokio::spawn(async move { read_master(&master, 64 * 1024).await })
    };
    session.write(&big).await.unwrap();
    assert_eq!(reader.await.unwrap(), big);

    // The port is exclusive while open.
    assert!(matches!(
        Session::open(&pty.path, &config).await,
        Err(Error::Busy(_))
    ));

    session.close().await.unwrap();
    assert_eq!(
        timeout(WAIT, events.recv()).await.unwrap().unwrap(),
        Event::Closed {
            reason: CloseReason::Local
        }
    );
    assert!(matches!(session.write(b"x").await, Err(Error::Closed)));

    // After close the device is released and can be reopened right away.
    let (again, mut events) = Session::open(&pty.path, &config).await.unwrap();
    pty.master.write_all(b"ok").unwrap();
    assert_eq!(collect_output(&mut events, 2).await, b"ok");
    again.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hang_up_is_reported_as_disconnect() {
    let mut pty = pty();
    let (session, mut events) = Session::open(&pty.path, &SerialConfig::default())
        .await
        .unwrap();
    drop(pty.slave.take());
    drop(pty.master); // the "device" goes away
    let event = timeout(WAIT, events.recv()).await.unwrap().unwrap();
    assert_eq!(
        event,
        Event::Closed {
            reason: CloseReason::Disconnected
        }
    );
    assert!(session.is_closed());
}

#[tokio::test]
async fn open_errors() {
    assert!(matches!(
        Session::open("/dev/does-not-exist-serial-io", &SerialConfig::default()).await,
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        Session::open("/dev/null", &SerialConfig::new(1)).await,
        Err(Error::InvalidConfig(_))
    ));
}
