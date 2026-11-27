// SPDX-License-Identifier: GPL-3.0-or-later
//! Error and close-reason types.

use std::fmt;
use std::time::Duration;

/// Broad category of why a connection attempt failed or an established connection closed.
///
/// The app uses it to decide what to do next: [`CloseCategory::Network`] is the only category
/// for which an automatic reconnect makes sense.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CloseCategory {
    /// TCP/proxy failure, timeout, keepalive timeout, connection reset. Retrying may help.
    Network,
    /// The server rejected every authentication method (or the user cancelled a prompt).
    AuthFailed,
    /// The host key was rejected (unknown under `Strict`, changed, revoked or user said no).
    HostKeyRejected,
    /// The server closed the connection or violated the protocol.
    Remote,
    /// We closed the connection ([`crate::Connection::close`]) or a local error occurred.
    Local,
}

/// Why a connection was closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseReason {
    pub category: CloseCategory,
    /// Human-readable description (never contains secrets).
    pub message: String,
}

impl CloseReason {
    #[must_use]
    pub fn new(category: CloseCategory, message: impl Into<String>) -> Self {
        Self {
            category,
            message: message.into(),
        }
    }

    /// `true` when an automatic reconnect is reasonable.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        self.category == CloseCategory::Network
    }
}

impl fmt::Display for CloseReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.category, self.message)
    }
}

/// Why a host key was not accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyRejection {
    /// The key (or the CA that signed its certificate) is marked `@revoked`.
    Revoked,
    /// The host is unknown and the policy is `Strict`.
    UnknownStrict,
    /// The key differs from the known key(s) and the policy is `Strict`.
    ChangedStrict,
    /// The user (the [`crate::Interaction`] implementation) declined the key.
    Declined,
}

/// Errors returned by this crate.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SshError {
    /// Invalid caller-provided options.
    #[error("invalid configuration: {0}")]
    Config(String),
    /// The connection attempt (TCP + proxy + handshake + auth of one hop) timed out.
    #[error("timed out after {0:?}")]
    Timeout(Duration),
    /// TCP-level I/O error.
    #[error("network error: {0}")]
    Network(#[source] std::io::Error),
    /// The proxy refused the request or spoke an unexpected protocol.
    #[error("proxy error: {0}")]
    Proxy(String),
    /// Host key verification failed.
    #[error("host key for {host}:{port} rejected ({reason:?})")]
    HostKeyRejected {
        host: String,
        port: u16,
        reason: HostKeyRejection,
    },
    /// No configured or interactive authentication method succeeded.
    #[error("authentication failed for {hop} (server still accepts: {remaining:?})")]
    AuthFailed { hop: String, remaining: Vec<String> },
    /// A private key could not be decoded / decrypted.
    #[error("key error: {0}")]
    Key(#[from] crate::keys::KeyError),
    /// SSH protocol error reported by russh.
    #[error("ssh protocol error: {0}")]
    Protocol(#[from] russh::Error),
    /// SFTP subsystem error.
    #[error("sftp error: {0}")]
    Sftp(#[from] russh_sftp::client::error::Error),
    /// The server refused to open a channel or to set up a forward.
    #[error("request refused by server: {0}")]
    Refused(String),
    /// A command produced more output than allowed.
    #[error("output limit of {0} bytes exceeded")]
    OutputLimit(usize),
    /// The remote command or helper produced unexpected output.
    #[error("unexpected remote response: {0}")]
    UnexpectedResponse(String),
    /// The connection is closed.
    #[error("connection closed: {0}")]
    Closed(CloseReason),
}

impl SshError {
    /// Category used for reconnect decisions.
    #[must_use]
    pub fn category(&self) -> CloseCategory {
        match self {
            Self::Timeout(_) | Self::Network(_) | Self::Proxy(_) => CloseCategory::Network,
            Self::HostKeyRejected { .. } => CloseCategory::HostKeyRejected,
            Self::AuthFailed { .. } | Self::Key(_) => CloseCategory::AuthFailed,
            Self::Protocol(e) => classify_russh(e),
            Self::Sftp(_) | Self::Refused(_) | Self::UnexpectedResponse(_) => CloseCategory::Remote,
            Self::Config(_) | Self::OutputLimit(_) => CloseCategory::Local,
            Self::Closed(r) => r.category,
        }
    }

    /// `true` when retrying the operation (or reconnecting) may succeed.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        self.category() == CloseCategory::Network
    }
}

impl From<std::io::Error> for SshError {
    fn from(e: std::io::Error) -> Self {
        Self::Network(e)
    }
}

/// Maps a russh error to a [`CloseCategory`].
pub(crate) fn classify_russh(e: &russh::Error) -> CloseCategory {
    use russh::Error as E;
    match e {
        E::IO(_)
        | E::HUP
        | E::ConnectionTimeout
        | E::KeepaliveTimeout
        | E::InactivityTimeout
        | E::Elapsed(_)
        | E::Disconnect
        | E::SendError
        | E::RecvError => CloseCategory::Network,
        E::UnknownKey | E::KeyChanged { .. } => CloseCategory::HostKeyRejected,
        E::NotAuthenticated | E::NoAuthMethod => CloseCategory::AuthFailed,
        E::InvalidConfig(_) => CloseCategory::Local,
        _ => CloseCategory::Remote,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories() {
        assert!(SshError::Timeout(Duration::from_secs(1)).is_retryable());
        assert!(!SshError::Config("x".into()).is_retryable());
        assert_eq!(
            SshError::Protocol(russh::Error::KeepaliveTimeout).category(),
            CloseCategory::Network
        );
        assert_eq!(
            SshError::HostKeyRejected {
                host: "h".into(),
                port: 22,
                reason: HostKeyRejection::Revoked
            }
            .category(),
            CloseCategory::HostKeyRejected
        );
        assert!(CloseReason::new(CloseCategory::Network, "reset").is_retryable());
    }
}
