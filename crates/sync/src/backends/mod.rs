// SPDX-License-Identifier: GPL-3.0-or-later
//! [`crate::SyncBackend`] implementations.

pub mod folder;
pub mod git;
pub mod s3;
pub mod webdav;

pub use folder::FolderBackend;
pub use git::{GitBackend, GitTransport, LocalRepoTransport};
pub use s3::{Addressing, S3Backend, S3Config};
pub use webdav::{WebDavBackend, WebDavCredentials};
