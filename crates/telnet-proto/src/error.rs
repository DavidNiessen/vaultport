// SPDX-License-Identifier: GPL-3.0-or-later
//! Error type.

use std::io;

/// Errors returned by the Telnet client.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Socket error.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    /// The TCP connection was not established within the configured timeout.
    #[error("connection timed out")]
    ConnectTimeout,
    /// The session has ended.
    #[error("telnet session closed")]
    Closed,
}

/// Result alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;
