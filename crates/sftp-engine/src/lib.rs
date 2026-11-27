// SPDX-License-Identifier: GPL-3.0-or-later
//! File management for ssh-client: one async [`FileSystem`] trait over the local disk
//! ([`Local`], `tokio::fs`) and SFTP ([`Remote`], `russh-sftp`).
//!
//! # Security
//!
//! * Entry names returned by a server are untrusted. They are validated before being joined
//!   into any path ([`path`]); [`Remote::list`](FileSystem::list) drops names such as
//!   `../../.bashrc`.
//! * Recursive deletion never follows symlinks.

#![forbid(unsafe_code)]

mod error;
mod fs;
mod local;
pub mod path;
mod remote;

pub use error::FsError;
pub use fs::{
    Entry, EntryKind, FileReader, FileSystem, FileWriter, FsKind, RemoveLimits, RemoveStats,
    WriteMode, mode_string,
};
pub use local::Local;
pub use remote::Remote;

/// Re-export of the SFTP client crate.
pub use russh_sftp;
