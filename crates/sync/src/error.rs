// SPDX-License-Identifier: GPL-3.0-or-later
//! Error type of the sync crate.

use std::time::Duration;
use vault::VaultError;

/// Errors returned by backends, the sync engine, export/import and pairing.
///
/// Messages never contain secrets (passwords, tokens, keys or decrypted data).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SyncError {
    /// The object does not exist.
    #[error("object not found: {0}")]
    NotFound(String),
    /// A conditional write failed (`If-None-Match` / `If-Match`).
    #[error("precondition failed for {0}")]
    PreconditionFailed(String),
    /// The storage rejected the credentials.
    #[error("storage authentication failed: {0}")]
    Auth(String),
    /// The storage throttled the request.
    #[error("rate limited by the storage")]
    RateLimited { retry_after: Option<Duration> },
    /// Transport, I/O or protocol failure of a backend.
    #[error("storage error: {0}")]
    Backend(String),
    /// An object name is not acceptable (path traversal, unsupported characters).
    #[error("invalid object name {0:?}")]
    InvalidName(String),
    /// Wrong passphrase / pairing code, or the data was tampered with.
    #[error("wrong passphrase or code, or the data has been tampered with")]
    Authentication,
    /// A file or payload is malformed.
    #[error("malformed data: {0}")]
    Format(String),
    /// A pairing offer has expired.
    #[error("the pairing code has expired")]
    Expired,
    /// The sync state belongs to another vault.
    #[error("sync state belongs to vault {state}, not {vault}")]
    StateMismatch {
        state: uuid::Uuid,
        vault: uuid::Uuid,
    },
    /// The operation is not supported by this backend.
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// An error from the local vault.
    #[error(transparent)]
    Vault(VaultError),
}

impl From<VaultError> for SyncError {
    fn from(e: VaultError) -> Self {
        match e {
            VaultError::Authentication => Self::Authentication,
            other => Self::Vault(other),
        }
    }
}

impl From<std::io::Error> for SyncError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            Self::NotFound(e.to_string())
        } else {
            Self::Backend(e.to_string())
        }
    }
}

impl From<reqwest::Error> for SyncError {
    fn from(e: reqwest::Error) -> Self {
        Self::Backend(e.without_url().to_string())
    }
}

impl From<cloud_inventory::InventoryError> for SyncError {
    fn from(e: cloud_inventory::InventoryError) -> Self {
        use cloud_inventory::InventoryError as E;
        match e {
            E::AuthError { message } => Self::Auth(message),
            E::RateLimited { retry_after } => Self::RateLimited { retry_after },
            other => Self::Backend(other.to_string()),
        }
    }
}

impl From<cloud_inventory::sigv4::SignError> for SyncError {
    fn from(e: cloud_inventory::sigv4::SignError) -> Self {
        Self::Backend(e.to_string())
    }
}
