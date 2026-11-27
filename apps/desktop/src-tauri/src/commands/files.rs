// SPDX-License-Identifier: GPL-3.0-or-later
//! File manager commands for the dual-pane SFTP view (local and remote file systems), the
//! transfer queue and edit-and-upload sessions.

use crate::error::{CmdError, CmdResult, ErrorCode};
use crate::net::ConnectionPool;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sftp_engine::{
    ConflictPolicy, EditSession, Entry, EntryKind, FileSystem, Local, Remote, TransferEvent,
    TransferOptions, TransferQueue, TransferRequest,
};
use specta::Type;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

pub const EDIT_EVENT: &str = "sftp://edited";

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

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EditedFile {
    pub edit_id: String,
    pub name: String,
}

/// File-system handles, transfer queue and edit sessions.
pub struct FilesState {
    remotes: tokio::sync::Mutex<HashMap<String, Arc<dyn FileSystem>>>,
    queue: TransferQueue,
    transfers: Arc<Mutex<Vec<Transfer>>>,
    edits: Arc<tokio::sync::Mutex<HashMap<String, EditSession>>>,
}

impl FilesState {
    pub fn new() -> Self {
        let (queue, mut events) = TransferQueue::new(3, None);
        let transfers: Arc<Mutex<Vec<Transfer>>> = Arc::default();
        let t = transfers.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(ev) = events.recv().await {
                let mut list = t.lock();
                match ev {
                    TransferEvent::Queued {
                        id, source_path, ..
                    } => list.push(Transfer {
                        id: id.to_string(),
                        name: source_path
                            .rsplit(['/', '\\'])
                            .next()
                            .unwrap_or(&source_path)
                            .to_owned(),
                        state: TransferState::Queued,
                        done: 0.0,
                        total: 0.0,
                        rate: 0.0,
                        error: None,
                    }),
                    TransferEvent::Started {
                        id, bytes_total, ..
                    } => {
                        if let Some(x) = list.iter_mut().find(|x| x.id == id.to_string()) {
                            x.state = TransferState::Running;
                            x.total = bytes_total as f64;
                        }
                    }
                    TransferEvent::Progress(p) => {
                        if let Some(x) = list.iter_mut().find(|x| x.id == p.id.to_string()) {
                            x.done = p.bytes_done as f64;
                            x.total = p.bytes_total as f64;
                            x.rate = p.bytes_per_second;
                        }
                    }
                    TransferEvent::Paused { id } => set(&mut list, id, TransferState::Paused, None),
                    TransferEvent::Resumed { id } => {
                        set(&mut list, id, TransferState::Running, None);
                    }
                    TransferEvent::Completed { id, bytes, .. } => {
                        if let Some(x) = list.iter_mut().find(|x| x.id == id.to_string()) {
                            x.state = TransferState::Done;
                            x.done = bytes as f64;
                            x.total = x.total.max(bytes as f64);
                        }
                    }
                    TransferEvent::Failed { id, error } => {
                        set(&mut list, id, TransferState::Failed, Some(error));
                    }
                    TransferEvent::Cancelled { id } => {
                        set(&mut list, id, TransferState::Cancelled, None);
                    }
                    _ => {}
                }
            }
        });
        Self {
            remotes: tokio::sync::Mutex::default(),
            queue,
            transfers,
            edits: Arc::default(),
        }
    }

    async fn fs(&self, app: &AppHandle, source: &FsSource) -> CmdResult<Arc<dyn FileSystem>> {
        match source {
            FsSource::Local => Ok(Arc::new(Local)),
            FsSource::Remote { host_id } => {
                let mut remotes = self.remotes.lock().await;
                let pool = app.state::<ConnectionPool>();
                let conn = pool.get(app, host_id).await?;
                if let Some(fs) = remotes.get(host_id)
                    && !conn.is_closed()
                {
                    return Ok(fs.clone());
                }
                let sftp = conn.sftp().await.map_err(crate::net::map_ssh)?;
                let fs: Arc<dyn FileSystem> = Arc::new(Remote::new(sftp));
                remotes.insert(host_id.clone(), fs.clone());
                Ok(fs)
            }
        }
    }

    /// Drops cached SFTP sessions (on vault lock).
    pub async fn reset(&self) {
        self.remotes.lock().await.clear();
        self.edits.lock().await.clear();
    }
}

impl Default for FilesState {
    fn default() -> Self {
        Self::new()
    }
}

