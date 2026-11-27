// SPDX-License-Identifier: GPL-3.0-or-later
//! Tokio UDP driver around the sans-IO [`Transport`].

use std::collections::VecDeque;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;
use tokio::sync::{mpsc, oneshot};
use tokio::time::MissedTickBehavior;

use crate::error::{Error, Result};
use crate::key::MoshKey;
use crate::transport::{CloseReason, Transport, TransportConfig, TransportEvent};

/// How long a superseded socket keeps receiving after a rebind (mosh's `MAX_OLD_SOCKET_AGE`).
const OLD_SOCKET_AGE: Duration = Duration::from_secs(60);
const RECV_BUF: usize = 65_536;

/// Session tunables.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Transport tunables.
    pub transport: TransportConfig,
    /// Report [`Status::Stale`] after this long without hearing from the server.
    pub stale_after: Duration,
    /// Automatically move to a new local port when round trips stop (like mosh-client).
    pub auto_rebind: bool,
    /// Capacity of the event channel.
    pub event_capacity: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            transport: TransportConfig::default(),
            stale_after: Duration::from_millis(6500),
            auto_rebind: true,
            event_capacity: 1024,
        }
    }
}

/// Connection status changes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Status {
    /// First authentic datagram from the server arrived.
    Connected,
    /// The server's terminal size changed.
    RemoteResize {
        /// Columns.
        cols: u16,
        /// Rows.
        rows: u16,
    },
    /// Nothing heard from the server for `silent_for`.
    Stale {
        /// Time since the last authentic datagram (or since connecting).
        silent_for: Duration,
    },
    /// The server is reachable again after [`Status::Stale`].
    Reachable,
    /// Moved to a new local UDP socket (roaming).
    Rebound {
        /// New local address.
        local_addr: SocketAddr,
    },
}

/// Something that happened on the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Terminal output; feed to the terminal emulator in order.
    Output(Vec<u8>),
    /// Status change.
    Status(Status),
    /// The session ended; no further events follow.
    Closed {
        /// Why.
        reason: CloseReason,
    },
}

/// Receiver of session events.
pub type SessionEvents = mpsc::Receiver<Event>;

enum Command {
    Write(Vec<u8>, oneshot::Sender<Result<()>>),
    Resize(u16, u16),
    Rebind(oneshot::Sender<Result<SocketAddr>>),
    Close(oneshot::Sender<()>),
}

/// Handle to a running Mosh session. Cheap to clone; dropping every handle shuts the session
/// down gracefully.
#[derive(Debug, Clone)]
pub struct Session {
    cmd: mpsc::Sender<Command>,
    remote: SocketAddr,
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Write(..) => "Write",
            Self::Resize(..) => "Resize",
            Self::Rebind(..) => "Rebind",
            Self::Close(..) => "Close",
        })
    }
}

impl Session {
    /// Starts a session with `mosh-server` at `addr` (port from `MOSH CONNECT`), using the
    /// printable `MOSH_KEY` and the initial terminal size.
    ///
    /// Returns as soon as the local socket is ready; [`Status::Connected`] follows when the
    /// server answers.
    pub async fn connect(
        addr: SocketAddr,
        key: &str,
        cols: u16,
        rows: u16,
    ) -> Result<(Self, SessionEvents)> {
        let key = MoshKey::parse(key)?;
        Self::connect_with(addr, &key, cols, rows, SessionConfig::default()).await
    }

    /// Like [`Session::connect`] with a parsed key and explicit configuration.
    pub async fn connect_with(
        addr: SocketAddr,
        key: &MoshKey,
        cols: u16,
        rows: u16,
        config: SessionConfig,
    ) -> Result<(Self, SessionEvents)> {
        let socket = bind_for(addr).await?;
        let now = Instant::now();
        let mut transport = Transport::new(key, config.transport.clone(), now);
        transport.push_resize(cols, rows);
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let (ev_tx, ev_rx) = mpsc::channel(config.event_capacity.max(1));
        let driver = Driver {
            transport,
            socket,
            old_socket: None,
            remote: addr,
            cmd_rx,
            ev_tx,
            config,
            pending_writes: VecDeque::new(),
            close_waiters: Vec::new(),
            stale: false,
            started: now,
        };
        tokio::spawn(driver.run());
        Ok((
            Self {
                cmd: cmd_tx,
                remote: addr,
            },
            ev_rx,
        ))
    }

