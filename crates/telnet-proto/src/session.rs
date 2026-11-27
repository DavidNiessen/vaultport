// SPDX-License-Identifier: GPL-3.0-or-later
//! Thin tokio wrapper driving a [`Codec`] over any byte stream (TCP by default).

use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::sync::{mpsc, oneshot};

use crate::codec::{Codec, CodecEvent, Config, Side};
use crate::consts::TelnetOption;
use crate::error::{Error, Result};

/// Stop accepting user input while this much output is waiting for the socket.
const HIGH_WATER: usize = 64 * 1024;
const READ_BUF: usize = 16 * 1024;

/// Session tunables.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Codec (negotiation) configuration.
    pub codec: Config,
    /// TCP connect timeout (including name resolution).
    pub connect_timeout: Duration,
    /// Capacity of the event channel.
    pub event_capacity: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            codec: Config::default(),
            connect_timeout: Duration::from_secs(10),
            event_capacity: 256,
        }
    }
}

/// Status changes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Status {
    /// An option was negotiated on or off. `ECHO` on the remote side means the server
    /// echoes input (character mode); otherwise the terminal should echo locally.
    OptionChanged {
        /// The option.
        option: TelnetOption,
        /// Who performs it.
        side: Side,
        /// New state.
        enabled: bool,
    },
    /// A bare Telnet command from the server (GA, NOP, AYT, ...).
    Command(u8),
}

/// Why the session ended.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CloseReason {
    /// The server closed the connection.
    RemoteClosed,
    /// [`Session::close`] was called (or every handle dropped).
    LocalClosed,
    /// Socket failure.
    Io(String),
}

impl std::fmt::Display for CloseReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RemoteClosed => f.write_str("connection closed by server"),
            Self::LocalClosed => f.write_str("session closed"),
            Self::Io(e) => write!(f, "network error: {e}"),
        }
    }
}

/// Something that happened on the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Terminal output.
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

#[derive(Debug)]
enum Command {
    Write(Vec<u8>),
    Resize(u16, u16),
    Close(oneshot::Sender<()>),
}

/// Handle to a running Telnet session. Cheap to clone; dropping every handle closes it.
#[derive(Debug, Clone)]
pub struct Session {
    cmd: mpsc::Sender<Command>,
}

impl Session {
    /// Connects over TCP (with `connect_timeout`) and starts the session.
    pub async fn connect<A: ToSocketAddrs>(
        addr: A,
        config: SessionConfig,
    ) -> Result<(Self, SessionEvents)> {
        let stream = tokio::time::timeout(config.connect_timeout, TcpStream::connect(addr))
            .await
            .map_err(|_| Error::ConnectTimeout)??;
        stream.set_nodelay(true)?;
        Ok(Self::from_stream(stream, config))
    }

    /// Runs the session over an already-connected byte stream (TCP, TLS, a proxy tunnel...).
    pub fn from_stream<S>(stream: S, config: SessionConfig) -> (Self, SessionEvents)
    where
        S: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let (ev_tx, ev_rx) = mpsc::channel(config.event_capacity.max(1));
        let mut codec = Codec::new(config.codec);
        codec.start();
        tokio::spawn(run(stream, codec, cmd_rx, ev_tx));
        (Self { cmd: cmd_tx }, ev_rx)
    }

    /// Sends user keystrokes.
    pub async fn write(&self, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        self.send(Command::Write(data.to_vec())).await
    }

    /// Reports a new window size (sent via NAWS when the server accepted it).
    pub async fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.send(Command::Resize(cols, rows)).await
    }

    /// Flushes pending output, closes the connection and waits for the session to end.
    pub async fn close(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        if self.cmd.send(Command::Close(tx)).await.is_ok() {
            let _ = rx.await;
        }
        Ok(())
    }

    async fn send(&self, cmd: Command) -> Result<()> {
        self.cmd.send(cmd).await.map_err(|_| Error::Closed)
    }
}

async fn run<S>(
    stream: S,
    mut codec: Codec,
    mut cmd_rx: mpsc::Receiver<Command>,
    ev_tx: mpsc::Sender<Event>,
) where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (mut rd, mut wr) = tokio::io::split(stream);
    let mut out: Vec<u8> = codec.take_transmit();
    let mut buf = vec![0u8; READ_BUF];
    let mut needs_flush = false;
    let mut close_waiter = None;

    let reason = loop {
        tokio::select! {
            r = rd.read(&mut buf) => match r {
                Ok(0) => break CloseReason::RemoteClosed,
                Ok(n) => {
                    let events = codec.decode(&buf[..n]);
                    out.extend(codec.take_transmit());
                    for ev in events {
                        let ev = match ev {
                            CodecEvent::Data(d) => Event::Output(d),
                            CodecEvent::OptionChanged { option, side, enabled } => {
                                Event::Status(Status::OptionChanged { option, side, enabled })
                            }
                            CodecEvent::Command(c) => Event::Status(Status::Command(c)),
                        };
                        // A dropped receiver is fine; keep the connection serviced.
                        let _ = ev_tx.send(ev).await;
                    }
                }
                Err(e) => break CloseReason::Io(e.to_string()),
            },
            r = write_or_flush(&mut wr, &out), if !out.is_empty() || needs_flush => match r {
                Ok(Some(0)) => break CloseReason::Io("connection closed while writing".into()),
                Ok(Some(n)) => {
                    out.drain(..n);
                    needs_flush = out.is_empty();
                }
                Ok(None) => needs_flush = false,
                Err(e) => break CloseReason::Io(e.to_string()),
            },
            cmd = cmd_rx.recv(), if out.len() < HIGH_WATER => match cmd {
                Some(Command::Write(data)) => {
                    codec.encode_data(&data);
                    out.extend(codec.take_transmit());
                }
                Some(Command::Resize(cols, rows)) => {
                    codec.set_window_size(cols, rows);
                    out.extend(codec.take_transmit());
                }
                Some(Command::Close(reply)) => {
                    close_waiter = Some(reply);
                    break CloseReason::LocalClosed;
                }
                None => break CloseReason::LocalClosed,
            },
        }
    };

    if reason == CloseReason::LocalClosed {
        let graceful = async {
            wr.write_all(&out).await?;
            wr.shutdown().await
        };
        let _ = tokio::time::timeout(Duration::from_secs(2), graceful).await;
    }
    let _ = ev_tx.send(Event::Closed { reason }).await;
    if let Some(w) = close_waiter {
        let _ = w.send(());
    }
}

/// Writes some of `out` (returning the count) or, when it is empty, flushes (`None`).
async fn write_or_flush<W: AsyncWrite + Unpin>(
    wr: &mut W,
    out: &[u8],
) -> std::io::Result<Option<usize>> {
    if out.is_empty() {
        wr.flush().await.map(|()| None)
    } else {
        wr.write(out).await.map(Some)
    }
}

#[cfg(test)]
mod tests;
