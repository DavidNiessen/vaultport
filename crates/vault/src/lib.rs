// SPDX-License-Identifier: GPL-3.0-or-later
//! The encrypted local vault.
//!
//! # Design
//!
//! * A random 256-bit **vault key** (VK) encrypts every record and every replicated operation
//!   with XChaCha20-Poly1305. The vault id and the record (or op) id are bound as associated
//!   data, so ciphertexts cannot be swapped between records or vaults.
//! * The VK is stored only **wrapped**: by an Argon2id-derived key from the master password
//!   and/or by a 256-bit **device key** held in the OS key store (Keychain, DPAPI, Secret
//!   Service, Android Keystore). At least one wrap always exists.
//! * Data is an append-only **operation log** with per-field last-writer-wins semantics
//!   ([`crdt`]), materialized into documents. Any two devices that have seen the same set of
//!   operations have the same data, independent of order. This makes sync with dumb,
//!   user-owned storage (a folder, WebDAV, S3, Git) safe without a server.
//! * Secrets (passwords, private keys) are separate records that are only decrypted on demand
//!   and returned in zeroizing buffers.

#![forbid(unsafe_code)]

pub mod crdt;
pub mod crypto;
mod entity;
mod error;
pub mod hlc;
mod store;

pub use crypto::{KdfParams, SymmetricKey};
pub use entity::{Entity, SECRET_KIND, SETTINGS_KIND};
pub use error::VaultError;
pub use store::{
    ChangeEvent, CreateOptions, FORMAT_VERSION, SealedOp, UnlockMethods, Vault, unwrap_exported_key,
};

#[cfg(test)]
mod tests;
