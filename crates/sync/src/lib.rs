// SPDX-License-Identifier: GPL-3.0-or-later
//! Serverless, end-to-end encrypted vault sync through **user-owned storage only**.
//!
//! There is no project-operated server. A vault is replicated through storage the user
//! already has — a synced folder (iCloud Drive, Dropbox, Syncthing), WebDAV (Nextcloud), any
//! S3-compatible bucket, or a Git repository — which only ever sees ciphertext:
//!
//! * the vault's ops are already sealed with the vault key ([`vault::SealedOp`]);
//! * they travel in immutable **segment** files ([`segment`]) that add an outer AEAD layer
//!   with a per-device hash chain, so the storage can neither read, forge, reorder nor
//!   silently drop data without being detected ([`engine`]);
//! * the vault is a CRDT, so concurrent edits on several devices merge without conflicts.
//!
//! # What the app provides
//!
//! * Persistence of [`SyncState`] (one per backend; [`SyncState::load`]/[`SyncState::save`]
//!   keep it in the vault as a device-local settings document).
//! * Backend credentials (stored as vault secrets).
//! * Scheduling (sync on change / on a timer) and surfacing [`engine::Issue`]s to the user.

#![forbid(unsafe_code)]

mod backend;
pub mod backends;
pub mod engine;
mod error;
mod keys;
pub mod segment;
mod state;

pub use backend::{
    ObjectInfo, Precondition, SyncBackend, is_conflict_copy, validate_name, validate_prefix,
};
pub use engine::{
    CompactOptions, CompactReport, Issue, SyncEngine, SyncOptions, SyncReport, Uploaded,
};
pub use error::SyncError;
pub use state::{Head, STATE_SETTINGS_PREFIX, SyncState};
