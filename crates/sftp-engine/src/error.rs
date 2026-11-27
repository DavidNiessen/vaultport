// SPDX-License-Identifier: GPL-3.0-or-later
//! File system errors.

use russh_sftp::client::error::Error as SftpError;
use russh_sftp::protocol::StatusCode;
use std::io;

/// Errors of [`crate::FileSystem`] operations and transfers.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum FsError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("not a directory: {0}")]
    NotADirectory(String),
    #[error("is a directory: {0}")]
    IsADirectory(String),
    /// A name or path that would escape its directory (e.g. `../x` sent by a server).
    #[error("unsafe path rejected: {0}")]
    UnsafePath(String),
    /// A safety limit (entries, depth, size) was hit.
    #[error("limit exceeded: {0}")]
    LimitExceeded(String),
    #[error("operation not supported: {0}")]
    Unsupported(String),
    /// The remote file changed since it was downloaded (edit-and-upload).
    #[error("remote file changed since download: {0}")]
    RemoteChanged(String),
    #[error("cancelled")]
    Cancelled,
    /// The SFTP session or the connection is gone.
    #[error("connection lost: {0}")]
    ConnectionLost(String),
    #[error("i/o error: {0}")]
    Io(String),
    /// Generic server-side failure (`SSH_FX_FAILURE`), e.g. directory not empty.
    #[error("server error: {0}")]
    Remote(String),
}

impl FsError {
    /// Maps an I/O error, adding the path for context.
    pub(crate) fn io(e: &io::Error, path: &str) -> Self {
        match e.kind() {
            io::ErrorKind::NotFound => Self::NotFound(path.to_owned()),
            io::ErrorKind::PermissionDenied => Self::PermissionDenied(path.to_owned()),
            io::ErrorKind::AlreadyExists => Self::AlreadyExists(path.to_owned()),
            io::ErrorKind::NotADirectory => Self::NotADirectory(path.to_owned()),
            io::ErrorKind::IsADirectory => Self::IsADirectory(path.to_owned()),
            _ => Self::Io(format!("{path}: {e}")),
        }
    }

    /// Maps an SFTP client error, adding the path for context.
    pub(crate) fn sftp(e: &SftpError, path: &str) -> Self {
        match e {
            SftpError::Status(s) => match s.status_code {
                StatusCode::NoSuchFile => Self::NotFound(path.to_owned()),
                StatusCode::PermissionDenied => Self::PermissionDenied(path.to_owned()),
                StatusCode::OpUnsupported => {
                    Self::Unsupported(format!("{path}: {}", s.error_message))
                }
                StatusCode::NoConnection | StatusCode::ConnectionLost => {
                    Self::ConnectionLost(s.error_message.clone())
                }
                _ => Self::Remote(format!("{path}: {}", s.error_message)),
            },
            SftpError::Timeout => Self::ConnectionLost(format!("{path}: request timed out")),
            SftpError::IO(m) => Self::ConnectionLost(format!("{path}: {m}")),
            other => Self::Remote(format!("{path}: {other}")),
        }
    }

    /// Maps an I/O error produced by a remote file stream.
    pub(crate) fn stream(e: &io::Error, path: &str) -> Self {
        Self::Io(format!("{path}: {e}"))
    }

    /// Whether retrying (after reconnecting) may succeed.
    #[must_use]
    pub fn is_connection_error(&self) -> bool {
        matches!(self, Self::ConnectionLost(_))
    }
}
