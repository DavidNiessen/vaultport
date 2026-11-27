// SPDX-License-Identifier: GPL-3.0-or-later
//! Loopback tests: a real [`Session`] against a fake server on 127.0.0.1.
#![allow(clippy::unwrap_used)]

use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::time::timeout;

use super::*;
use crate::proto::transport_buffers::Instruction;
use crate::testutil::{FakeServer, TEST_KEY, host_bytes, inst, user_input};

const WAIT: Duration = Duration::from_secs(10);

struct Server {
    socket: UdpSocket,
    fake: FakeServer,
    buf: Vec<u8>,
}

impl Server {
    async fn new() -> Self {
        Self {
            socket: UdpSocket::bind("127.0.0.1:0").await.unwrap(),
            fake: FakeServer::new(),
            buf: vec![0; 65_536],
        }
    }

    fn addr(&self) -> SocketAddr {
        self.socket.local_addr().unwrap()
    }

    /// Receives client datagrams until `pred` accepts a completed instruction.
    async fn recv_until(
        &mut self,
        pred: impl Fn(&Instruction) -> bool,
    ) -> (Instruction, SocketAddr) {
        timeout(WAIT, async {
            loop {
                let (n, from) = self.socket.recv_from(&mut self.buf).await.unwrap();
                if let Some(i) = self.fake.receive(&self.buf[..n])
                    && pred(&i)
                {
                    return (i, from);
                }
            }
        })
        .await
        .expect("timed out waiting for client datagram")
    }

    async fn send(&mut self, inst: &Instruction, to: SocketAddr) {
        let reply = self.fake.last_client_timestamp;
        for d in self.fake.send(inst, reply) {
            self.socket.send_to(&d, to).await.unwrap();
        }
    }
}

async fn next_event(events: &mut SessionEvents) -> Event {
    timeout(WAIT, events.recv())
        .await
        .expect("timed out")
        .expect("event channel closed")
}

#[tokio::test]
async fn keystroke_and_host_bytes_over_udp() {
    let mut server = Server::new().await;
    let (session, mut events) = Session::connect(server.addr(), TEST_KEY, 80, 24)
        .await
        .unwrap();

    let (first, client) = server.recv_until(|_| true).await;
    assert_eq!(
        user_input(first.diff.as_deref().unwrap()).resizes,
        vec![(80, 24)]
    );

    session.write(b"ls\r").await.unwrap();
    let (keys, _) = server
        .recv_until(|i| user_input(i.diff.as_deref().unwrap()).keys == b"ls\r")
        .await;
    let client_state = keys.new_num.unwrap();

    server
        .send(
            &inst(0, 1, client_state, host_bytes(b"total 0\r\n")),
            client,
        )
        .await;
    assert_eq!(
        next_event(&mut events).await,
        Event::Status(Status::Connected)
    );
    assert_eq!(
        next_event(&mut events).await,
        Event::Output(b"total 0\r\n".to_vec())
    );

    // The client acknowledges terminal state 1.
    let (ack, _) = server.recv_until(|i| i.ack_num == Some(1)).await;
    assert!(ack.old_num.unwrap() >= client_state);

    // Graceful shutdown.
    let closer = {
        let session = session.clone();
        tokio::spawn(async move { session.close().await })
    };
    let (bye, _) = server.recv_until(|i| i.new_num == Some(u64::MAX)).await;
    assert_eq!(bye.ack_num, Some(1));
    server.send(&inst(1, 2, u64::MAX, vec![]), client).await;
    timeout(WAIT, closer).await.unwrap().unwrap().unwrap();
    assert_eq!(
        next_event(&mut events).await,
        Event::Closed {
            reason: CloseReason::LocalShutdown
        }
    );
    assert!(matches!(session.write(b"x").await, Err(Error::Closed)));
}

#[tokio::test]
async fn roaming_to_a_new_socket() {
    let mut server = Server::new().await;
    let (session, mut events) = Session::connect(server.addr(), TEST_KEY, 100, 30)
        .await
        .unwrap();
    let (first, old_addr) = server.recv_until(|_| true).await;
    server
        .send(
            &inst(0, 1, first.new_num.unwrap(), host_bytes(b"a")),
            old_addr,
        )
        .await;
    assert_eq!(
        next_event(&mut events).await,
        Event::Status(Status::Connected)
    );
    assert_eq!(next_event(&mut events).await, Event::Output(b"a".to_vec()));

    let new_local = session.rebind().await.unwrap();
    assert_eq!(
        next_event(&mut events).await,
        Event::Status(Status::Rebound {
            local_addr: new_local
        })
    );
    session.write(b"z").await.unwrap();
    let (_, new_addr) = server
        .recv_until(|i| user_input(i.diff.as_deref().unwrap()).keys == b"z")
        .await;
    assert_ne!(new_addr, old_addr);
    assert_eq!(new_addr.port(), new_local.port());
    // The server follows the client to its new address.
    server
        .send(&inst(1, 2, 0, host_bytes(b"b")), new_addr)
        .await;
    assert_eq!(next_event(&mut events).await, Event::Output(b"b".to_vec()));
    // Late packets to the old socket are still accepted for a while.
    server
        .send(&inst(2, 3, 0, host_bytes(b"c")), old_addr)
        .await;
    assert_eq!(next_event(&mut events).await, Event::Output(b"c".to_vec()));
}

#[tokio::test]
async fn server_exit_closes_session() {
    let mut server = Server::new().await;
    let (session, mut events) = Session::connect(server.addr(), TEST_KEY, 80, 24)
        .await
        .unwrap();
    let (first, client) = server.recv_until(|_| true).await;
    server
        .send(
            &inst(
                0,
                u64::MAX,
                first.new_num.unwrap(),
                host_bytes(b"logout\r\n"),
            ),
            client,
        )
        .await;
    assert_eq!(
        next_event(&mut events).await,
        Event::Status(Status::Connected)
    );
    assert_eq!(
        next_event(&mut events).await,
        Event::Output(b"logout\r\n".to_vec())
    );
    let (ack, _) = server.recv_until(|i| i.ack_num == Some(u64::MAX)).await;
    assert_eq!(ack.protocol_version, Some(2));
    assert_eq!(
        next_event(&mut events).await,
        Event::Closed {
            reason: CloseReason::ServerExited
        }
    );
    assert!(events.recv().await.is_none());
    assert!(matches!(session.write(b"x").await, Err(Error::Closed)));
    // Garbage to a closed session is harmless.
    let _ = server.socket.send_to(&[0; 4], client).await;
}

#[tokio::test]
async fn invalid_key_rejected() {
    let addr: SocketAddr = "127.0.0.1:9".parse().unwrap();
    assert!(matches!(
        Session::connect(addr, "short", 80, 24).await,
        Err(Error::InvalidKey)
    ));
}
