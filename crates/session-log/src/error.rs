// SPDX-License-Identifier: GPL-3.0-or-later
use thiserror::Error;

/// Errors returned when writing or reading a session log.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LogError {
    /// Wrong key, or a chunk was modified, swapped, reordered or taken from another log.
    #[error("wrong key, or the log has been tampered with (chunk {chunk})")]
    Tampered { chunk: u64 },
    /// The log ends before its end chunk (crash, or deliberate truncation).
    #[error("the log is truncated (no end chunk after {chunks} chunks)")]
    Truncated { chunks: u64 },
    /// The file is not a session log or uses an unsupported version.
    #[error("not a session log, or unsupported format version")]
    BadMagic,
    /// The file belongs to a different log than expected.
    #[error("the file belongs to another log")]
    WrongLog,
    /// Structurally invalid content inside an authentic chunk, or data after the end chunk.
    #[error("corrupt session log: {0}")]
    Corrupt(String),
    /// The writer was already finished.
    #[error("the log writer is already finished")]
    Finished,
    #[error("cryptographic failure: {0}")]
    Crypto(#[from] vault::VaultError),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}
