// SPDX-License-Identifier: GPL-3.0-or-later
//! The crate-wide error type.

use std::io;
use std::path::PathBuf;

/// Convenience alias for results produced by this crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Everything that can go wrong while installing, loading or running a local model.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The crate was built without on-device inference (cargo feature `llama` disabled).
    #[error("local command generation is not available in this build")]
    Unavailable,

    /// No catalog entry has the requested id.
    #[error("unknown model `{0}`")]
    UnknownModel(String),

    /// The model file is not (completely) installed.
    #[error("model `{0}` is not installed")]
    NotInstalled(String),

    /// Another install, load or removal of the same model is already running.
    #[error("another operation on `{0}` is already in progress")]
    Busy(String),

    /// A URL was rejected by the download policy (scheme, host, port or credentials).
    #[error("refusing to fetch {url}: {reason}")]
    DisallowedUrl {
        /// The offending URL (redirect target or initial URL).
        url: String,
        /// Human readable reason.
        reason: &'static str,
    },

    /// The server redirected more often than allowed.
    #[error("too many redirects")]
    TooManyRedirects,

    /// The server answered with an unexpected HTTP status.
    #[error("unexpected HTTP status {status} from {url}")]
    HttpStatus {
        /// The status code.
        status: u16,
        /// The URL that produced it.
        url: String,
    },

    /// The server sent a response the downloader cannot use (bad `Content-Range`, missing
    /// `Location`, …).
    #[error("invalid server response: {0}")]
    Protocol(String),

    /// Transport-level failure (DNS, TLS, connection reset, timeout, …).
    #[error("network error: {0}")]
    Network(#[source] reqwest::Error),

    /// The downloaded data does not have the pinned size.
    #[error("size mismatch: expected {expected} bytes, got {actual}")]
    SizeMismatch {
        /// Size recorded in the catalog.
        expected: u64,
        /// Size received or found on disk.
        actual: u64,
    },

    /// The download ended early; the partial file is kept so it can be resumed.
    #[error("download incomplete: {received} of {expected} bytes")]
    Incomplete {
        /// Bytes on disk.
        received: u64,
        /// Bytes expected.
        expected: u64,
    },

    /// The downloaded data does not have the pinned SHA-256. The file has been deleted.
    #[error("SHA-256 mismatch: expected {expected}, got {actual}")]
    HashMismatch {
        /// Digest recorded in the catalog (lowercase hex).
        expected: String,
        /// Digest of the received data (lowercase hex).
        actual: String,
    },

    /// A catalog entry carries no verified digest; downloads fail closed.
    #[error("model `{0}` has no verified SHA-256 pinned; refusing to install it")]
    UnverifiedModel(String),

    /// Not enough free space in the model directory.
    #[error("not enough disk space: {required} bytes needed, {available} available")]
    InsufficientDiskSpace {
        /// Bytes still to be written (plus a safety margin).
        required: u64,
        /// Bytes available to this user.
        available: u64,
    },

    /// The operation was cancelled through its cancellation token.
    #[error("cancelled")]
    Cancelled,

    /// Generation did not finish within the configured time limit.
    #[error("timed out")]
    Timeout,

    /// The model could not be loaded (corrupt file, unsupported architecture, out of memory, …).
    #[error("failed to load model {path}: {message}")]
    ModelLoad {
        /// The GGUF file.
        path: PathBuf,
        /// Details from llama.cpp.
        message: String,
    },

    /// The inference backend failed while generating.
    #[error("inference failed: {0}")]
    Inference(String),

    /// The request (prompt plus context) does not fit into the context window.
    #[error("request too long: {tokens} prompt tokens for a context of {ctx_size}")]
    PromptTooLong {
        /// Number of prompt tokens.
        tokens: usize,
        /// Configured context size.
        ctx_size: u32,
    },

    /// The request itself is unusable (for example an empty prompt).
    #[error("invalid request: {0}")]
    InvalidRequest(&'static str),

    /// The model produced output that failed validation. Nothing is returned to the caller,
    /// so an unsafe or malformed suggestion can never reach the terminal.
    #[error("model output rejected: {0}")]
    InvalidOutput(String),

    /// Local file system error.
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Self::Network(e)
    }
}
