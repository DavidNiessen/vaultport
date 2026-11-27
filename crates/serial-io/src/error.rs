// SPDX-License-Identifier: GPL-3.0-or-later
//! Error type for serial sessions.

/// Errors produced by serial port operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The configuration or an argument is out of range.
    #[error("invalid serial configuration: {0}")]
    InvalidConfig(&'static str),
    /// The device does not exist (or was unplugged).
    #[error("serial port not found: {0}")]
    NotFound(String),
    /// The OS refused access (missing `dialout` group membership, sandbox, ...).
    #[error("permission denied opening serial port {0}")]
    PermissionDenied(String),
    /// The port is already open in another session or program.
    #[error("serial port {0} is busy")]
    Busy(String),
    /// The session has been closed.
    #[error("serial session is closed")]
    Closed,
    /// A write did not complete in time (e.g. hardware flow control holding CTS low).
    #[error("serial write timed out")]
    WriteTimeout,
    /// The operation is not available with this build or transport.
    #[error("not supported: {0}")]
    Unsupported(&'static str),
    /// No tokio runtime is running on this thread.
    #[error("a tokio runtime is required")]
    NoRuntime,
    /// An error reported by the serial port driver.
    #[error("serial port error: {0}")]
    Port(String),
    /// An I/O error from the transport.
    #[error("serial I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result alias for this crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;
