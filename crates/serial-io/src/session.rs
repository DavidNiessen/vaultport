// SPDX-License-Identifier: GPL-3.0-or-later
//! Serial console sessions.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::{Mutex, mpsc, watch};
use tokio::task::JoinHandle;

use crate::error::{Error, Result};
use crate::transport::SerialTransport;

/// Capacity of the event channel returned with a session. When the consumer falls behind,
/// reading from the device pauses (back-pressure) instead of buffering without bound.
pub const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Longest accepted break duration.
pub const MAX_BREAK: Duration = Duration::from_secs(10);

/// Something that happened on a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Bytes received from the device.
    Output(Vec<u8>),
    /// The session ended; no further events follow.
    Closed {
        /// Why it ended.
        reason: CloseReason,
    },
}

/// Why a session ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CloseReason {
    /// [`Session::close`] was called or every [`Session`] handle was dropped.
    Local,
    /// The device disappeared (unplugged, hang-up).
    Disconnected,
    /// A read error ended the session.
    Error {
        /// Description of the error.
        message: String,
    },
}

impl std::fmt::Display for CloseReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local => f.write_str("closed"),
            Self::Disconnected => f.write_str("device disconnected"),
            Self::Error { message } => write!(f, "read error: {message}"),
        }
    }
}

/// An open serial console. Cheap to clone; all clones control the same session.
///
/// Received data arrives on the [`mpsc::Receiver<Event>`] returned when the session is
/// created, ending with exactly one [`Event::Closed`] (unless the receiver was dropped,
/// which also ends the session).
#[derive(Debug, Clone)]
pub struct Session {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    transport: Arc<dyn SerialTransport>,
    closed: Arc<AtomicBool>,
    shutdown: watch::Sender<bool>,
    reader: Mutex<Option<JoinHandle<()>>>,
    write_lock: Mutex<()>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        // Stops the reader task, which then closes the transport.
        let _ = self.shutdown.send(true);
    }
}

impl Session {
    /// Opens the native serial port at `path` (e.g. `/dev/ttyUSB0`, `COM3`).
    #[cfg(all(feature = "native", not(any(target_os = "android", target_os = "ios"))))]
    pub async fn open(
        path: &str,
        config: &crate::SerialConfig,
    ) -> Result<(Self, mpsc::Receiver<Event>)> {
        config.validate()?;
        let path = path.to_owned();
        let config = *config;
        let transport = tokio::task::spawn_blocking(move || {
            crate::native::NativeTransport::open(&path, config)
        })
        .await
        .map_err(|_| Error::Port("open task failed".to_owned()))??;
        Self::from_transport(Box::new(transport))
    }

