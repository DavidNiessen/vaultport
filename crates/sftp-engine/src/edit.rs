// SPDX-License-Identifier: GPL-3.0-or-later
//! Edit-and-upload: download a remote file into a private temporary directory, let the user
//! edit it with a local application, detect modifications and upload them back.
//!
//! * The temporary directory is created with mode `0700` (files `0600`) and removed when the
//!   [`EditSession`] is dropped.
//! * [`EditSession::upload`] refuses to overwrite a remote file that changed since it was
//!   downloaded ([`FsError::RemoteChanged`]); [`EditSession::upload_force`] overrides.
//! * Uploads go through `<name>.sshc-part` + rename, keep the remote permissions and never
//!   leave a half-written file under the original name.
//! * With the `watch` feature (desktop), [`EditSession::watch`] reports modifications
//!   (debounced, content-based, robust against editors that save via rename).

use crate::error::FsError;
use crate::fs::{Entry, FileSystem, WriteMode};
use crate::path::safe_local_join;
use crate::transfer::PART_SUFFIX;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Files larger than this are not opened for editing by default.
pub const DEFAULT_MAX_EDIT_SIZE: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RemoteStamp {
    size: u64,
    modified: Option<SystemTime>,
}

impl From<&Entry> for RemoteStamp {
    fn from(e: &Entry) -> Self {
        Self {
            size: e.size,
            modified: e.modified,
        }
    }
}

/// A remote file checked out for local editing.
#[derive(Debug)]
pub struct EditSession {
    remote: Arc<dyn FileSystem>,
    remote_path: String,
    local_path: PathBuf,
    permissions: Option<u32>,
    /// Hash of the content last downloaded or uploaded.
    baseline: u64,
    remote_stamp: RemoteStamp,
    // Keep last: dropping it removes the directory.
    dir: tempfile::TempDir,
}

fn hash(data: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    data.hash(&mut h);
    h.finish()
}

async fn read_remote(fs: &dyn FileSystem, path: &str, max: u64) -> Result<Vec<u8>, FsError> {
    let mut r = fs.open_read(path, 0).await?;
    let mut out = Vec::new();
    let n = (&mut r)
        .take(max + 1)
        .read_to_end(&mut out)
        .await
        .map_err(|e| FsError::stream(&e, path))?;
    if n as u64 > max {
        return Err(FsError::LimitExceeded(format!(
            "{path} is larger than {max} bytes"
        )));
    }
    Ok(out)
}

impl EditSession {
    /// Downloads `remote_path` into a new private directory under the system temp dir.
    pub async fn open(remote: Arc<dyn FileSystem>, remote_path: &str) -> Result<Self, FsError> {
        Self::open_in(
            remote,
            remote_path,
            &std::env::temp_dir(),
            DEFAULT_MAX_EDIT_SIZE,
        )
        .await
    }

    /// Like [`Self::open`] with an explicit parent directory and size limit.
    pub async fn open_in(
        remote: Arc<dyn FileSystem>,
        remote_path: &str,
        parent: &Path,
        max_size: u64,
    ) -> Result<Self, FsError> {
        let entry = remote.stat(remote_path).await?;
        if entry.kind != crate::fs::EntryKind::File {
            return Err(FsError::Unsupported(format!(
                "{remote_path} is not a regular file"
            )));
        }
        if entry.size > max_size {
            return Err(FsError::LimitExceeded(format!(
                "{remote_path} is larger than {max_size} bytes"
            )));
        }
        let name = remote
            .file_name(remote_path)
            .ok_or_else(|| FsError::UnsafePath(remote_path.to_owned()))?;
        let dir = tempfile::Builder::new()
            .prefix("ssh-client-edit-")
            .tempdir_in(parent)
            .map_err(|e| FsError::io(&e, &parent.to_string_lossy()))?;
        restrict(dir.path(), 0o700).await?;
        let local_path = safe_local_join(dir.path(), &name)?;
        let content = read_remote(&*remote, remote_path, max_size).await?;
        tokio::fs::write(&local_path, &content)
            .await
            .map_err(|e| FsError::io(&e, &local_path.to_string_lossy()))?;
        restrict(&local_path, 0o600).await?;
        Ok(Self {
            remote,
            remote_path: remote_path.to_owned(),
            local_path,
            permissions: entry.permissions,
            baseline: hash(&content),
            remote_stamp: RemoteStamp::from(&entry),
            dir,
        })
    }

