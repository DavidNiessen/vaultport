// SPDX-License-Identifier: GPL-3.0-or-later
//! Errors returned to the UI. Messages are user-presentable and never contain secrets.

use serde::Serialize;
use specta::Type;

/// Machine-readable error category for the UI.
#[allow(dead_code, reason = "variants are produced by protocol engines")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    Locked,
    NoVault,
    Authentication,
    NotFound,
    Invalid,
    Conflict,
    HostKeyMismatch,
    HostKeyUnknown,
    AuthFailed,
    Network,
    Timeout,
    Unsupported,
    Cancelled,
    Internal,
}

/// Error returned by every command.
#[derive(Debug, Clone, Serialize, Type, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{message}")]
pub struct CmdError {
    pub code: ErrorCode,
    pub message: String,
}

impl CmdError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Invalid, message)
    }

    #[allow(dead_code)]
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }
}

impl From<vault::VaultError> for CmdError {
    fn from(e: vault::VaultError) -> Self {
        use vault::VaultError as V;
        let code = match &e {
            V::Locked => ErrorCode::Locked,
            V::Authentication => ErrorCode::Authentication,
            V::NotFound(_) => ErrorCode::NotFound,
            V::Invalid(_) | V::Validation(_) | V::LastUnlockMethod | V::NoSuchUnlockMethod => {
                ErrorCode::Invalid
            }
            _ => ErrorCode::Internal,
        };
        if code == ErrorCode::Internal {
            tracing::error!(error = %e, "vault error");
        }
        Self::new(code, e.to_string())
    }
}

impl From<core_model::ModelError> for CmdError {
    fn from(e: core_model::ModelError) -> Self {
        Self::invalid(e.to_string())
    }
}

impl From<std::io::Error> for CmdError {
    fn from(e: std::io::Error) -> Self {
        Self::internal(e.to_string())
    }
}

impl From<tauri::Error> for CmdError {
    fn from(e: tauri::Error) -> Self {
        Self::internal(e.to_string())
    }
}

pub type CmdResult<T> = Result<T, CmdError>;

/// An arbitrary JSON value, exported to TypeScript as `unknown`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct JsonValue(pub serde_json::Value);

impl specta::Type for JsonValue {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        <specta_typescript::Unknown as specta::Type>::definition(types)
    }
}