fn set(list: &mut [Transfer], id: u64, state: TransferState, error: Option<String>) {
    if let Some(x) = list.iter_mut().find(|x| x.id == id.to_string()) {
        x.state = state;
        x.error = error;
    }
}

fn map_fs(e: sftp_engine::FsError) -> CmdError {
    CmdError::new(ErrorCode::Internal, e.to_string())
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

fn entry(e: Entry) -> FileEntry {
    let kind = match e.kind {
        EntryKind::File => FileKind::File,
        EntryKind::Dir => FileKind::Dir,
        EntryKind::Symlink => FileKind::Symlink,
        EntryKind::Other => FileKind::Other,
    };
    let mode = e.permissions.unwrap_or(0) & 0o7777;
    FileEntry {
        size: matches!(kind, FileKind::File).then_some(e.size as f64),
        modified: e
            .modified
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as f64),
        permissions: mode_string(mode, kind),
        mode,
        hidden: e.hidden,
        name: e.name,
        path: e.path,
        kind,
    }
}

/// Lists a directory (`None` = home directory).
#[tauri::command]
#[specta::specta]
pub async fn fs_list(
    app: AppHandle,
    files: State<'_, FilesState>,
    source: FsSource,
    path: Option<String>,
) -> CmdResult<FsListing> {
    let fs = files.fs(&app, &source).await?;
    let path = match path {
        Some(p) if !p.is_empty() => fs.canonicalize(&p).await.map_err(map_fs)?,
        _ => fs.home().await.map_err(map_fs)?,
    };
    let mut entries: Vec<FileEntry> = fs
        .list(&path)
        .await
        .map_err(map_fs)?
        .into_iter()
        .map(entry)
        .collect();
    entries.truncate(100_000);
    Ok(FsListing { path, entries })
}

/// Creates a directory.
#[tauri::command]
#[specta::specta]
pub async fn fs_mkdir(
    app: AppHandle,
    files: State<'_, FilesState>,
    source: FsSource,
    path: String,
) -> CmdResult<()> {
    files
        .fs(&app, &source)
        .await?
        .mkdir(&path)
        .await
        .map_err(map_fs)
}

/// Renames / moves an entry within one file system.
#[tauri::command]
#[specta::specta]
pub async fn fs_rename(
    app: AppHandle,
    files: State<'_, FilesState>,
    source: FsSource,
    from: String,
    to: String,
) -> CmdResult<()> {
    files
        .fs(&app, &source)
        .await?
        .rename(&from, &to)
        .await
        .map_err(map_fs)
}

/// Changes Unix permissions.
#[tauri::command]
#[specta::specta]
pub async fn fs_chmod(
    app: AppHandle,
    files: State<'_, FilesState>,
    source: FsSource,
    path: String,
    mode: u32,
) -> CmdResult<()> {
    if mode > 0o7777 {
        return Err(CmdError::invalid("invalid mode"));
    }
    files
        .fs(&app, &source)
        .await?
        .chmod(&path, mode)
        .await
        .map_err(map_fs)
}

/// Deletes a file or (recursively, never following symlinks) a directory.
#[tauri::command]
#[specta::specta]
pub async fn fs_remove(
    app: AppHandle,
    files: State<'_, FilesState>,
    source: FsSource,
    path: String,
    recursive: bool,
) -> CmdResult<()> {
    let fs = files.fs(&app, &source).await?;
    let meta = fs.lstat(&path).await.map_err(map_fs)?;
    if meta.kind == EntryKind::Dir {
        if recursive {
            fs.remove_dir_recursive(&path, sftp_engine::RemoveLimits::default())
                .await
                .map_err(map_fs)?;
        } else {
            fs.remove_dir(&path).await.map_err(map_fs)?;
        }
    } else {
        fs.remove_file(&path).await.map_err(map_fs)?;
    }
    Ok(())
}

