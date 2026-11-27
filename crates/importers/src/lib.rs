// SPDX-License-Identifier: GPL-3.0-or-later
//! Importers that convert other SSH clients' configuration into [`core_model`] entities.
//!
//! Supported sources:
//!
//! | Format | Module |
//! |---|---|
//! | OpenSSH `ssh_config` | [`ssh_config`] |
//! | OpenSSH `known_hosts` | [`known_hosts`] |
//! | CSV ([`csv::TEMPLATE_HEADER`]) | [`csv`] |
//! | PuTTY registry export (`.reg`) | [`putty`] |
//! | MobaXterm `.mobaconf` / `.mxtsession` | [`mobaxterm`] |
//! | SecureCRT XML export | [`securecrt`] |
//!
//! # Design
//!
//! * **Pure.** Parsers take `&str` / `&[u8]` and never touch the file system, the network or
//!   the vault. Files referenced by the source (OpenSSH `Include`, `IdentityFile`, PuTTY
//!   `.ppk` paths) are returned to the caller as paths or resolved through a callback.
//! * **Secrets stay out of entities.** Passwords found in the source are returned as
//!   [`ImportedSecret`]s whose [`SecretRef`](core_model::SecretRef) slot ids are already used
//!   in the produced hosts. The caller stores them in the vault. `Debug` of every
//!   secret-bearing type is redacted and warnings never contain secret values.
//! * **Untrusted input.** Inputs larger than [`MAX_INPUT_BYTES`] are rejected, nesting and
//!   recursion are bounded, malformed entries are skipped with an [`ImportWarning`], and no
//!   input can cause a panic.
//! * **Clean output.** Every produced host, group and port forward is validated with
//!   [`core_model::Validate`] (invalid ones are dropped with a warning), and hosts are
//!   deduplicated by `(address, port, username)` within one import.

#![forbid(unsafe_code)]

mod bundle;
mod common;
mod error;
mod finalize;
pub mod known_hosts;
pub mod ssh_config;
mod text;
mod wire;

pub use bundle::{
    ImportBundle, ImportWarning, ImportedKey, ImportedSecret, KeyFileFormat, KeyMaterial,
    SecretKind, WarningKind,
};
pub use error::ImportError;
pub use text::MAX_INPUT_BYTES;
pub use wire::sha256_fingerprint;
