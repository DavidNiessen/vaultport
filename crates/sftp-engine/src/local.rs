// SPDX-License-Identifier: GPL-3.0-or-later
//! The local file system (tokio::fs).

use crate::error::FsError;
use crate::fs::{
    Entry, EntryKind, FileReader, FileSystem, FileWriter, FsKind, RemoveLimits, RemoveStats,
    WriteMode,
};
use crate::path::safe_local_join;
use async_trait::async_trait;
use std::fs::Metadata;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::io::AsyncSeekExt;

/// The device's own file system.
#[derive(Debug, Clone, Copy, Default)]
pub struct Local;

/// Opens `path` so its timestamps can be changed.
///
/// On Windows `SetFileTime` needs a handle with `FILE_WRITE_ATTRIBUTES`, which a read-only
/// `File::open` does not have; backup semantics let the same call open directories.
fn open_for_times(path: &str) -> std::io::Result<std::fs::File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        std::fs::OpenOptions::new()
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
    }
    #[cfg(not(windows))]
    {
        std::fs::File::open(path)
    }
}

fn kind_of(m: &Metadata) -> EntryKind {
    let ft = m.file_type();
    if ft.is_symlink() {
        EntryKind::Symlink
    } else if ft.is_dir() {
        EntryKind::Dir
    } else if ft.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    }
}

#[cfg(unix)]
fn unix_fields(m: &Metadata) -> (Option<u32>, Option<u32>, Option<u32>) {
    use std::os::unix::fs::MetadataExt;
    (Some(m.mode() & 0o7777), Some(m.uid()), Some(m.gid()))
}

#[cfg(not(unix))]
fn unix_fields(m: &Metadata) -> (Option<u32>, Option<u32>, Option<u32>) {
    let perms = if m.permissions().readonly() {
        0o444
    } else {
        0o644
    };
    let perms = if m.is_dir() { perms | 0o111 } else { perms };
    (Some(perms), None, None)
}

#[cfg(windows)]
fn hidden_attr(m: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    m.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
}

#[cfg(not(windows))]
fn hidden_attr(_m: &Metadata) -> bool {
    false
}

fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.to_string_lossy().into_owned(),
        |n| n.to_string_lossy().into_owned(),
    )
}

pub(crate) fn entry_from(path: &Path, m: &Metadata, link_target: Option<EntryKind>) -> Entry {
    let (permissions, uid, gid) = unix_fields(m);
    let name = name_of(path);
    Entry {
        hidden: name.starts_with('.') || hidden_attr(m),
        name,
        path: path.to_string_lossy().into_owned(),
        kind: kind_of(m),
        size: m.len(),
        modified: m.modified().ok(),
        accessed: m.accessed().ok(),
        permissions,
        uid,
        gid,
        owner: None,
        group: None,
        link_target_kind: link_target,
    }
}

async fn lstat_path(path: &Path) -> Result<Entry, FsError> {
    let p = path.to_string_lossy();
    let m = tokio::fs::symlink_metadata(path)
        .await
        .map_err(|e| FsError::io(&e, &p))?;
    let target = if m.file_type().is_symlink() {
        tokio::fs::metadata(path).await.ok().map(|t| kind_of(&t))
    } else {
        None
    };
    Ok(entry_from(path, &m, target))
}

#[async_trait]
impl FileSystem for Local {
    fn kind(&self) -> FsKind {
        FsKind::Local
    }

    fn join(&self, dir: &str, name: &str) -> Result<String, FsError> {
        Ok(safe_local_join(Path::new(dir), name)?
            .to_string_lossy()
            .into_owned())
    }

    fn parent(&self, path: &str) -> Option<String> {
        Path::new(path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_string_lossy().into_owned())
    }

    fn file_name(&self, path: &str) -> Option<String> {
        Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
    }

