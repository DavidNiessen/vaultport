// SPDX-License-Identifier: GPL-3.0-or-later
use thiserror::Error;

/// Fatal import errors. Recoverable problems are reported as
/// [`ImportWarning`](crate::ImportWarning)s instead.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImportError {
    /// The input exceeds [`MAX_INPUT_BYTES`](crate::MAX_INPUT_BYTES).
    #[error("input is too large ({size} bytes, maximum {max})")]
    TooLarge { size: usize, max: usize },
    /// The input is not valid text in any supported encoding.
    #[error("input is not valid text: {0}")]
    Encoding(String),
    /// The input does not look like the requested format.
    #[error("input is not a valid {format} file: {reason}")]
    Malformed {
        format: &'static str,
        reason: String,
    },
    /// The input format could not be detected.
    #[error("unrecognized input format")]
    UnknownFormat,
}
