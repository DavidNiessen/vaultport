// SPDX-License-Identifier: GPL-3.0-or-later
//! Async SSH client engine for ssh-client, built on [`russh`] with pure-Rust cryptography
//! (`ring` + RustCrypto; no OpenSSL, no aws-lc) so it builds for desktop and mobile alike.

#![forbid(unsafe_code)]

pub mod algorithms;
pub mod fingerprint;
pub mod known_hosts;
pub mod proxy;

pub use algorithms::{AlgorithmOptions, NegotiatedAlgorithms};
pub use known_hosts::{
    HostCertificateInfo, HostKeyCheck, HostKeyStatus, KnownHosts, MemoryKnownHosts,
};
pub use proxy::{Proxy, ProxyAuth, ProxyKind};

/// Re-exported dependencies whose types appear in the public API.
pub use russh::keys::ssh_key;
pub use zeroize::Zeroizing;