    async fn list(&self, path: &str) -> Result<Vec<Entry>, FsError> {
        let mut rd = tokio::fs::read_dir(path)
            .await
            .map_err(|e| FsError::io(&e, path))?;
        let mut out = Vec::new();
        while let Some(de) = rd.next_entry().await.map_err(|e| FsError::io(&e, path))? {
            let p = de.path();
            match lstat_path(&p).await {
                Ok(e) => out.push(e),
                // Vanished between readdir and lstat.
                Err(FsError::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(out)
    }

    async fn stat(&self, path: &str) -> Result<Entry, FsError> {
        let m = tokio::fs::metadata(path)
            .await
            .map_err(|e| FsError::io(&e, path))?;
        Ok(entry_from(Path::new(path), &m, None))
    }

    async fn lstat(&self, path: &str) -> Result<Entry, FsError> {
        lstat_path(Path::new(path)).await
    }

    async fn canonicalize(&self, path: &str) -> Result<String, FsError> {
        let p = tokio::fs::canonicalize(path)
            .await
            .map_err(|e| FsError::io(&e, path))?;
        Ok(p.to_string_lossy().into_owned())
    }

    async fn home(&self) -> Result<String, FsError> {
        let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        std::env::var_os(var)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .map(|p| p.to_string_lossy().into_owned())
            .ok_or_else(|| FsError::NotFound("home directory".into()))
    }

    async fn mkdir(&self, path: &str) -> Result<(), FsError> {
        tokio::fs::create_dir(path)
            .await
            .map_err(|e| FsError::io(&e, path))
    }

    async fn remove_file(&self, path: &str) -> Result<(), FsError> {
        tokio::fs::remove_file(path)
            .await
            .map_err(|e| FsError::io(&e, path))
    }

    async fn remove_dir(&self, path: &str) -> Result<(), FsError> {
        tokio::fs::remove_dir(path)
            .await
            .map_err(|e| FsError::io(&e, path))
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), FsError> {
        tokio::fs::rename(from, to)
            .await
            .map_err(|e| FsError::io(&e, from))
    }

    async fn rename_replace(&self, from: &str, to: &str) -> Result<(), FsError> {
        // std::fs::rename replaces existing files on all supported platforms.
        self.rename(from, to).await
    }

    async fn chmod(&self, path: &str, mode: u32) -> Result<(), FsError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & 0o7777))
                .await
                .map_err(|e| FsError::io(&e, path))
        }
        #[cfg(not(unix))]
        {
            let m = tokio::fs::metadata(path)
                .await
                .map_err(|e| FsError::io(&e, path))?;
            let mut perms = m.permissions();
            perms.set_readonly(mode & 0o200 == 0);
            tokio::fs::set_permissions(path, perms)
                .await
                .map_err(|e| FsError::io(&e, path))
        }
    }

    async fn set_times(
        &self,
        path: &str,
        modified: SystemTime,
        accessed: Option<SystemTime>,
    ) -> Result<(), FsError> {
        let p = path.to_owned();
        tokio::task::spawn_blocking(move || {
            let f = open_for_times(&p)?;
            let mut t = std::fs::FileTimes::new().set_modified(modified);
            if let Some(a) = accessed {
                t = t.set_accessed(a);
            }
            f.set_times(t)
        })
        .await
        .map_err(|e| FsError::Io(e.to_string()))?
        .map_err(|e| FsError::io(&e, path))
    }

    async fn read_link(&self, path: &str) -> Result<String, FsError> {
        let p = tokio::fs::read_link(path)
            .await
            .map_err(|e| FsError::io(&e, path))?;
        Ok(p.to_string_lossy().into_owned())
    }

    async fn open_read(&self, path: &str, offset: u64) -> Result<FileReader, FsError> {
        let mut f = tokio::fs::File::open(path)
            .await
            .map_err(|e| FsError::io(&e, path))?;
        if offset > 0 {
            f.seek(SeekFrom::Start(offset))
                .await
                .map_err(|e| FsError::io(&e, path))?;
        }
        Ok(Box::new(f))
    }

    async fn open_write(&self, path: &str, mode: WriteMode) -> Result<FileWriter, FsError> {
        let mut o = tokio::fs::OpenOptions::new();
        o.write(true);
        match mode {
            WriteMode::Truncate => {
                o.create(true).truncate(true);
            }
            WriteMode::CreateNew => {
                o.create_new(true);
            }
            WriteMode::Resume { .. } => {
                o.create(true);
            }
        }
        let mut f = o.open(path).await.map_err(|e| FsError::io(&e, path))?;
        if let WriteMode::Resume { offset } = mode {
            let len = f.metadata().await.map_err(|e| FsError::io(&e, path))?.len();
            if len < offset {
                return Err(FsError::Io(format!(
                    "{path}: cannot resume at {offset}, file has {len} bytes"
                )));
            }
            f.set_len(offset).await.map_err(|e| FsError::io(&e, path))?;
            f.seek(SeekFrom::Start(offset))
                .await
                .map_err(|e| FsError::io(&e, path))?;
        }
        Ok(Box::new(f))
    }

    /// Uses `std::fs::remove_dir_all`, which never follows symlinks and is hardened against
    /// symlink races on Unix (`openat`/`unlinkat`). `limits` are not enforced locally.
    async fn remove_dir_recursive(
        &self,
        path: &str,
        _limits: RemoveLimits,
    ) -> Result<RemoveStats, FsError> {
        let root = self.lstat(path).await?;
        if root.kind != EntryKind::Dir {
            self.remove_file(path).await?;
            return Ok(RemoveStats { files: 1, dirs: 0 });
        }
        tokio::fs::remove_dir_all(path)
            .await
            .map_err(|e| FsError::io(&e, path))?;
        Ok(RemoveStats::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[tokio::test]
    #[allow(clippy::many_single_char_names)]
    async fn basic_operations() {
        let d = tmp();
        let root = d.path().to_string_lossy().into_owned();
        let fs = Local;
        let sub = fs.join(&root, "sub").expect("join");
        fs.mkdir(&sub).await.expect("mkdir");
        let f = fs.join(&sub, ".hidden.txt").expect("join");
        let mut w = fs.open_write(&f, WriteMode::Truncate).await.expect("open");
        w.write_all(b"hello world").await.expect("write");
        w.shutdown().await.expect("close");
        let list = fs.list(&sub).await.expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, ".hidden.txt");
        assert!(list[0].hidden);
        assert_eq!(list[0].size, 11);
        assert_eq!(list[0].kind, EntryKind::File);

        let mut r = fs.open_read(&f, 6).await.expect("read");
        let mut s = String::new();
        r.read_to_string(&mut s).await.expect("read");
        assert_eq!(s, "world");

        let mut w = fs
            .open_write(&f, WriteMode::Resume { offset: 5 })
            .await
            .expect("resume");
        w.write_all(b"!").await.expect("write");
        w.shutdown().await.expect("close");
        assert_eq!(tokio::fs::read(&f).await.expect("read"), b"hello!");
        assert!(matches!(
            fs.open_write(&f, WriteMode::CreateNew).await,
            Err(FsError::AlreadyExists(_))
        ));

        #[cfg(unix)]
        {
            fs.chmod(&f, 0o640).await.expect("chmod");
            assert_eq!(fs.stat(&f).await.expect("stat").permissions, Some(0o640));
        }
        let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        fs.set_times(&f, t, None).await.expect("times");
        assert_eq!(fs.stat(&f).await.expect("stat").modified, Some(t));

        let g = fs.join(&root, "moved.txt").expect("join");
        fs.rename(&f, &g).await.expect("rename");
        assert!(!fs.exists(&f).await.expect("exists"));
        assert!(fs.exists(&g).await.expect("exists"));
        assert!(fs.join(&root, "../escape").is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn recursive_delete_does_not_follow_symlinks() {
        let d = tmp();
        let root = d.path();
        let outside = root.join("outside");
        std::fs::create_dir(&outside).expect("mkdir");
        std::fs::write(outside.join("precious"), b"keep").expect("write");
        let victim = root.join("victim");
        std::fs::create_dir_all(victim.join("a/b")).expect("mkdir");
        std::fs::write(victim.join("a/b/f"), b"x").expect("write");
        std::os::unix::fs::symlink(&outside, victim.join("link")).expect("symlink");

        // Both the hardened local implementation and the generic one used for SFTP.
        let v = victim.to_string_lossy().into_owned();
        Local
            .remove_dir_recursive(&v, RemoveLimits::default())
            .await
            .expect("remove");
        assert!(!victim.exists());
        assert!(outside.join("precious").exists());

        std::fs::create_dir_all(victim.join("a/b")).expect("mkdir");
        std::fs::write(victim.join("a/b/f"), b"x").expect("write");
        std::os::unix::fs::symlink(&outside, victim.join("link")).expect("symlink");
        let stats = crate::fs::generic_remove_tree(&Local, &v, RemoveLimits::default())
            .await
            .expect("remove");
        assert_eq!(stats, RemoveStats { files: 2, dirs: 3 });
        assert!(!victim.exists());
        assert!(outside.join("precious").exists());

        // A symlink to a directory as the root: only the link goes.
        let link = root.join("rootlink");
        std::os::unix::fs::symlink(&outside, &link).expect("symlink");
        let stats = crate::fs::generic_remove_tree(
            &Local,
            &link.to_string_lossy(),
            RemoveLimits::default(),
        )
        .await
        .expect("remove");
        assert_eq!(stats.files, 1);
        assert!(outside.join("precious").exists());
    }

    #[tokio::test]
    async fn remove_limits_are_enforced() {
        let d = tmp();
        let v = d.path().join("tree");
        for i in 0..5 {
            std::fs::create_dir_all(v.join(format!("d{i}"))).expect("mkdir");
        }
        let err = crate::fs::generic_remove_tree(
            &Local,
            &v.to_string_lossy(),
            RemoveLimits {
                max_entries: 3,
                max_depth: 10,
            },
        )
        .await
        .expect_err("limit");
        assert!(matches!(err, FsError::LimitExceeded(_)));
    }
}
