// SPDX-License-Identifier: GPL-3.0-or-later
//! File management for ssh-client: one async [`FileSystem`] trait over the local disk
//! ([`Local`], `tokio::fs`) and SFTP ([`Remote`], `russh-sftp`) and a [`TransferQueue`].
//!
//! # Example
//!
//! ```no_run
//! # async fn demo(sftp: russh_sftp::client::SftpSession) -> Result<(), sftp_engine::FsError> {
//! use sftp_engine::{FileSystem, Local, Remote, TransferOptions, TransferQueue, TransferRequest};
//! use std::sync::Arc;
//!
//! let remote: Arc<dyn FileSystem> = Arc::new(Remote::new(sftp));
//! let home = remote.home().await?;
//! for entry in remote.list(&home).await? {
//!     println!("{} {:>10} {}", entry.mode_string(), entry.size, entry.name);
//! }
//!
//! let (queue, mut events) = TransferQueue::new(3, None);
//! let id = queue.enqueue(TransferRequest {
//!     source: remote.clone(),
//!     source_path: format!("{home}/backup.tar"),
//!     destination: Arc::new(Local),
//!     destination_path: "/tmp/backup.tar".into(),
//!     options: TransferOptions::default(),
//! });
//! while let Some(event) = events.recv().await {
//!     println!("{event:?}");
//! #   break;
//! }
//! queue.wait(id).await;
//! # Ok(()) }
//! ```
//!
//! # Security
//!
//! * Entry names returned by a server are untrusted. They are validated before being joined
//!   into any path ([`path`]); [`Remote::list`](FileSystem::list) drops names such as
//!   `../../.bashrc`, and transfers skip (and report) anything that is not a plain name.
//! * Recursive deletion never follows symlinks.
//! * Transfers write to `*.sshc-part` files and rename on completion, so an interrupted
//!   transfer never leaves a truncated file under the real name (and can be resumed).

#![forbid(unsafe_code)]

mod error;
mod fs;
mod local;
pub mod path;
mod remote;
pub mod transfer;

pub use error::FsError;
pub use fs::{
    Entry, EntryKind, FileReader, FileSystem, FileWriter, FsKind, RemoveLimits, RemoveStats,
    WriteMode, mode_string,
};
pub use local::Local;
pub use remote::Remote;
pub use transfer::{
    Conflict, ConflictDecision, ConflictPolicy, ConflictResolver, TransferEvent, TransferId,
    TransferOptions, TransferProgress, TransferQueue, TransferRequest, TransferState,
};

/// Re-export of the SFTP client crate.
pub use russh_sftp;
