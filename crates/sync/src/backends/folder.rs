// SPDX-License-Identifier: GPL-3.0-or-later
//! A local folder, typically one kept in sync by another tool (iCloud Drive, Dropbox,
//! Syncthing, Nextcloud desktop, a mounted network share).
//!
//! Writes are atomic: data goes to a hidden temporary file in the same directory, is flushed
//! to disk and then moved into place. Create-only writes use a hard link (which fails if the
//! target exists); on file systems without hard links they fall back to an existence check
//! followed by a rename (a tiny race window, harmless here because segment names are random).
//!
//! Conflict copies created by sync tools and anything that is not a valid object name are
//! skipped during listing (and logged).

use crate::backend::{
    ObjectInfo, Precondition, SyncBackend, is_conflict_copy, validate_name, validate_prefix,
};
use crate::error::SyncError;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

/// Prefix of temporary files.
const TMP_PREFIX: &str = ".tmp-";
/// Maximum directory depth walked when listing.
const MAX_DEPTH: usize = 8;

/// Sync backend on a local directory.
#[derive(Debug, Clone)]
pub struct FolderBackend {
    root: PathBuf,
}

impl FolderBackend {
    /// Uses `root` as the backend root (it is created on first write).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path_of(&self, name: &str) -> Result<PathBuf, SyncError> {
        validate_name(name)?;
        let mut p = self.root.clone();
        for seg in name.split('/') {
            p.push(seg);
        }
        Ok(p)
    }
}

