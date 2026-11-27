// SPDX-License-Identifier: GPL-3.0-or-later
//! A remote file system over SFTP (`russh-sftp`).

use crate::error::FsError;
use crate::fs::{Entry, EntryKind, FileReader, FileSystem, FileWriter, FsKind, WriteMode};
use crate::path::{remote_file_name, remote_join, remote_parent, validate_name};
use async_trait::async_trait;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, FileType, OpenFlags};
use std::io::SeekFrom;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncSeekExt;

/// Maximum number of symlinks resolved (one `stat` each) per directory listing.
const MAX_LINK_RESOLVES: usize = 256;

/// An SFTP session as a [`FileSystem`]. Cheap to clone.
#[derive(Clone)]
pub struct Remote {
    session: Arc<SftpSession>,
}

impl std::fmt::Debug for Remote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Remote").finish_non_exhaustive()
    }
}

impl Remote {
    #[must_use]
    pub fn new(session: SftpSession) -> Self {
        Self {
            session: Arc::new(session),
        }
    }

    #[must_use]
    pub fn from_arc(session: Arc<SftpSession>) -> Self {
        Self { session }
    }

    /// The underlying session (for operations not covered by [`FileSystem`]).
    #[must_use]
    pub fn session(&self) -> &Arc<SftpSession> {
        &self.session
    }
}

fn kind_of(t: FileType) -> EntryKind {
    match t {
        FileType::Dir => EntryKind::Dir,
        FileType::File => EntryKind::File,
        FileType::Symlink => EntryKind::Symlink,
        FileType::Other => EntryKind::Other,
    }
}

fn time(t: Option<u32>) -> Option<SystemTime> {
    t.map(|s| UNIX_EPOCH + Duration::from_secs(u64::from(s)))
}

fn unix_secs(t: SystemTime) -> u32 {
    t.duration_since(UNIX_EPOCH)
        .map_or(0, |d| u32::try_from(d.as_secs()).unwrap_or(u32::MAX))
}

fn entry_from(name: String, path: String, a: &FileAttributes) -> Entry {
    Entry {
        hidden: name.starts_with('.'),
        name,
        path,
        kind: kind_of(a.file_type()),
        size: a.size.unwrap_or(0),
        modified: time(a.mtime),
        accessed: time(a.atime),
        permissions: a.permissions.map(|p| p & 0o7777),
        uid: a.uid,
        gid: a.gid,
        owner: a.user.clone(),
        group: a.group.clone(),
        link_target_kind: None,
    }
}

fn name_for(path: &str) -> String {
    remote_file_name(path).unwrap_or_else(|| path.to_owned())
}

#[async_trait]
impl FileSystem for Remote {
    fn kind(&self) -> FsKind {
        FsKind::Remote
    }

    fn join(&self, dir: &str, name: &str) -> Result<String, FsError> {
        remote_join(dir, name)
    }

    fn parent(&self, path: &str) -> Option<String> {
        remote_parent(path)
    }

    fn file_name(&self, path: &str) -> Option<String> {
        remote_file_name(path)
    }

    async fn list(&self, path: &str) -> Result<Vec<Entry>, FsError> {
        let rd = self
            .session
            .read_dir(path)
            .await
            .map_err(|e| FsError::sftp(&e, path))?;
        let mut out = Vec::new();
        let mut resolved = 0usize;
        for de in rd {
            let name = de.file_name();
            // Never trust server-provided names: drop anything that is not a plain component.
            if let Err(e) = validate_name(&name) {
                tracing::warn!(dir = %path, "ignoring unsafe entry name from server: {e}");
                continue;
            }
            let full = remote_join(path, &name)?;
            let mut e = entry_from(name, full, &de.metadata());
            if e.kind == EntryKind::Symlink && resolved < MAX_LINK_RESOLVES {
                resolved += 1;
                e.link_target_kind = self
                    .session
                    .metadata(e.path.as_str())
                    .await
                    .ok()
                    .map(|a| kind_of(a.file_type()));
            }
            out.push(e);
        }
        Ok(out)
    }

    async fn stat(&self, path: &str) -> Result<Entry, FsError> {
        let a = self
            .session
            .metadata(path)
            .await
            .map_err(|e| FsError::sftp(&e, path))?;
        Ok(entry_from(name_for(path), path.to_owned(), &a))
    }

    async fn lstat(&self, path: &str) -> Result<Entry, FsError> {
        let a = self
            .session
            .symlink_metadata(path)
            .await
            .map_err(|e| FsError::sftp(&e, path))?;
        let mut e = entry_from(name_for(path), path.to_owned(), &a);
        if e.kind == EntryKind::Symlink {
            e.link_target_kind = self
                .session
                .metadata(path)
                .await
                .ok()
                .map(|a| kind_of(a.file_type()));
        }
        Ok(e)
    }

