// SPDX-License-Identifier: GPL-3.0-or-later
//! Error type.

use std::io;

/// Errors returned by the Mosh client.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The `MOSH_KEY` is not 22 characters of canonical base64 encoding 16 bytes.
    #[error("invalid mosh session key")]
    InvalidKey,
    /// Socket error.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    /// The session has ended (or is shutting down) and accepts no more input.
    #[error("mosh session closed")]
    Closed,
}

/// Result alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;
