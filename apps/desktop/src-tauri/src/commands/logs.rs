// SPDX-License-Identifier: GPL-3.0-or-later
//! Encrypted session logs (asciicast v2) and command history.

use crate::error::{CmdError, CmdResult, ErrorCode};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use session_log::{LogId, LogMetadata, LogReader};
use specta::Type;
use std::path::PathBuf;
use tauri::State;

pub const LOG_INDEX: &str = "logIndex";
pub const LOG_KEY_PURPOSE: &str = "session-log";
pub const HISTORY: &str = "history";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct LogIndex {
    pub logs: Vec<LogMetadata>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LogInfo {
    pub id: String,
    pub host_id: Option<String>,
    pub host_label: String,
    pub started_at: f64,
    pub ended_at: Option<f64>,
    pub bytes: f64,
    pub bookmarked: bool,
    pub comment: Option<String>,
    pub device_name: String,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LogEventDto {
    pub time: f64,
    pub kind: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LogContent {
    pub cols: u16,
    pub rows: u16,
    pub duration: f64,
    pub complete: bool,
    pub events: Vec<LogEventDto>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub command: String,
    pub count: u32,
}

pub fn logs_dir(state: &AppState) -> PathBuf {
    state.data_dir.join("logs")
}

pub fn load_index(v: &vault::Vault) -> CmdResult<LogIndex> {
    Ok(v.get_settings(LOG_INDEX)?
        .and_then(|j| serde_json::from_value(j).ok())
        .unwrap_or_default())
}

pub fn save_index(v: &mut vault::Vault, index: &LogIndex) -> CmdResult<()> {
    let value = serde_json::to_value(index).map_err(|e| CmdError::internal(e.to_string()))?;
    v.put_settings(LOG_INDEX, &value, true)?;
    Ok(())
}

fn info(m: &LogMetadata) -> LogInfo {
    LogInfo {
        id: m.id.0.to_string(),
        host_id: m.host_id.map(|h| h.to_string()),
        host_label: m.host_label.clone(),
        started_at: m.started_at as f64,
        ended_at: m.ended_at.map(|e| e as f64),
        bytes: m.bytes as f64,
        bookmarked: m.bookmarked,
        comment: m.comment.clone(),
        device_name: m.device_name.clone(),
    }
}

fn parse_id(id: &str) -> CmdResult<LogId> {
    uuid::Uuid::parse_str(id)
        .map(LogId)
        .map_err(|_| CmdError::invalid("invalid log id"))
}

/// Lists recorded session logs (newest first).
#[tauri::command]
#[specta::specta]
pub fn list_logs(state: State<'_, AppState>) -> CmdResult<Vec<LogInfo>> {
    state.with_vault(|v| {
        let mut logs: Vec<LogInfo> = load_index(v)?.logs.iter().map(info).collect();
        logs.sort_by(|a, b| b.started_at.total_cmp(&a.started_at));
        Ok(logs)
    })
}

/// Bookmarks (keeps) a log, optionally with a comment.
#[tauri::command]
#[specta::specta]
pub fn log_bookmark(
    state: State<'_, AppState>,
    id: String,
    bookmarked: bool,
    comment: Option<String>,
) -> CmdResult<()> {
    let lid = parse_id(&id)?;
    state.with_vault(|v| {
        let mut idx = load_index(v)?;
        let m = idx
            .logs
            .iter_mut()
            .find(|m| m.id == lid)
            .ok_or_else(|| CmdError::not_found("log not found"))?;
        m.bookmarked = bookmarked;
        if comment.is_some() || !bookmarked {
            m.comment = comment.filter(|c| !c.trim().is_empty());
        }
        save_index(v, &idx)
    })
}

/// Deletes a log that is not bookmarked.
#[tauri::command]
#[specta::specta]
pub fn log_delete(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let lid = parse_id(&id)?;
    let dir = logs_dir(&state);
    state.with_vault(|v| {
        let mut idx = load_index(v)?;
        if idx.logs.iter().any(|m| m.id == lid && m.bookmarked) {
            return Err(CmdError::invalid(
                "remove the bookmark before deleting this log",
            ));
        }
        idx.logs.retain(|m| m.id != lid);
        let _ = std::fs::remove_file(dir.join(format!("{lid}.sshlog", lid = lid.0)));
        save_index(v, &idx)
    })
}

fn open_reader(
    state: &AppState,
    id: LogId,
) -> CmdResult<LogReader<std::io::BufReader<std::fs::File>>> {
    let key = state.with_vault(|v| Ok(v.derive_subkey(LOG_KEY_PURPOSE)?))?;
    let path = logs_dir(state).join(format!("{}.sshlog", id.0));
    let file =
        std::fs::File::open(&path).map_err(|_| CmdError::not_found("the log file is missing"))?;
    LogReader::open_recovering(std::io::BufReader::new(file), &key, id)
        .map_err(|e| CmdError::new(ErrorCode::Authentication, e.to_string()))
}

/// Decrypts a log for replay in a read-only terminal tab.
#[tauri::command]
#[specta::specta]
pub fn log_read(state: State<'_, AppState>, id: String) -> CmdResult<LogContent> {
    let lid = parse_id(&id)?;
    let reader = open_reader(&state, lid)?;
    let header = reader.header().clone();
    let complete = reader.is_complete();
    let mut events = Vec::new();
    let mut duration: f64 = 0.0;
    for ev in reader {
        let ev = ev.map_err(|e| CmdError::new(ErrorCode::Authentication, e.to_string()))?;
        duration = duration.max(ev.time);
        if events.len() < 500_000 {
            events.push(LogEventDto {
                time: ev.time,
                kind: ev.kind.code().to_owned(),
                data: ev.data,
            });
        }
    }
    Ok(LogContent {
        cols: header.width,
        rows: header.height,
        duration,
        complete,
        events,
    })
}

/// Exports a decrypted `.cast` file next to the user's downloads.
#[tauri::command]
#[specta::specta]
pub fn log_export(state: State<'_, AppState>, id: String) -> CmdResult<String> {
    let lid = parse_id(&id)?;
    let reader = open_reader(&state, lid)?;
    let dir = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|h| PathBuf::from(h).join("Downloads"))
        .filter(|p| p.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    let path = dir.join(format!("session-{}.cast", lid.0));
    let file = std::fs::File::create(&path)?;
    reader
        .export_cast(std::io::BufWriter::new(file))
        .map_err(|e| CmdError::internal(e.to_string()))?;
    Ok(path.to_string_lossy().into_owned())
}

/// Recently used commands for a host (from shell integration), most frequent first.
#[tauri::command]
#[specta::specta]
pub fn history_list(
    state: State<'_, AppState>,
    host_id: Option<String>,
    limit: u32,
) -> CmdResult<Vec<HistoryItem>> {
    let name = host_id.map_or_else(|| HISTORY.to_owned(), |h| format!("{HISTORY}-{h}"));
    let name: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(64)
        .collect();
    state.with_vault(|v| {
        let index: term_intel::history::HistoryIndex = v
            .get_settings(&name)?
            .and_then(|j| serde_json::from_value(j).ok())
            .unwrap_or_default();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        Ok(index
            .prefix_matches("", now, limit as usize)
            .into_iter()
            .map(|(e, _)| HistoryItem {
                command: e.command.clone(),
                count: e.count,
            })
            .collect())
    })
}