    async fn canonicalize(&self, path: &str) -> Result<String, FsError> {
        self.session
            .canonicalize(path)
            .await
            .map_err(|e| FsError::sftp(&e, path))
    }

    async fn home(&self) -> Result<String, FsError> {
        self.canonicalize(".").await
    }

    async fn mkdir(&self, path: &str) -> Result<(), FsError> {
        match self.session.create_dir(path).await {
            Ok(()) => Ok(()),
            Err(e) => {
                // SFTP v3 reports "exists" as a generic failure.
                if self.session.symlink_metadata(path).await.is_ok() {
                    Err(FsError::AlreadyExists(path.to_owned()))
                } else {
                    Err(FsError::sftp(&e, path))
                }
            }
        }
    }

    async fn remove_file(&self, path: &str) -> Result<(), FsError> {
        self.session
            .remove_file(path)
            .await
            .map_err(|e| FsError::sftp(&e, path))
    }

    async fn remove_dir(&self, path: &str) -> Result<(), FsError> {
        self.session
            .remove_dir(path)
            .await
            .map_err(|e| FsError::sftp(&e, path))
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), FsError> {
        self.session
            .rename(from, to)
            .await
            .map_err(|e| FsError::sftp(&e, from))
    }

    async fn rename_replace(&self, from: &str, to: &str) -> Result<(), FsError> {
        match self.session.posix_rename(from, to).await {
            Ok(true) => Ok(()),
            Ok(false) => {
                // No posix-rename@openssh.com: emulate (not atomic).
                if let Ok(e) = self.lstat(to).await
                    && e.kind != EntryKind::Dir
                {
                    self.remove_file(to).await?;
                }
                self.rename(from, to).await
            }
            Err(e) => Err(FsError::sftp(&e, from)),
        }
    }

    async fn chmod(&self, path: &str, mode: u32) -> Result<(), FsError> {
        let attrs = FileAttributes {
            permissions: Some(mode & 0o7777),
            ..FileAttributes::empty()
        };
        self.session
            .set_metadata(path, attrs)
            .await
            .map_err(|e| FsError::sftp(&e, path))
    }

    async fn set_times(
        &self,
        path: &str,
        modified: SystemTime,
        accessed: Option<SystemTime>,
    ) -> Result<(), FsError> {
        // SFTP v3 sets both times at once.
        let attrs = FileAttributes {
            mtime: Some(unix_secs(modified)),
            atime: Some(unix_secs(accessed.unwrap_or(modified))),
            ..FileAttributes::empty()
        };
        self.session
            .set_metadata(path, attrs)
            .await
            .map_err(|e| FsError::sftp(&e, path))
    }

    async fn read_link(&self, path: &str) -> Result<String, FsError> {
        self.session
            .read_link(path)
            .await
            .map_err(|e| FsError::sftp(&e, path))
    }

    async fn open_read(&self, path: &str, offset: u64) -> Result<FileReader, FsError> {
        let mut f = self
            .session
            .open(path)
            .await
            .map_err(|e| FsError::sftp(&e, path))?;
        if offset > 0 {
            f.seek(SeekFrom::Start(offset))
                .await
                .map_err(|e| FsError::stream(&e, path))?;
        }
        Ok(Box::new(f))
    }

    async fn open_write(&self, path: &str, mode: WriteMode) -> Result<FileWriter, FsError> {
        let flags = match mode {
            WriteMode::Truncate => OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
            WriteMode::CreateNew => OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
            WriteMode::Resume { .. } => OpenFlags::CREATE | OpenFlags::WRITE,
        };
        let mut f = self
            .session
            .open_with_flags(path, flags)
            .await
            .map_err(|e| {
                if mode == WriteMode::CreateNew {
                    FsError::AlreadyExists(path.to_owned())
                } else {
                    FsError::sftp(&e, path)
                }
            })?;
        if let WriteMode::Resume { offset } = mode {
            let size = f
                .metadata()
                .await
                .map_err(|e| FsError::sftp(&e, path))?
                .size
                .unwrap_or(0);
            if size < offset {
                return Err(FsError::Io(format!(
                    "{path}: cannot resume at {offset}, file has {size} bytes"
                )));
            }
            f.seek(SeekFrom::Start(offset))
                .await
                .map_err(|e| FsError::stream(&e, path))?;
        }
        Ok(Box::new(f))
    }
}
