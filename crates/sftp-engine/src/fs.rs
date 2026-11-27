// SPDX-License-Identifier: GPL-3.0-or-later
//! The [`FileSystem`] abstraction shared by local and remote (SFTP) file systems.

use crate::error::FsError;
use async_trait::async_trait;
use std::fmt;
use std::time::SystemTime;
use tokio::io::{AsyncRead, AsyncWrite};

/// Type of a directory entry (symlinks are *not* followed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    /// Device, FIFO, socket…
    Other,
}

/// Metadata of a file or directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// File name (last path component).
    pub name: String,
    /// Full path in the owning file system.
    pub path: String,
    pub kind: EntryKind,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    /// Permission bits (`0o7777` mask), when known.
    pub permissions: Option<u32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    /// User / group names when the file system reports them.
    pub owner: Option<String>,
    pub group: Option<String>,
    /// Dot-file (or hidden attribute on Windows).
    pub hidden: bool,
    /// For symlinks: the kind of the target if it could be resolved (dangling → `None`).
    pub link_target_kind: Option<EntryKind>,
}

impl Entry {
    /// `ls -l` style mode string, e.g. `drwxr-xr-x` (`?` for unknown permissions).
    #[must_use]
    pub fn mode_string(&self) -> String {
        mode_string(self.kind, self.permissions)
    }

    /// Whether this is a directory, or a symlink to one.
    #[must_use]
    pub fn is_dir_like(&self) -> bool {
        self.kind == EntryKind::Dir || self.link_target_kind == Some(EntryKind::Dir)
    }
}

/// `ls -l` style mode string for `kind` and permission bits.
#[must_use]
pub fn mode_string(kind: EntryKind, permissions: Option<u32>) -> String {
    let t = match kind {
        EntryKind::File => '-',
        EntryKind::Dir => 'd',
        EntryKind::Symlink => 'l',
        EntryKind::Other => '?',
    };
    let Some(m) = permissions else {
        return format!("{t}?????????");
    };
    let bit = |mask: u32, c: char| if m & mask == 0 { '-' } else { c };
    let special =
        |exec: u32, special: u32, set: char, unset: char| match (m & exec != 0, m & special != 0) {
            (true, true) => set,
            (false, true) => unset,
            (true, false) => 'x',
            (false, false) => '-',
        };
    [
        t,
        bit(0o400, 'r'),
        bit(0o200, 'w'),
        special(0o100, 0o4000, 's', 'S'),
        bit(0o040, 'r'),
        bit(0o020, 'w'),
        special(0o010, 0o2000, 's', 'S'),
        bit(0o004, 'r'),
        bit(0o002, 'w'),
        special(0o001, 0o1000, 't', 'T'),
    ]
    .iter()
    .collect()
}

/// How [`FileSystem::open_write`] opens a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    /// Create or truncate.
    Truncate,
    /// Fail if the file exists.
    CreateNew,
    /// Keep the content and continue writing at `offset` (resume).
    Resume { offset: u64 },
}

/// Which kind of file system an implementation is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsKind {
    Local,
    Remote,
}

/// Limits for [`FileSystem::remove_dir_recursive`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoveLimits {
    pub max_entries: usize,
    pub max_depth: usize,
}

impl Default for RemoveLimits {
    fn default() -> Self {
        Self {
            max_entries: 1_000_000,
            max_depth: 256,
        }
    }
}

/// What [`FileSystem::remove_dir_recursive`] removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RemoveStats {
    /// Files, symlinks and other non-directories.
    pub files: u64,
    pub dirs: u64,
}

/// Streaming reader returned by [`FileSystem::open_read`].
pub type FileReader = Box<dyn AsyncRead + Send + Unpin>;
/// Streaming writer returned by [`FileSystem::open_write`]. Call
/// [`tokio::io::AsyncWriteExt::shutdown`] to flush and close it; dropping it may lose the
/// last writes (and their errors).
pub type FileWriter = Box<dyn AsyncWrite + Send + Unpin>;

/// Async file system operations, implemented by [`crate::Local`] and [`crate::Remote`].
///
/// Paths are strings: native paths for [`crate::Local`], POSIX paths for [`crate::Remote`].
/// Names coming from listings are validated (see [`crate::path`]) before being joined.
#[async_trait]
pub trait FileSystem: Send + Sync + fmt::Debug {
    fn kind(&self) -> FsKind;

    /// Joins an (untrusted) entry name to a directory path; rejects traversal.
    fn join(&self, dir: &str, name: &str) -> Result<String, FsError>;

    /// Parent directory of `path`.
    fn parent(&self, path: &str) -> Option<String>;

    /// Last component of `path`.
    fn file_name(&self, path: &str) -> Option<String>;

    /// Directory listing (without `.` and `..`; symlinks are reported as such).
    async fn list(&self, path: &str) -> Result<Vec<Entry>, FsError>;

    /// Metadata, following symlinks.
    async fn stat(&self, path: &str) -> Result<Entry, FsError>;

    /// Metadata of the path itself (symlinks not followed).
    async fn lstat(&self, path: &str) -> Result<Entry, FsError>;

    /// Absolute, normalized path.
    async fn canonicalize(&self, path: &str) -> Result<String, FsError>;

    /// The user's home directory (remote: the SFTP start directory).
    async fn home(&self) -> Result<String, FsError>;

