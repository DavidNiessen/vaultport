// SPDX-License-Identifier: GPL-3.0-or-later
//! Error type shared by all providers.

use std::time::Duration;

/// Errors returned by inventory clients.
///
/// Messages never contain credentials; provider error messages are truncated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InventoryError {
    /// The provider rejected the credentials or the credentials lack permissions
    /// (HTTP 401/403 or a provider-specific authentication error code).
    #[error("authentication failed: {message}")]
    AuthError { message: String },
    /// The provider throttled the request (HTTP 429 or a throttling error code).
    #[error("rate limited by the provider")]
    RateLimited { retry_after: Option<Duration> },
    /// Any other unsuccessful HTTP response.
    #[error("provider returned HTTP {status}: {message}")]
    Http { status: u16, message: String },
    /// Connection, TLS or timeout failure.
    #[error("network error: {0}")]
    Network(String),
    /// The response could not be decoded.
    #[error("unexpected response: {0}")]
    Decode(String),
    /// The integration settings or credentials are invalid.
    #[error("invalid configuration: {0}")]
    Config(String),
}

impl From<reqwest::Error> for InventoryError {
    fn from(e: reqwest::Error) -> Self {
        Self::Network(e.without_url().to_string())
    }
}

impl From<crate::sigv4::SignError> for InventoryError {
    fn from(e: crate::sigv4::SignError) -> Self {
        Self::Config(e.to_string())
    }
}