    /// Starts a session over any transport (e.g. Android USB serial from the app).
    ///
    /// Must be called within a tokio runtime; the reader runs as a task on it.
    pub fn from_transport(
        transport: Box<dyn SerialTransport>,
    ) -> Result<(Self, mpsc::Receiver<Event>)> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| Error::NoRuntime)?;
        let transport: Arc<dyn SerialTransport> = Arc::from(transport);
        let closed = Arc::new(AtomicBool::new(false));
        let (shutdown, shutdown_rx) = watch::channel(false);
        let (events, receiver) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let reader = runtime.spawn(read_loop(
            Arc::clone(&transport),
            Arc::clone(&closed),
            shutdown_rx,
            events,
        ));
        let session = Self {
            inner: Arc::new(Inner {
                transport,
                closed,
                shutdown,
                reader: Mutex::new(Some(reader)),
                write_lock: Mutex::new(()),
            }),
        };
        Ok((session, receiver))
    }

    /// Whether the session has ended.
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Acquire)
    }

    fn ensure_open(&self) -> Result<()> {
        if self.is_closed() {
            Err(Error::Closed)
        } else {
            Ok(())
        }
    }

    /// Sends bytes to the device. Concurrent writes are applied in call order.
    pub async fn write(&self, data: &[u8]) -> Result<()> {
        let _guard = self.inner.write_lock.lock().await;
        self.ensure_open()?;
        self.inner.transport.write(data).await.map_err(io_error)
    }

    /// Sends a break of `duration` (1 ms to [`MAX_BREAK`]); 250 ms is a common value
    /// (e.g. Cisco ROMMON, Linux SysRq).
    pub async fn send_break(&self, duration: Duration) -> Result<()> {
        if duration < Duration::from_millis(1) || duration > MAX_BREAK {
            return Err(Error::InvalidConfig("break duration must be 1 ms to 10 s"));
        }
        let _guard = self.inner.write_lock.lock().await;
        self.ensure_open()?;
        self.inner
            .transport
            .send_break(duration)
            .await
            .map_err(io_error)
    }

    /// Sets the DTR line.
    pub async fn set_dtr(&self, level: bool) -> Result<()> {
        self.ensure_open()?;
        self.inner.transport.set_dtr(level).await.map_err(io_error)
    }

    /// Sets the RTS line.
    pub async fn set_rts(&self, level: bool) -> Result<()> {
        self.ensure_open()?;
        self.inner.transport.set_rts(level).await.map_err(io_error)
    }

    /// Closes the session and waits until the device is released, so it can be reopened
    /// immediately. Idempotent.
    pub async fn close(&self) -> Result<()> {
        self.inner.closed.store(true, Ordering::Release);
        let _ = self.inner.shutdown.send(true);
        let reader = self.inner.reader.lock().await.take();
        if let Some(reader) = reader {
            // The reader task closes the transport and emits `Closed` before finishing.
            let _ = reader.await;
        }
        Ok(())
    }
}

fn io_error(error: io::Error) -> Error {
    match error.kind() {
        io::ErrorKind::TimedOut => Error::WriteTimeout,
        io::ErrorKind::Unsupported => Error::Unsupported("operation not supported by the port"),
        _ => Error::Io(error),
    }
}

/// Whether a read error means the device went away rather than a transient failure.
fn is_disconnect(error: &io::Error) -> bool {
    const EIO: i32 = 5;
    const ENXIO: i32 = 6;
    const ENODEV: i32 = 19;
    matches!(
        error.kind(),
        io::ErrorKind::BrokenPipe | io::ErrorKind::NotConnected | io::ErrorKind::UnexpectedEof
    ) || (cfg!(unix) && matches!(error.raw_os_error(), Some(EIO | ENXIO | ENODEV)))
}

async fn read_loop(
    transport: Arc<dyn SerialTransport>,
    closed: Arc<AtomicBool>,
    mut shutdown: watch::Receiver<bool>,
    events: mpsc::Sender<Event>,
) {
    // `None` means the consumer went away: nobody is left to tell.
    let reason = loop {
        if *shutdown.borrow() {
            break Some(CloseReason::Local);
        }
        let chunk = tokio::select! {
            biased;
            _ = shutdown.changed() => break Some(CloseReason::Local),
            read = transport.read() => read,
        };
        match chunk {
            Ok(Some(data)) if data.is_empty() => {}
            Ok(Some(data)) => {
                tokio::select! {
                    biased;
                    _ = shutdown.changed() => break Some(CloseReason::Local),
                    sent = events.send(Event::Output(data)) => {
                        if sent.is_err() {
                            break None;
                        }
                    }
                }
            }
            Ok(None) => break Some(CloseReason::Disconnected),
            Err(error) if is_disconnect(&error) => break Some(CloseReason::Disconnected),
            Err(error) => {
                break Some(CloseReason::Error {
                    message: error.to_string(),
                });
            }
        }
    };
    closed.store(true, Ordering::Release);
    if let Err(error) = transport.close().await {
        tracing::debug!(%error, "closing serial transport failed");
    }
    if let Some(reason) = reason {
        tracing::debug!(%reason, "serial session ended");
        // Never block here: `Session::close` waits for this task, and its caller may not
        // be draining the channel at the same time.
        if let Err(mpsc::error::TrySendError::Full(event)) =
            events.try_send(Event::Closed { reason })
        {
            tokio::spawn(async move {
                let _ = events.send(event).await;
            });
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