fn etag_of(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

async fn write_temp(dir: &Path, bytes: &[u8]) -> Result<PathBuf, SyncError> {
    let tmp = dir.join(format!(
        "{TMP_PREFIX}{}",
        hex::encode(crate::keys::random::<8>()?)
    ));
    let mut f = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .await?;
    let res = async {
        f.write_all(bytes).await?;
        f.flush().await?;
        f.sync_all().await
    }
    .await;
    drop(f);
    if let Err(e) = res {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(e.into());
    }
    Ok(tmp)
}

#[async_trait]
impl SyncBackend for FolderBackend {
    async fn list(&self, prefix: &str) -> Result<Vec<ObjectInfo>, SyncError> {
        validate_prefix(prefix)?;
        // Walk the deepest directory implied by the prefix.
        let dir_part = prefix.rsplit_once('/').map_or("", |(d, _)| d);
        let mut start = self.root.clone();
        for seg in dir_part.split('/').filter(|s| !s.is_empty()) {
            start.push(seg);
        }
        let mut out = Vec::new();
        let mut stack = vec![(start, dir_part.to_owned(), 0usize)];
        while let Some((dir, rel, depth)) = stack.pop() {
            let mut rd = match tokio::fs::read_dir(&dir).await {
                Ok(rd) => rd,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            while let Some(entry) = rd.next_entry().await? {
                let file_name = entry.file_name();
                let Some(file_name) = file_name.to_str() else {
                    tracing::warn!("skipping non-UTF-8 file name");
                    continue;
                };
                let name = if rel.is_empty() {
                    file_name.to_owned()
                } else {
                    format!("{rel}/{file_name}")
                };
                if file_name.starts_with(TMP_PREFIX) {
                    continue;
                }
                if is_conflict_copy(&name) {
                    tracing::warn!(name, "skipping conflict copy created by a sync tool");
                    continue;
                }
                if validate_name(&name).is_err() {
                    tracing::debug!(name, "skipping unexpected file");
                    continue;
                }
                let ft = entry.file_type().await?;
                if ft.is_dir() {
                    if depth < MAX_DEPTH {
                        stack.push((entry.path(), name, depth + 1));
                    }
                } else if ft.is_file() && name.starts_with(prefix) {
                    let meta = entry.metadata().await?;
                    out.push(ObjectInfo {
                        name,
                        size: meta.len(),
                        etag: None,
                    });
                }
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    async fn get(&self, name: &str) -> Result<Vec<u8>, SyncError> {
        let path = self.path_of(name)?;
        match tokio::fs::read(&path).await {
            Ok(b) => Ok(b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(SyncError::NotFound(name.to_owned()))
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn put(
        &self,
        name: &str,
        bytes: &[u8],
        precondition: Option<Precondition>,
    ) -> Result<String, SyncError> {
        let path = self.path_of(name)?;
        let dir = path
            .parent()
            .ok_or_else(|| SyncError::InvalidName(name.to_owned()))?
            .to_path_buf();
        tokio::fs::create_dir_all(&dir).await?;
        if let Some(Precondition::IfMatch(etag)) = &precondition {
            let current = match tokio::fs::read(&path).await {
                Ok(b) => etag_of(&b),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(e) => return Err(e.into()),
            };
            if &current != etag {
                return Err(SyncError::PreconditionFailed(name.to_owned()));
            }
        }
        let tmp = write_temp(&dir, bytes).await?;
        let result = match precondition {
            Some(Precondition::IfNoneMatch) => match tokio::fs::hard_link(&tmp, &path).await {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    Err(SyncError::PreconditionFailed(name.to_owned()))
                }
                Err(_) => {
                    // No hard links on this file system (FAT, some FUSE mounts).
                    if tokio::fs::try_exists(&path).await? {
                        Err(SyncError::PreconditionFailed(name.to_owned()))
                    } else {
                        tokio::fs::rename(&tmp, &path)
                            .await
                            .map_err(SyncError::from)
                    }
                }
            },
            _ => tokio::fs::rename(&tmp, &path)
                .await
                .map_err(SyncError::from),
        };
        // After a hard link (or a failure) the temp file still exists.
        let _ = tokio::fs::remove_file(&tmp).await;
        result.map(|()| etag_of(bytes))
    }

    async fn delete(&self, name: &str) -> Result<(), SyncError> {
        let path = self.path_of(name)?;
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[tokio::test]
    async fn crud_and_preconditions() {
        let dir = tempfile::tempdir().unwrap();
        let b = FolderBackend::new(dir.path().join("root"));
        assert!(b.list("a/").await.unwrap().is_empty());
        let etag = b
            .put("a/b/c.seg", b"one", Some(Precondition::IfNoneMatch))
            .await
            .unwrap();
        assert!(matches!(
            b.put("a/b/c.seg", b"two", Some(Precondition::IfNoneMatch))
                .await,
            Err(SyncError::PreconditionFailed(_))
        ));
        assert!(matches!(
            b.put(
                "a/b/c.seg",
                b"two",
                Some(Precondition::IfMatch("nope".into()))
            )
            .await,
            Err(SyncError::PreconditionFailed(_))
        ));
        b.put("a/b/c.seg", b"two", Some(Precondition::IfMatch(etag)))
            .await
            .unwrap();
        assert_eq!(b.get("a/b/c.seg").await.unwrap(), b"two");
        b.put("a/d.seg", b"x", None).await.unwrap();
        b.put("other/e.seg", b"x", None).await.unwrap();
        // Junk a sync tool might leave behind.
        std::fs::write(dir.path().join("root/a/b/c (conflicted copy).seg"), b"x").unwrap();
        std::fs::write(dir.path().join("root/a/.tmp-123"), b"x").unwrap();
        let names: Vec<String> = b
            .list("a/")
            .await
            .unwrap()
            .into_iter()
            .map(|o| o.name)
            .collect();
        assert_eq!(names, vec!["a/b/c.seg".to_owned(), "a/d.seg".into()]);
        assert_eq!(b.list("").await.unwrap().len(), 3);
        b.delete("a/d.seg").await.unwrap();
        b.delete("a/d.seg").await.unwrap();
        assert!(matches!(
            b.get("a/d.seg").await,
            Err(SyncError::NotFound(_))
        ));
        assert!(matches!(
            b.get("../x").await,
            Err(SyncError::InvalidName(_))
        ));
    }
}
