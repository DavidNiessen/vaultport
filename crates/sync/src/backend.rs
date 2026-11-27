// SPDX-License-Identifier: GPL-3.0-or-later
//! The storage abstraction: a flat namespace of immutable-ish objects.
//!
//! Object names are relative, `/`-separated paths made of `[A-Za-z0-9._-]` segments (no `.`
//! or `..`), so no backend can be tricked into a path traversal by a hostile listing.

use crate::error::SyncError;
use async_trait::async_trait;

/// Metadata of a stored object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectInfo {
    /// Name relative to the backend root, e.g. `vault-<id>/segments/<dev>/<...>.seg`.
    pub name: String,
    pub size: u64,
    /// Opaque version tag, when the backend has one.
    pub etag: Option<String>,
}

/// Condition for a write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Precondition {
    /// Only create: fail with [`SyncError::PreconditionFailed`] if the object exists
    /// (`If-None-Match: *`).
    IfNoneMatch,
    /// Only replace the object if its etag still matches (`If-Match: <etag>`).
    IfMatch(String),
}

/// User-owned storage the vault is synced through.
#[async_trait]
pub trait SyncBackend: Send + Sync {
    /// Lists all objects whose name starts with `prefix` (recursively).
    async fn list(&self, prefix: &str) -> Result<Vec<ObjectInfo>, SyncError>;
    /// Reads an object ([`SyncError::NotFound`] if it does not exist).
    async fn get(&self, name: &str) -> Result<Vec<u8>, SyncError>;
    /// Writes an object, creating parent "directories" as needed; returns its new etag.
    async fn put(
        &self,
        name: &str,
        bytes: &[u8],
        precondition: Option<Precondition>,
    ) -> Result<String, SyncError>;
    /// Deletes an object. Deleting a missing object succeeds.
    async fn delete(&self, name: &str) -> Result<(), SyncError>;
}

fn segment_ok(seg: &str) -> bool {
    !seg.is_empty()
        && seg != "."
        && seg != ".."
        && seg.len() <= 255
        && seg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Validates an object name.
pub fn validate_name(name: &str) -> Result<(), SyncError> {
    if name.len() <= 1024 && name.split('/').all(segment_ok) {
        Ok(())
    } else {
        Err(SyncError::InvalidName(name.to_owned()))
    }
}

/// Validates a listing prefix: empty, or a valid name optionally followed by `/`.
pub fn validate_prefix(prefix: &str) -> Result<(), SyncError> {
    let p = prefix.strip_suffix('/').unwrap_or(prefix);
    if p.is_empty() {
        return Ok(());
    }
    validate_name(p)
}

/// Whether a file name looks like a conflict copy created by a file-sync tool (Dropbox,
/// Nextcloud/ownCloud, Syncthing, OneDrive, Google Drive, iCloud).
#[must_use]
pub fn is_conflict_copy(name: &str) -> bool {
    let file = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
    file.contains("conflicted copy")
        || file.contains("(conflict")
        || file.contains("_conflict-")
        || file.contains(".sync-conflict-")
        || file.contains("-conflict-")
        || file.contains(" (1)")
        || file.contains(" (2)")
        || file.ends_with(" 2.seg")
        || file.ends_with(".icloud")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(validate_name("vault-1/segments/ab/0-1-x.seg").is_ok());
        for bad in [
            "", "/abs", "a//b", "../x", "a/./b", "a\\b", "a b", "a/", "ä",
        ] {
            assert!(validate_name(bad).is_err(), "{bad}");
        }
        assert!(validate_prefix("").is_ok());
        assert!(validate_prefix("vault-1/").is_ok());
        assert!(validate_prefix("../").is_err());
    }

    #[test]
    fn conflict_copies() {
        assert!(is_conflict_copy(
            "s/a (David's conflicted copy 2024-01-01).seg"
        ));
        assert!(is_conflict_copy(
            "s/a.sync-conflict-20240101-120000-ABC.seg"
        ));
        assert!(!is_conflict_copy(
            "s/0000000000000001-0000000000000001-ab.seg"
        ));
    }
}