    /// The local copy to open in an editor.
    pub fn local_path(&self) -> &Path {
        &self.local_path
    }

    pub fn remote_path(&self) -> &str {
        &self.remote_path
    }

    /// The private directory holding the copy.
    pub fn directory(&self) -> &Path {
        self.dir.path()
    }

    /// Whether the local copy differs from what was last downloaded/uploaded.
    pub async fn is_modified(&self) -> Result<bool, FsError> {
        let content = tokio::fs::read(&self.local_path)
            .await
            .map_err(|e| FsError::io(&e, &self.local_path.to_string_lossy()))?;
        Ok(hash(&content) != self.baseline)
    }

    /// Whether the remote file changed since it was downloaded (size or mtime).
    pub async fn remote_changed(&self) -> Result<bool, FsError> {
        let e = self.remote.stat(&self.remote_path).await?;
        Ok(RemoteStamp::from(&e) != self.remote_stamp)
    }

    /// Uploads the local copy unless the remote file changed in the meantime.
    pub async fn upload(&mut self) -> Result<(), FsError> {
        if self.remote_changed().await? {
            return Err(FsError::RemoteChanged(self.remote_path.clone()));
        }
        self.upload_force().await
    }

    /// Uploads the local copy, overwriting remote changes.
    pub async fn upload_force(&mut self) -> Result<(), FsError> {
        let content = tokio::fs::read(&self.local_path)
            .await
            .map_err(|e| FsError::io(&e, &self.local_path.to_string_lossy()))?;
        let part = format!("{}{PART_SUFFIX}", self.remote_path);
        let mut w = self.remote.open_write(&part, WriteMode::Truncate).await?;
        let written = async {
            w.write_all(&content).await?;
            w.shutdown().await
        }
        .await;
        if let Err(e) = written {
            let _ = self.remote.remove_file(&part).await;
            return Err(FsError::stream(&e, &part));
        }
        if let Some(p) = self.permissions {
            let _ = self.remote.chmod(&part, p).await;
        }
        self.remote.rename_replace(&part, &self.remote_path).await?;
        self.baseline = hash(&content);
        let e = self.remote.stat(&self.remote_path).await?;
        self.remote_stamp = RemoteStamp::from(&e);
        Ok(())
    }

    /// Deletes the local copy now (also happens on drop).
    pub fn close(self) -> Result<(), FsError> {
        let p = self.dir.path().to_string_lossy().into_owned();
        self.dir.close().map_err(|e| FsError::io(&e, &p))
    }

    /// Watches the local copy; see [`EditWatcher`].
    #[cfg(feature = "watch")]
    pub fn watch(&self) -> Result<EditWatcher, FsError> {
        EditWatcher::new(&self.local_path, self.baseline)
    }
}

async fn restrict(path: &Path, mode: u32) -> Result<(), FsError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .await
            .map_err(|e| FsError::io(&e, &path.to_string_lossy()))
    }
    #[cfg(not(unix))]
    {
        // Windows: the per-user temp directory is already private to the user.
        let _ = (path, mode);
        Ok(())
    }
}

/// Reports modifications of an [`EditSession`]'s local copy (feature `watch`).
#[cfg(feature = "watch")]
pub struct EditWatcher {
    rx: tokio::sync::mpsc::UnboundedReceiver<()>,
    _watcher: notify::RecommendedWatcher,
}

#[cfg(feature = "watch")]
impl std::fmt::Debug for EditWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditWatcher").finish_non_exhaustive()
    }
}

