// SPDX-License-Identifier: GPL-3.0-or-later
use thiserror::Error;

/// Errors returned by the vault.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum VaultError {
    #[error("the vault is locked")]
    Locked,
    #[error("wrong password or key, or the data has been tampered with")]
    Authentication,
    #[error("no unlock method of this kind is configured")]
    NoSuchUnlockMethod,
    #[error("the vault must keep at least one unlock method")]
    LastUnlockMethod,
    #[error("vault data is corrupt: {0}")]
    Corrupt(String),
    #[error("cryptographic failure: {0}")]
    Crypto(String),
    #[error("record not found: {0}")]
    NotFound(String),
    #[error("invalid record: {0}")]
    Invalid(String),
    #[error("unsupported vault version {0}")]
    UnsupportedVersion(u32),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("validation failed: {0}")]
    Validation(#[from] core_model::ModelError),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}