    /// Sends user keystrokes. Waits while too much input is unacknowledged.
    pub async fn write(&self, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        let (tx, rx) = oneshot::channel();
        self.send(Command::Write(data.to_vec(), tx)).await?;
        rx.await.map_err(|_| Error::Closed)?
    }

    /// Reports a new terminal size to the server.
    pub async fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.send(Command::Resize(cols, rows)).await
    }

    /// Moves to a new local UDP socket (e.g. after a network change) and returns its address.
    /// The server follows automatically.
    pub async fn rebind(&self) -> Result<SocketAddr> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Rebind(tx)).await?;
        rx.await.map_err(|_| Error::Closed)?
    }

    /// Shuts the session down gracefully and waits until the server acknowledged it (or the
    /// shutdown timed out).
    pub async fn close(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        if self.cmd.send(Command::Close(tx)).await.is_err() {
            return Ok(());
        }
        let _ = rx.await;
        Ok(())
    }

    /// The server address.
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote
    }

    async fn send(&self, cmd: Command) -> Result<()> {
        self.cmd.send(cmd).await.map_err(|_| Error::Closed)
    }
}

async fn bind_for(remote: SocketAddr) -> io::Result<UdpSocket> {
    let local: SocketAddr = if remote.is_ipv4() {
        (Ipv4Addr::UNSPECIFIED, 0).into()
    } else {
        (Ipv6Addr::UNSPECIFIED, 0).into()
    };
    let socket = UdpSocket::bind(local).await?;
    socket.connect(remote).await?;
    Ok(socket)
}

async fn recv_opt(socket: Option<&UdpSocket>, buf: &mut [u8]) -> io::Result<usize> {
    match socket {
        Some(s) => s.recv(buf).await,
        None => std::future::pending().await,
    }
}

async fn sleep_until_opt(deadline: Option<Instant>) {
    match deadline {
        Some(d) => tokio::time::sleep_until(tokio::time::Instant::from_std(d)).await,
        None => std::future::pending().await,
    }
}

struct PendingWrite {
    data: Vec<u8>,
    offset: usize,
    reply: oneshot::Sender<Result<()>>,
}

struct Driver {
    transport: Transport,
    socket: UdpSocket,
    old_socket: Option<(UdpSocket, Instant)>,
    remote: SocketAddr,
    cmd_rx: mpsc::Receiver<Command>,
    ev_tx: mpsc::Sender<Event>,
    config: SessionConfig,
    pending_writes: VecDeque<PendingWrite>,
    close_waiters: Vec<oneshot::Sender<()>>,
    stale: bool,
    started: Instant,
}

impl Driver {
    async fn run(mut self) {
        let mut buf = vec![0u8; RECV_BUF];
        let mut old_buf = vec![0u8; RECV_BUF];
        let mut ticker = tokio::time::interval(Duration::from_millis(250));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut handles_alive = true;
        let reason = loop {
            let now = Instant::now();
            self.feed_writes();
            if self.config.auto_rebind
                && self.transport.needs_rebind(now)
                && let Err(e) = self.rebind(now).await
            {
                tracing::debug!("automatic rebind failed: {e}");
                self.transport.on_rebind(now);
            }
            while let Some(d) = self.transport.poll_transmit(now) {
                if let Err(e) = self.socket.send(&d).await {
                    tracing::trace!("mosh send failed: {e}");
                }
            }
            if let Some(reason) = self.forward_events().await {
                break reason;
            }
            let deadline = self.transport.next_timeout();
            tokio::select! {
                cmd = self.cmd_rx.recv(), if handles_alive => {
                    if let Some(cmd) = cmd {
                        self.handle(cmd).await;
                    } else {
                        // Every handle was dropped: shut down gracefully.
                        handles_alive = false;
                        self.transport.start_shutdown(Instant::now());
                    }
                }
                r = self.socket.recv(&mut buf) => match r {
                    Ok(n) => self.transport.on_datagram(&buf[..n], Instant::now()),
                    Err(e) => tracing::trace!("mosh recv failed: {e}"),
                },
                r = recv_opt(self.old_socket.as_ref().map(|(s, _)| s), &mut old_buf) => match r {
                    Ok(n) => self.transport.on_datagram(&old_buf[..n], Instant::now()),
                    Err(e) => tracing::trace!("mosh recv (old socket) failed: {e}"),
                },
                () = sleep_until_opt(deadline) => {}
                _ = ticker.tick() => self.housekeeping(Instant::now()).await,
            }
        };
        for w in self.pending_writes.drain(..) {
            let _ = w.reply.send(Err(Error::Closed));
        }
        let _ = self.ev_tx.send(Event::Closed { reason }).await;
        for w in self.close_waiters.drain(..) {
            let _ = w.send(());
        }
    }

