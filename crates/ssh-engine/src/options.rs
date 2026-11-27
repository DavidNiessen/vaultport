// SPDX-License-Identifier: GPL-3.0-or-later
//! Connection options: hops, authentication methods and transport settings.

use crate::algorithms::AlgorithmOptions;
use crate::proxy::Proxy;
use async_trait::async_trait;
use core_model::HostKeyPolicy;
use russh::keys::ssh_key::{Certificate, HashAlg, PublicKey};
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use zeroize::Zeroizing;

/// Error returned by an [`ExternalSigner`].
#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct SignerError(pub String);

/// A signer whose private key lives outside this process: FIDO2 security keys, the Secure
/// Enclave, a TPM, Android Keystore… implemented by the app.
#[async_trait]
pub trait ExternalSigner: Send + Sync {
    /// The public key (for FIDO2: `sk-ssh-ed25519@openssh.com` / `sk-ecdsa-…`).
    fn public_key(&self) -> PublicKey;

    /// Optional OpenSSH certificate for this key.
    fn certificate(&self) -> Option<Certificate> {
        None
    }

    /// Human-readable label (shown in agent confirmation prompts).
    fn label(&self) -> String {
        self.public_key().comment().to_string()
    }

    /// Signs `data` and returns the SSH **signature blob**: `string algorithm-name ||
    /// string signature` (for `sk-*` keys followed by `byte flags || uint32 counter`), i.e.
    /// what `ssh_key::Signature` encodes to.
    ///
    /// `hash` is only meaningful for RSA keys (`Some(Sha512)` → `rsa-sha2-512`,
    /// `Some(Sha256)` → `rsa-sha2-256`, `None` → legacy `ssh-rsa`).
    async fn sign(&self, data: &[u8], hash: Option<HashAlg>) -> Result<Vec<u8>, SignerError>;
}

/// One way to authenticate. Methods of a [`Hop`] are tried in order.
#[derive(Clone)]
pub enum AuthMethod {
    /// Password authentication.
    Password(Zeroizing<String>),
    /// A private key (OpenSSH, PKCS#8, PKCS#1 PEM or PuTTY PPK v2/v3), optionally with an
    /// OpenSSH certificate (`*-cert.pub` contents). If the key is encrypted and no passphrase
    /// is given, [`crate::Interaction::key_passphrase`] is asked.
    PrivateKey {
        key_pem: Zeroizing<String>,
        passphrase: Option<Zeroizing<String>>,
        certificate: Option<String>,
    },
    /// A key held by the app (hardware-backed).
    External(Arc<dyn ExternalSigner>),
    /// `keyboard-interactive`; prompts go to [`crate::Interaction::keyboard_interactive`].
    KeyboardInteractive,
    /// The system SSH agent: `$SSH_AUTH_SOCK` on Unix, the Windows OpenSSH agent named pipe
    /// (`\\.\pipe\openssh-ssh-agent`, or `$SSH_AUTH_SOCK` if it names a pipe) on Windows.
    /// Silently skipped where no agent is reachable (e.g. iOS/Android).
    Agent,
}

impl AuthMethod {
    /// Convenience constructor for an unencrypted or passphrase-less key.
    #[must_use]
    pub fn key(pem: impl Into<String>) -> Self {
        Self::PrivateKey {
            key_pem: Zeroizing::new(pem.into()),
            passphrase: None,
            certificate: None,
        }
    }

    /// Convenience constructor for a password.
    #[must_use]
    pub fn password(p: impl Into<String>) -> Self {
        Self::Password(Zeroizing::new(p.into()))
    }

    /// The SSH method name (`password`, `publickey`, `keyboard-interactive`).
    #[must_use]
    pub fn method_name(&self) -> &'static str {
        match self {
            Self::Password(_) => "password",
            Self::PrivateKey { .. } | Self::External(_) | Self::Agent => "publickey",
            Self::KeyboardInteractive => "keyboard-interactive",
        }
    }
}

impl fmt::Debug for AuthMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Password(_) => f.write_str("Password(<redacted>)"),
            Self::PrivateKey {
                passphrase,
                certificate,
                ..
            } => f
                .debug_struct("PrivateKey")
                .field("key_pem", &"<redacted>")
                .field("passphrase", &passphrase.as_ref().map(|_| "<redacted>"))
                .field("certificate", &certificate.is_some())
                .finish(),
            Self::External(s) => f
                .debug_tuple("External")
                .field(&s.public_key().algorithm().as_str())
                .finish(),
            Self::KeyboardInteractive => f.write_str("KeyboardInteractive"),
            Self::Agent => f.write_str("Agent"),
        }
    }
}

/// One SSH server in the chain.
#[derive(Debug, Clone)]
pub struct Hop {
    /// Host name or IP address as typed by the user (also the known-hosts lookup key).
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Authentication methods, tried in order.
    pub auth: Vec<AuthMethod>,
    pub host_key_policy: HostKeyPolicy,
    /// After the configured methods are exhausted, fall back to interactive
    /// `keyboard-interactive` and `password` prompts when the server allows them (like
    /// OpenSSH). Default `true`.
    pub interactive_fallback: bool,
}