    async fn mkdir(&self, path: &str) -> Result<(), FsError>;

    async fn remove_file(&self, path: &str) -> Result<(), FsError>;

    /// Removes an empty directory.
    async fn remove_dir(&self, path: &str) -> Result<(), FsError>;

    /// Renames; may fail when `to` exists (SFTP v3 semantics).
    async fn rename(&self, from: &str, to: &str) -> Result<(), FsError>;

    /// Renames, replacing `to` if it exists (atomically where supported).
    async fn rename_replace(&self, from: &str, to: &str) -> Result<(), FsError> {
        match self.rename(from, to).await {
            Ok(()) => Ok(()),
            Err(first) => match self.lstat(to).await {
                Ok(e) if e.kind != EntryKind::Dir => {
                    self.remove_file(to).await?;
                    self.rename(from, to).await
                }
                _ => Err(first),
            },
        }
    }

    /// Sets permission bits.
    async fn chmod(&self, path: &str, mode: u32) -> Result<(), FsError>;

    /// Sets modification (and access) time.
    async fn set_times(
        &self,
        path: &str,
        modified: SystemTime,
        accessed: Option<SystemTime>,
    ) -> Result<(), FsError>;

    /// Target of a symlink.
    async fn read_link(&self, path: &str) -> Result<String, FsError>;

    /// Opens a file for streaming reads starting at `offset`.
    async fn open_read(&self, path: &str, offset: u64) -> Result<FileReader, FsError>;

    /// Opens a file for streaming writes.
    async fn open_write(&self, path: &str, mode: WriteMode) -> Result<FileWriter, FsError>;

    /// Whether `path` exists (symlinks not followed).
    async fn exists(&self, path: &str) -> Result<bool, FsError> {
        match self.lstat(path).await {
            Ok(_) => Ok(true),
            Err(FsError::NotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Creates `path` and missing parents.
    async fn create_dir_all(&self, path: &str) -> Result<(), FsError> {
        let mut missing = Vec::new();
        let mut cur = Some(path.to_owned());
        while let Some(p) = cur {
            match self.lstat(&p).await {
                Ok(e) if e.is_dir_like() => break,
                Ok(_) => return Err(FsError::NotADirectory(p)),
                Err(FsError::NotFound(_)) => {
                    cur = self.parent(&p);
                    missing.push(p);
                }
                Err(e) => return Err(e),
            }
            if missing.len() > 256 {
                return Err(FsError::LimitExceeded("path too deep".into()));
            }
        }
        for p in missing.iter().rev() {
            match self.mkdir(p).await {
                Ok(()) | Err(FsError::AlreadyExists(_)) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Deletes a directory tree **without following symlinks** (a symlink to a directory is
    /// removed, never its target's content). If `path` itself is a symlink or file, only it is
    /// removed. Every directory is re-checked with `lstat` right before it is listed.
    async fn remove_dir_recursive(
        &self,
        path: &str,
        limits: RemoveLimits,
    ) -> Result<RemoveStats, FsError> {
        generic_remove_tree(self, path, limits).await
    }
}

/// Portable, symlink-safe recursive delete built on the primitive operations.
pub(crate) async fn generic_remove_tree<F: FileSystem + ?Sized>(
    fs: &F,
    path: &str,
    limits: RemoveLimits,
) -> Result<RemoveStats, FsError> {
    let mut stats = RemoveStats::default();
    let root = fs.lstat(path).await?;
    if root.kind != EntryKind::Dir {
        fs.remove_file(path).await?;
        stats.files += 1;
        return Ok(stats);
    }
    // (path, depth, children already queued)
    let mut stack: Vec<(String, usize, bool)> = vec![(path.to_owned(), 0, false)];
    let mut seen = 0usize;
    while let Some((dir, depth, expanded)) = stack.pop() {
        if expanded {
            fs.remove_dir(&dir).await?;
            stats.dirs += 1;
            continue;
        }
        if depth > limits.max_depth {
            return Err(FsError::LimitExceeded(format!(
                "directory depth above {}",
                limits.max_depth
            )));
        }
        // Re-check: it must still be a real directory, not a symlink swapped in.
        if fs.lstat(&dir).await?.kind != EntryKind::Dir {
            return Err(FsError::NotADirectory(dir));
        }
        stack.push((dir.clone(), depth, true));
        for e in fs.list(&dir).await? {
            seen += 1;
            if seen > limits.max_entries {
                return Err(FsError::LimitExceeded(format!(
                    "more than {} entries",
                    limits.max_entries
                )));
            }
            let child = fs.join(&dir, &e.name)?;
            if e.kind == EntryKind::Dir {
                stack.push((child, depth + 1, false));
            } else {
                fs.remove_file(&child).await?;
                stats.files += 1;
            }
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_strings() {
        assert_eq!(mode_string(EntryKind::Dir, Some(0o755)), "drwxr-xr-x");
        assert_eq!(mode_string(EntryKind::File, Some(0o644)), "-rw-r--r--");
        assert_eq!(mode_string(EntryKind::File, Some(0o4755)), "-rwsr-xr-x");
        assert_eq!(mode_string(EntryKind::Dir, Some(0o1777)), "drwxrwxrwt");
        assert_eq!(mode_string(EntryKind::File, Some(0o2640)), "-rw-r-S---");
        assert_eq!(mode_string(EntryKind::Symlink, None), "l?????????");
    }
}
