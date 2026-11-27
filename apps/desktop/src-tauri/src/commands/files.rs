// SPDX-License-Identifier: GPL-3.0-or-later
//! File manager commands for the dual-pane SFTP view (local and remote file systems) and the
//! transfer queue.

use crate::error::{CmdError, CmdResult, ErrorCode};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FsSource {
    Local,
    #[serde(rename_all = "camelCase")]
    Remote {
        host_id: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    File,
    Dir,
    Symlink,
    Other,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: FileKind,
    pub size: Option<f64>,
    pub modified: Option<f64>,
    pub permissions: String,
    pub mode: u32,
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FsListing {
    pub path: String,
    pub entries: Vec<FileEntry>,
}

#[derive(Debug, Clone, Copy, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TransferState {
    Queued,
    Running,
    Paused,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Transfer {
    pub id: String,
    pub name: String,
    pub state: TransferState,
    pub done: f64,
    pub total: f64,
    pub rate: f64,
    pub error: Option<String>,
}

/// `rwxr-xr-x` style rendering of a Unix mode, prefixed with the file type.
pub fn mode_string(mode: u32, kind: FileKind) -> String {
    let t = match kind {
        FileKind::Dir => 'd',
        FileKind::Symlink => 'l',
        _ => '-',
    };
    let mut s = String::with_capacity(10);
    s.push(t);
    for shift in [6u32, 3, 0] {
        let bits = (mode >> shift) & 7;
        s.push(if bits & 4 != 0 { 'r' } else { '-' });
        s.push(if bits & 2 != 0 { 'w' } else { '-' });
        s.push(if bits & 1 != 0 { 'x' } else { '-' });
    }
    s
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

fn remote_unavailable() -> CmdError {
    CmdError::new(
        ErrorCode::Unsupported,
        "remote file systems require the SFTP engine",
    )
}

#[cfg(unix)]
fn unix_mode(m: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    m.permissions().mode()
}

#[cfg(not(unix))]
fn unix_mode(m: &std::fs::Metadata) -> u32 {
    if m.permissions().readonly() {
        0o444
    } else {
        0o644
    }
}

fn local_list(path: Option<String>) -> CmdResult<FsListing> {
    let dir = path.map_or_else(home, PathBuf::from);
    let dir = std::fs::canonicalize(&dir)?;
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&dir)?.take(100_000) {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(meta) = entry.path().symlink_metadata() else {
            continue;
        };
        let ft = meta.file_type();
        let kind = if ft.is_symlink() {
            FileKind::Symlink
        } else if ft.is_dir() {
            FileKind::Dir
        } else if ft.is_file() {
            FileKind::File
        } else {
            FileKind::Other
        };
        let mode = unix_mode(&meta);
        entries.push(FileEntry {
            hidden: name.starts_with('.'),
            path: entry.path().to_string_lossy().into_owned(),
            name,
            kind,
            size: matches!(kind, FileKind::File).then(|| meta.len() as f64),
            modified: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as f64),
            permissions: mode_string(mode, kind),
            mode,
        });
    }
    Ok(FsListing {
        path: dir.to_string_lossy().into_owned(),
        entries,
    })
}

/// Lists a directory (`None` = home directory).
#[tauri::command]
#[specta::specta]
pub async fn fs_list(source: FsSource, path: Option<String>) -> CmdResult<FsListing> {
    match source {
        FsSource::Local => tauri::async_runtime::spawn_blocking(move || local_list(path))
            .await
            .map_err(|e| CmdError::internal(e.to_string()))?,
        FsSource::Remote { .. } => Err(remote_unavailable()),
    }
}

/// Creates a directory.
#[tauri::command]
#[specta::specta]
pub fn fs_mkdir(source: FsSource, path: String) -> CmdResult<()> {
    match source {
        FsSource::Local => Ok(std::fs::create_dir(path)?),
        FsSource::Remote { .. } => Err(remote_unavailable()),
    }
}

/// Renames / moves an entry within one file system.
#[tauri::command]
#[specta::specta]
pub fn fs_rename(source: FsSource, from: String, to: String) -> CmdResult<()> {
    match source {
        FsSource::Local => Ok(std::fs::rename(from, to)?),
        FsSource::Remote { .. } => Err(remote_unavailable()),
    }
}

/// Changes Unix permissions.
#[tauri::command]
#[specta::specta]
pub fn fs_chmod(source: FsSource, path: String, mode: u32) -> CmdResult<()> {
    if mode > 0o7777 {
        return Err(CmdError::invalid("invalid mode"));
    }
    match source {
        #[cfg(unix)]
        FsSource::Local => {
            use std::os::unix::fs::PermissionsExt;
            Ok(std::fs::set_permissions(
                path,
                std::fs::Permissions::from_mode(mode),
            )?)
        }
        #[cfg(not(unix))]
        FsSource::Local => {
            let _ = path;
            Err(CmdError::new(
                ErrorCode::Unsupported,
                "permissions are not supported here",
            ))
        }
        FsSource::Remote { .. } => Err(remote_unavailable()),
    }
}

/// Deletes a file or (recursively, without following symlinks) a directory.
#[tauri::command]
#[specta::specta]
pub fn fs_remove(source: FsSource, path: String, recursive: bool) -> CmdResult<()> {
    match source {
        FsSource::Local => {
            let p = Path::new(&path);
            let meta = p.symlink_metadata()?;
            if meta.is_dir() {
                if recursive {
                    std::fs::remove_dir_all(p)?;
                } else {
                    std::fs::remove_dir(p)?;
                }
            } else {
                std::fs::remove_file(p)?;
            }
            Ok(())
        }
        FsSource::Remote { .. } => Err(remote_unavailable()),
    }
}

/// Opens a file in the default local application (remote files are downloaded to a private
/// temp folder and watched for changes).
#[tauri::command]
#[specta::specta]
pub fn fs_edit(app: tauri::AppHandle, source: FsSource, path: String) -> CmdResult<()> {
    match source {
        FsSource::Local => {
            use tauri_plugin_opener::OpenerExt;
            app.opener()
                .open_path(path, None::<&str>)
                .map_err(|e| CmdError::internal(e.to_string()))
        }
        FsSource::Remote { .. } => Err(remote_unavailable()),
    }
}

/// Downloads a remote file to the Downloads folder.
#[tauri::command]
#[specta::specta]
pub fn fs_download(source: FsSource, path: String) -> CmdResult<String> {
    let _ = (source, path);
    Err(remote_unavailable())
}

/// Queues a copy of `paths` from `from` into directory `to_dir` on `to`.
#[tauri::command]
#[specta::specta]
pub fn transfer_start(
    from: FsSource,
    paths: Vec<String>,
    to: FsSource,
    to_dir: String,
    conflict: String,
) -> CmdResult<Vec<String>> {
    let _ = (from, paths, to, to_dir, conflict);
    Err(remote_unavailable())
}

/// Lists queued, running and finished transfers.
#[tauri::command]
#[specta::specta]
pub fn transfer_list() -> Vec<Transfer> {
    Vec::new()
}

/// Cancels a transfer.
#[tauri::command]
#[specta::specta]
pub fn transfer_cancel(id: String) -> CmdResult<()> {
    let _ = id;
    Ok(())
}

/// Removes finished transfers from the list.
#[tauri::command]
#[specta::specta]
pub fn transfer_clear_finished() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_render_like_ls() {
        assert_eq!(mode_string(0o755, FileKind::Dir), "drwxr-xr-x");
        assert_eq!(mode_string(0o640, FileKind::File), "-rw-r-----");
        assert_eq!(mode_string(0o777, FileKind::Symlink), "lrwxrwxrwx");
    }
}