/// Opens a file in the default local application. Remote files are downloaded to a private
/// temp folder and watched; when they change an `sftp://edited` event offers to upload them.
#[tauri::command]
#[specta::specta]
pub async fn fs_edit(
    app: AppHandle,
    files: State<'_, FilesState>,
    source: FsSource,
    path: String,
) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let local = match &source {
        FsSource::Local => path.clone(),
        FsSource::Remote { .. } => {
            let fs = files.fs(&app, &source).await?;
            let session = EditSession::open(fs, &path).await.map_err(map_fs)?;
            let local = session.local_path().to_string_lossy().into_owned();
            let edit_id = uuid::Uuid::now_v7().to_string();
            let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
            files.edits.lock().await.insert(edit_id.clone(), session);
            let edits = files.edits.clone();
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                let mut notified = false;
                loop {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    let mut guard = edits.lock().await;
                    let Some(s) = guard.get_mut(&edit_id) else {
                        break;
                    };
                    let modified = s.is_modified().await.unwrap_or(false);
                    drop(guard);
                    if modified && !notified {
                        let _ = app2.emit(
                            EDIT_EVENT,
                            EditedFile {
                                edit_id: edit_id.clone(),
                                name: name.clone(),
                            },
                        );
                        notified = true;
                    } else if !modified {
                        notified = false;
                    }
                }
            });
            local
        }
    };
    app.opener()
        .open_path(local, None::<&str>)
        .map_err(|e| CmdError::internal(e.to_string()))
}

/// Uploads a locally edited remote file.
#[tauri::command]
#[specta::specta]
pub async fn fs_upload_edit(
    files: State<'_, FilesState>,
    edit_id: String,
    force: bool,
) -> CmdResult<()> {
    let mut edits = files.edits.lock().await;
    let s = edits
        .get_mut(&edit_id)
        .ok_or_else(|| CmdError::not_found("edit session expired"))?;
    if force {
        s.upload_force().await
    } else {
        s.upload().await
    }
    .map_err(map_fs)
}

/// Downloads a remote file into the Downloads folder (via the transfer queue).
#[tauri::command]
#[specta::specta]
pub async fn fs_download(
    app: AppHandle,
    files: State<'_, FilesState>,
    source: FsSource,
    path: String,
) -> CmdResult<String> {
    let dir = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|h| std::path::PathBuf::from(h).join("Downloads"))
        .filter(|p| p.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    let dir = dir.to_string_lossy().into_owned();
    let ids = transfer_start(
        app,
        files,
        source,
        vec![path],
        FsSource::Local,
        dir.clone(),
        "keepBoth".into(),
    )
    .await?;
    Ok(ids.into_iter().next().unwrap_or(dir))
}

/// Queues a copy of `paths` from `from` into directory `to_dir` on `to`.
#[tauri::command]
#[specta::specta]
pub async fn transfer_start(
    app: AppHandle,
    files: State<'_, FilesState>,
    from: FsSource,
    paths: Vec<String>,
    to: FsSource,
    to_dir: String,
    conflict: String,
) -> CmdResult<Vec<String>> {
    let src = files.fs(&app, &from).await?;
    let dst = files.fs(&app, &to).await?;
    let conflict = match conflict.as_str() {
        "overwrite" => ConflictPolicy::Overwrite,
        "skip" => ConflictPolicy::Skip,
        // Without an interactive resolver, "ask" keeps both copies (never loses data).
        _ => ConflictPolicy::KeepBoth,
    };
    let mut ids = Vec::new();
    for p in paths {
        let name = src
            .file_name(&p)
            .ok_or_else(|| CmdError::invalid("invalid path"))?;
        let dest = dst.join(&to_dir, &name).map_err(map_fs)?;
        let id = files.queue.enqueue(TransferRequest {
            source: src.clone(),
            source_path: p,
            destination: dst.clone(),
            destination_path: dest,
            options: TransferOptions {
                conflict,
                ..TransferOptions::default()
            },
        });
        ids.push(id.to_string());
    }
    Ok(ids)
}

/// Lists queued, running and finished transfers.
#[tauri::command]
#[specta::specta]
pub fn transfer_list(files: State<'_, FilesState>) -> Vec<Transfer> {
    files.transfers.lock().clone()
}

/// Cancels a transfer.
#[tauri::command]
#[specta::specta]
pub fn transfer_cancel(files: State<'_, FilesState>, id: String) -> CmdResult<()> {
    let id: u64 = id
        .parse()
        .map_err(|_| CmdError::invalid("invalid transfer id"))?;
    files.queue.cancel(id);
    Ok(())
}

/// Removes finished transfers from the list.
#[tauri::command]
#[specta::specta]
pub fn transfer_clear_finished(files: State<'_, FilesState>) {
    files.queue.prune_finished();
    files.transfers.lock().retain(|t| {
        matches!(
            t.state,
            TransferState::Queued | TransferState::Running | TransferState::Paused
        )
    });
}

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
