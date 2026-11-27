// SPDX-License-Identifier: GPL-3.0-or-later
//! Async SSH client engine for ssh-client, built on [`russh`] with pure-Rust cryptography
//! (`ring` + RustCrypto; no OpenSSL, no aws-lc) so it builds for desktop and mobile alike.
//!
//! The crate is app-agnostic: storage, prompts and hardware keys are supplied by the app
//! through three traits:
//!
//! * [`Interaction`] — host key prompts, passwords, passphrases, keyboard-interactive,
//!   banners and agent-use confirmation;
//! * [`KnownHosts`] — lookup / persistence of trusted host keys
//!   ([`MemoryKnownHosts`] is a ready-made in-memory implementation);
//! * [`ExternalSigner`] — optional, for FIDO2 / Secure Enclave / TPM keys.
//!
//! # Connecting
//!
//! ```no_run
//! # async fn demo(ui: std::sync::Arc<dyn ssh_engine::Interaction>) -> Result<(), ssh_engine::SshError> {
//! use ssh_engine::{AuthMethod, ConnectOptions, Hop, MemoryKnownHosts, ShellRequest};
//! use std::sync::Arc;
//!
//! let jump = Hop::new("bastion.example.com", 22, "alice").with_auth(AuthMethod::Agent);
//! let target = Hop::new("10.0.0.5", 22, "alice").with_auth(AuthMethod::password("secret"));
//! let mut opts = ConnectOptions::new(target);
//! opts.hops.insert(0, jump);
//! let conn = ssh_engine::connect(opts, ui, Arc::new(MemoryKnownHosts::default())).await?;
//!
//! let out = conn.exec("uname -a").await?;
//! let (shell, mut events) = conn.open_shell(ShellRequest::default()).await?;
//! shell.write(b"ls\n").await?;
//! while let Some(event) = events.recv().await {
//!     println!("{event:?}"); // render in the terminal widget
//! }
//! # Ok(()) }
//! ```
//!
//! # Architecture
//!
//! * [`connect`] dials the first hop directly or through an HTTP CONNECT / SOCKS5
//!   [`proxy`], then reaches each further hop through a `direct-tcpip` channel of the
//!   previous one (`ProxyJump`). Every hop gets its own host key verification ([`known_hosts`])
//!   and authentication ([`AuthMethod`], tried in order with partial-success handling).
//! * The connect timeout covers TCP, proxy, handshake and authentication of each hop but
//!   pauses while a prompt waits for the user.
//! * Algorithms default to modern, secure choices (see [`algorithms`]); post-quantum
//!   `mlkem768x25519-sha256` is preferred when enabled.
//! * A closed connection reports a [`CloseReason`] whose [`CloseCategory`] tells the app
//!   whether reconnecting makes sense.
//! * [`Connection`] offers shells, exec, SFTP and agent forwarding (an in-process,
//!   read-only agent serving the target hop's keys).
//!
//! # Secrets
//!
//! Passwords, passphrases and private keys are held in [`zeroize::Zeroizing`] buffers, their
//! `Debug` output is redacted and they are never logged.

#![forbid(unsafe_code)]

mod agent;
pub mod algorithms;
mod auth;
mod connection;
mod error;
pub mod fingerprint;
mod handler;
mod interaction;
pub mod keys;
pub mod known_hosts;
mod options;
pub mod proxy;
mod session;
mod signer;

pub use algorithms::{AlgorithmOptions, NegotiatedAlgorithms};
pub use connection::{Connection, HopInfo, connect};
pub use error::{CloseCategory, CloseReason, HostKeyRejection, SshError};
pub use interaction::{AgentSignRequest, HostKeyDecision, Interaction, Prompt};
pub use known_hosts::{
    HostCertificateInfo, HostKeyCheck, HostKeyStatus, KnownHosts, MemoryKnownHosts,
};
pub use options::{AuthMethod, ConnectOptions, ExternalSigner, Hop, SignerError};
pub use proxy::{Proxy, ProxyAuth, ProxyKind};
pub use session::{ExecOptions, ExecOutput, ShellEvent, ShellHandle, ShellRequest, Signal};

/// Re-exported dependencies whose types appear in the public API.
pub use russh::keys::ssh_key;
pub use russh_sftp;
pub use zeroize::Zeroizing;