#[cfg(feature = "watch")]
impl EditWatcher {
    fn new(file: &Path, baseline: u64) -> Result<Self, FsError> {
        use notify::{RecursiveMode, Watcher};
        use std::sync::Mutex;
        use std::time::Duration;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let last = Arc::new(Mutex::new(baseline));
        let target = file.to_path_buf();
        let handler = move |res: notify::Result<notify::Event>| {
            let Ok(ev) = res else { return };
            if !ev.paths.iter().any(|p| p.file_name() == target.file_name()) {
                return;
            }
            // Debounce: editors write in several steps.
            std::thread::sleep(Duration::from_millis(150));
            let Ok(content) = std::fs::read(&target) else {
                return;
            };
            let h = hash(&content);
            if let Ok(mut l) = last.lock()
                && *l != h
            {
                *l = h;
                let _ = tx.send(());
            }
        };
        let mut watcher =
            notify::recommended_watcher(handler).map_err(|e| FsError::Io(e.to_string()))?;
        let dir = file
            .parent()
            .ok_or_else(|| FsError::Io("no parent directory".into()))?;
        // Watch the directory: many editors replace the file instead of writing in place.
        watcher
            .watch(dir, RecursiveMode::NonRecursive)
            .map_err(|e| FsError::Io(e.to_string()))?;
        Ok(Self {
            rx,
            _watcher: watcher,
        })
    }

    /// Resolves when the content changed (compared with the last notification).
    pub async fn changed(&mut self) -> Option<()> {
        self.rx.recv().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::Local;

    #[tokio::test]
    async fn edit_cycle_with_conflict_detection() {
        let d = tempfile::tempdir().expect("tmp");
        let remote_file = d.path().join("config.ini");
        std::fs::write(&remote_file, b"a=1\n").expect("w");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&remote_file, std::fs::Permissions::from_mode(0o640))
                .expect("chmod");
        }
        let remote: Arc<dyn FileSystem> = Arc::new(Local);
        let rp = remote_file.to_string_lossy().into_owned();
        let mut s = EditSession::open_in(remote, &rp, d.path(), 1024)
            .await
            .expect("open");
        assert_eq!(std::fs::read(s.local_path()).expect("r"), b"a=1\n");
        assert!(!s.is_modified().await.expect("m"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).expect("meta").permissions().mode() & 0o777;
            assert_eq!(mode(s.directory()), 0o700);
            assert_eq!(mode(s.local_path()), 0o600);
        }

        std::fs::write(s.local_path(), b"a=2\n").expect("w");
        assert!(s.is_modified().await.expect("m"));
        s.upload().await.expect("upload");
        assert_eq!(std::fs::read(&remote_file).expect("r"), b"a=2\n");
        assert!(!s.is_modified().await.expect("m"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let m = std::fs::metadata(&remote_file)
                .expect("meta")
                .permissions()
                .mode();
            assert_eq!(m & 0o777, 0o640, "remote permissions preserved");
        }

        // Someone else changes the remote file: plain upload refuses.
        std::fs::write(&remote_file, b"a=3, from elsewhere\n").expect("w");
        std::fs::write(s.local_path(), b"a=4\n").expect("w");
        assert!(matches!(s.upload().await, Err(FsError::RemoteChanged(_))));
        s.upload_force().await.expect("force");
        assert_eq!(std::fs::read(&remote_file).expect("r"), b"a=4\n");

        let dir = s.directory().to_path_buf();
        s.close().expect("close");
        assert!(!dir.exists());
    }

    #[tokio::test]
    async fn size_limit_and_directories_are_refused() {
        let d = tempfile::tempdir().expect("tmp");
        let f = d.path().join("big");
        std::fs::write(&f, vec![0u8; 2048]).expect("w");
        let fs: Arc<dyn FileSystem> = Arc::new(Local);
        assert!(matches!(
            EditSession::open_in(fs.clone(), &f.to_string_lossy(), d.path(), 1024).await,
            Err(FsError::LimitExceeded(_))
        ));
        assert!(matches!(
            EditSession::open_in(fs, &d.path().to_string_lossy(), d.path(), 1024).await,
            Err(FsError::Unsupported(_))
        ));
    }

    #[cfg(feature = "watch")]
    #[tokio::test]
    async fn watcher_reports_changes() {
        let d = tempfile::tempdir().expect("tmp");
        let f = d.path().join("w.txt");
        std::fs::write(&f, b"x").expect("w");
        let fs: Arc<dyn FileSystem> = Arc::new(Local);
        let s = EditSession::open_in(fs, &f.to_string_lossy(), d.path(), 1024)
            .await
            .expect("open");
        let mut w = s.watch().expect("watch");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        std::fs::write(s.local_path(), b"changed").expect("w");
        let got = tokio::time::timeout(std::time::Duration::from_secs(10), w.changed()).await;
        assert!(matches!(got, Ok(Some(()))));
    }
}