impl Hop {
    #[must_use]
    pub fn new(host: impl Into<String>, port: u16, username: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port,
            username: username.into(),
            auth: Vec::new(),
            host_key_policy: HostKeyPolicy::Ask,
            interactive_fallback: true,
        }
    }

    /// Adds an authentication method (builder style).
    #[must_use]
    pub fn with_auth(mut self, m: AuthMethod) -> Self {
        self.auth.push(m);
        self
    }

    /// `user@host:port` label used in prompts and errors.
    #[must_use]
    pub fn label(&self) -> String {
        if self.host.contains(':') {
            format!("{}@[{}]:{}", self.username, self.host, self.port)
        } else {
            format!("{}@{}:{}", self.username, self.host, self.port)
        }
    }
}

/// Options for [`crate::connect`].
#[derive(Debug, Clone)]
pub struct ConnectOptions {
    /// Jump hosts first, target last. Must not be empty.
    pub hops: Vec<Hop>,
    /// Proxy used to reach the first hop.
    pub proxy: Option<Proxy>,
    /// Timeout for TCP connect + proxy + handshake (per hop). Authentication prompts waiting
    /// for the user are not counted.
    pub connect_timeout: Duration,
    /// Interval of keepalive probes; after 3 unanswered probes the connection is closed with
    /// [`crate::CloseCategory::Network`].
    pub keepalive: Option<Duration>,
    pub legacy_algorithms: bool,
    pub post_quantum: bool,
    pub compression: bool,
    /// Ask [`crate::Interaction::confirm_agent_use`] before every signature made by the
    /// forwarded agent.
    pub agent_confirm: bool,
    /// Client identification suffix, sent as `SSH-2.0-<client_id>`.
    pub client_id: String,
}

impl ConnectOptions {
    /// Options with secure defaults for a single hop.
    #[must_use]
    pub fn new(target: Hop) -> Self {
        Self {
            hops: vec![target],
            proxy: None,
            connect_timeout: Duration::from_secs(u64::from(
                core_model::Defaults::CONNECT_TIMEOUT_SECS,
            )),
            keepalive: Some(Duration::from_secs(u64::from(
                core_model::Defaults::KEEPALIVE_SECS,
            ))),
            legacy_algorithms: false,
            post_quantum: true,
            compression: false,
            agent_confirm: true,
            client_id: concat!("vaultport_", env!("CARGO_PKG_VERSION")).to_owned(),
        }
    }

    /// Applies the transport settings of a resolved [`core_model::SshConfig`] (keepalive,
    /// timeouts, algorithm switches, host key policy of every hop). Credentials, jump chain and
    /// proxy password must be resolved by the app (they live in the vault).
    pub fn apply_ssh_config(&mut self, cfg: &core_model::SshConfig) {
        if let Some(k) = cfg.keepalive_secs {
            self.keepalive = (k > 0).then(|| Duration::from_secs(u64::from(k)));
        }
        if let Some(t) = cfg.connect_timeout_secs.filter(|t| *t > 0) {
            self.connect_timeout = Duration::from_secs(u64::from(t));
        }
        if let Some(v) = cfg.legacy_algorithms {
            self.legacy_algorithms = v;
        }
        if let Some(v) = cfg.post_quantum {
            self.post_quantum = v;
        }
        if let Some(v) = cfg.compression {
            self.compression = v;
        }
        if let Some(v) = cfg.agent_confirm {
            self.agent_confirm = v;
        }
        if let Some(p) = cfg.host_key_policy {
            for h in &mut self.hops {
                h.host_key_policy = p;
            }
        }
    }

    pub(crate) fn algorithm_options(&self) -> AlgorithmOptions {
        AlgorithmOptions {
            legacy: self.legacy_algorithms,
            post_quantum: self.post_quantum,
            compression: self.compression,
        }
    }

    /// The final hop.
    #[must_use]
    pub fn target(&self) -> Option<&Hop> {
        self.hops.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_secrets() {
        let m = vec![
            AuthMethod::password("hunter2"),
            AuthMethod::PrivateKey {
                key_pem: Zeroizing::new("-----BEGIN SECRET-----".into()),
                passphrase: Some(Zeroizing::new("pass-phrase".into())),
                certificate: None,
            },
        ];
        let s = format!("{m:?}");
        assert!(!s.contains("hunter2"));
        assert!(!s.contains("SECRET"));
        assert!(!s.contains("pass-phrase"));
    }

    #[test]
    fn apply_config() {
        let mut o = ConnectOptions::new(Hop::new("h", 22, "u"));
        let cfg = core_model::SshConfig {
            keepalive_secs: Some(0),
            post_quantum: Some(false),
            host_key_policy: Some(HostKeyPolicy::Strict),
            ..Default::default()
        };
        o.apply_ssh_config(&cfg);
        assert_eq!(o.keepalive, None);
        assert!(!o.post_quantum);
        assert_eq!(o.hops[0].host_key_policy, HostKeyPolicy::Strict);
        assert_eq!(Hop::new("::1", 2222, "me").label(), "me@[::1]:2222");
    }
}
