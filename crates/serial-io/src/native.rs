// SPDX-License-Identifier: GPL-3.0-or-later
//! Native serial ports through the `serialport` crate.
//!
//! `serialport` is blocking. A dedicated reader thread polls the port with a short timeout
//! and forwards data into a bounded channel (so a slow consumer applies back-pressure);
//! writes and modem control run on tokio's blocking pool. This works identically on
//! Linux, macOS and Windows, unlike `tokio-serial`/`mio-serial`, whose Windows support
//! relies on named-pipe emulation and which lags behind `serialport` releases.
//!
//! On Unix the port is opened exclusively (`TIOCEXCL` + `flock`), so a second session (or
//! another program honouring the lock) cannot interleave with ours.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serialport::SerialPort;
use tokio::sync::mpsc;

use crate::config::{DataBits, FlowControl, Parity, SerialConfig, StopBits};
use crate::error::{Error, Result};
use crate::transport::SerialTransport;

/// Poll interval of the reader thread (bounds how long closing takes).
const READ_TIMEOUT: Duration = Duration::from_millis(100);
/// Total time allowed for one write (hardware flow control can stall it).
const WRITE_DEADLINE: Duration = Duration::from_secs(10);
const READ_BUFFER: usize = 4096;
const CHANNEL_CAPACITY: usize = 64;

enum Incoming {
    Data(Vec<u8>),
    Eof,
    Failed(io::Error),
}

type SharedPort = Arc<Mutex<Option<Box<dyn SerialPort>>>>;

pub(crate) struct NativeTransport {
    path: String,
    port: SharedPort,
    incoming: tokio::sync::Mutex<mpsc::Receiver<Incoming>>,
    stop: Arc<AtomicBool>,
    reader: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for NativeTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeTransport")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

fn open_error(path: &str, error: serialport::Error) -> Error {
    match error.kind {
        // On Unix a missing node is `Io(NotFound)`; `NoDevice` means TIOCEXCL/flock found
        // the port in use. On Windows `NoDevice` covers both "not found" and "access
        // denied" (which is how an open COM port presents itself).
        serialport::ErrorKind::NoDevice if cfg!(unix) => Error::Busy(path.to_owned()),
        serialport::ErrorKind::NoDevice if error.description.contains("denied") => {
            Error::Busy(path.to_owned())
        }
        serialport::ErrorKind::NoDevice | serialport::ErrorKind::Io(io::ErrorKind::NotFound) => {
            Error::NotFound(path.to_owned())
        }
        serialport::ErrorKind::Io(io::ErrorKind::PermissionDenied) => {
            Error::PermissionDenied(path.to_owned())
        }
        serialport::ErrorKind::Io(io::ErrorKind::ResourceBusy | io::ErrorKind::WouldBlock) => {
            Error::Busy(path.to_owned())
        }
        serialport::ErrorKind::InvalidInput => Error::InvalidConfig("rejected by the driver"),
        _ => Error::Port(error.description),
    }
}

fn to_io(error: serialport::Error) -> io::Error {
    io::Error::from(error)
}

fn lock(port: &SharedPort) -> MutexGuard<'_, Option<Box<dyn SerialPort>>> {
    port.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl NativeTransport {
    /// Opens `path`. Blocking; call from a blocking context.
    pub(crate) fn open(path: &str, config: SerialConfig) -> Result<Self> {
        let builder = serialport::new(path, config.baud_rate)
            .data_bits(match config.data_bits {
                DataBits::Five => serialport::DataBits::Five,
                DataBits::Six => serialport::DataBits::Six,
                DataBits::Seven => serialport::DataBits::Seven,
                DataBits::Eight => serialport::DataBits::Eight,
            })
            .stop_bits(match config.stop_bits {
                StopBits::One => serialport::StopBits::One,
                StopBits::Two => serialport::StopBits::Two,
            })
            .parity(match config.parity {
                Parity::None => serialport::Parity::None,
                Parity::Odd => serialport::Parity::Odd,
                Parity::Even => serialport::Parity::Even,
            })
            .flow_control(match config.flow_control {
                FlowControl::None => serialport::FlowControl::None,
                FlowControl::Software => serialport::FlowControl::Software,
                FlowControl::Hardware => serialport::FlowControl::Hardware,
            })
            .timeout(READ_TIMEOUT);
        let port = builder.open().map_err(|e| open_error(path, e))?;
        let mut reader_port = port.try_clone().map_err(|e| open_error(path, e))?;

        let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let reader = std::thread::Builder::new()
            .name("serial-reader".to_owned())
            .spawn(move || read_thread(reader_port.as_mut(), &thread_stop, &tx))
            .map_err(Error::Io)?;

        Ok(Self {
            path: path.to_owned(),
            port: Arc::new(Mutex::new(Some(port))),
            incoming: tokio::sync::Mutex::new(rx),
            stop,
            reader: Mutex::new(Some(reader)),
        })
    }

    async fn with_port<T, F>(&self, f: F) -> io::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut dyn SerialPort) -> io::Result<T> + Send + 'static,
    {
        let port = Arc::clone(&self.port);
        tokio::task::spawn_blocking(move || {
            let mut guard = lock(&port);
            let port = guard
                .as_mut()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "port closed"))?;
            f(port.as_mut())
        })
        .await
        .map_err(|_| io::Error::other("serial worker task failed"))?
    }
}