    fn feed_writes(&mut self) {
        while let Some(w) = self.pending_writes.front_mut() {
            if self.transport.is_shutting_down() || self.transport.is_closed() {
                if let Some(w) = self.pending_writes.pop_front() {
                    let _ = w.reply.send(Err(Error::Closed));
                }
                continue;
            }
            w.offset += self.transport.push_keystrokes(&w.data[w.offset..]);
            if w.offset < w.data.len() {
                break;
            }
            if let Some(w) = self.pending_writes.pop_front() {
                let _ = w.reply.send(Ok(()));
            }
        }
    }

    async fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::Write(data, reply) => self.pending_writes.push_back(PendingWrite {
                data,
                offset: 0,
                reply,
            }),
            Command::Resize(cols, rows) => self.transport.push_resize(cols, rows),
            Command::Rebind(reply) => {
                let r = self.rebind(Instant::now()).await;
                let _ = reply.send(r);
            }
            Command::Close(reply) => {
                self.close_waiters.push(reply);
                self.transport.start_shutdown(Instant::now());
            }
        }
    }

    async fn rebind(&mut self, now: Instant) -> Result<SocketAddr> {
        let fresh = bind_for(self.remote).await?;
        let local_addr = fresh.local_addr()?;
        let old = std::mem::replace(&mut self.socket, fresh);
        self.old_socket = Some((old, now));
        self.transport.on_rebind(now);
        self.emit(Event::Status(Status::Rebound { local_addr }))
            .await;
        Ok(local_addr)
    }

    async fn housekeeping(&mut self, now: Instant) {
        if self
            .old_socket
            .as_ref()
            .is_some_and(|(_, since)| now.saturating_duration_since(*since) > OLD_SOCKET_AGE)
        {
            self.old_socket = None;
        }
        let silent_for =
            now.saturating_duration_since(self.transport.last_heard().unwrap_or(self.started));
        if !self.stale && silent_for > self.config.stale_after {
            self.stale = true;
            self.emit(Event::Status(Status::Stale { silent_for })).await;
        }
    }

    /// Forwards transport events; returns the close reason once the transport has ended.
    async fn forward_events(&mut self) -> Option<CloseReason> {
        while let Some(ev) = self.transport.poll_event() {
            match ev {
                TransportEvent::Connected => self.emit(Event::Status(Status::Connected)).await,
                TransportEvent::Output(bytes) => self.emit(Event::Output(bytes)).await,
                TransportEvent::RemoteResize { cols, rows } => {
                    self.emit(Event::Status(Status::RemoteResize { cols, rows }))
                        .await;
                }
                TransportEvent::Closed(reason) => return Some(reason),
            }
        }
        if self.stale
            && self
                .transport
                .last_heard()
                .is_some_and(|h| h.elapsed() < self.config.stale_after)
        {
            self.stale = false;
            self.emit(Event::Status(Status::Reachable)).await;
        }
        None
    }

    async fn emit(&self, ev: Event) {
        // A dropped receiver is not an error: the session keeps running until closed.
        let _ = self.ev_tx.send(ev).await;
    }
}

#[cfg(test)]
mod tests;