impl Drop for NativeTransport {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

fn read_thread(port: &mut dyn SerialPort, stop: &AtomicBool, tx: &mpsc::Sender<Incoming>) {
    let mut buf = vec![0u8; READ_BUFFER];
    while !stop.load(Ordering::Acquire) {
        let item = match port.read(&mut buf) {
            // On Unix a zero-length read after poll() reported readiness means hang-up;
            // the Windows driver returns 0 when the read timeout expires.
            Ok(0) if cfg!(unix) => Incoming::Eof,
            Ok(0) => continue,
            Ok(n) => Incoming::Data(buf[..n].to_vec()),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::TimedOut
                        | io::ErrorKind::WouldBlock
                        | io::ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Err(e) => Incoming::Failed(e),
        };
        let last = !matches!(item, Incoming::Data(_));
        if tx.blocking_send(item).is_err() || last {
            break;
        }
    }
}

#[async_trait::async_trait]
impl SerialTransport for NativeTransport {
    async fn read(&self) -> io::Result<Option<Vec<u8>>> {
        match self.incoming.lock().await.recv().await {
            Some(Incoming::Data(data)) => Ok(Some(data)),
            Some(Incoming::Eof) | None => Ok(None),
            Some(Incoming::Failed(error)) => Err(error),
        }
    }

    async fn write(&self, data: &[u8]) -> io::Result<()> {
        let data = data.to_vec();
        self.with_port(move |port| {
            let deadline = Instant::now() + WRITE_DEADLINE;
            let mut rest = data.as_slice();
            while !rest.is_empty() {
                match port.write(rest) {
                    Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                    Ok(n) => rest = &rest[n..],
                    Err(e)
                        if matches!(
                            e.kind(),
                            io::ErrorKind::TimedOut
                                | io::ErrorKind::WouldBlock
                                | io::ErrorKind::Interrupted
                        ) && Instant::now() < deadline => {}
                    Err(e) => return Err(e),
                }
            }
            Ok(())
        })
        .await
    }

    async fn send_break(&self, duration: Duration) -> io::Result<()> {
        self.with_port(|port| port.set_break().map_err(to_io))
            .await?;
        tokio::time::sleep(duration).await;
        self.with_port(|port| port.clear_break().map_err(to_io))
            .await
    }

    async fn set_dtr(&self, level: bool) -> io::Result<()> {
        self.with_port(move |port| port.write_data_terminal_ready(level).map_err(to_io))
            .await
    }

    async fn set_rts(&self, level: bool) -> io::Result<()> {
        self.with_port(move |port| port.write_request_to_send(level).map_err(to_io))
            .await
    }

    async fn close(&self) -> io::Result<()> {
        self.stop.store(true, Ordering::Release);
        // Fails any `blocking_send` the reader thread is stuck in (full channel), so the
        // join below cannot hang.
        self.incoming.lock().await.close();
        let reader = self
            .reader
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        let port = Arc::clone(&self.port);
        tokio::task::spawn_blocking(move || {
            // Close our handle, then wait (at most one read timeout) for the reader thread
            // to drop its clone, so the device is fully released when this returns.
            drop(lock(&port).take());
            if let Some(reader) = reader {
                let _ = reader.join();
            }
        })
        .await
        .map_err(|_| io::Error::other("serial worker task failed"))
    }
}
