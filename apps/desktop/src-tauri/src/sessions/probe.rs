// SPDX-License-Identifier: GPL-3.0-or-later
//! Optional exec-channel probes after an SSH login (Settings → Terminal):
//!
//! * **Detect remote operating system** fills in the OS of a saved host whose OS is unknown;
//! * **Import remote shell history** reads the bash, zsh and fish history files once per host
//!   into the host's command history.
//!
//! Each probe opens one short-lived SSH channel at a time, in the background, after the shell
//! is up. Failures are ignored: the terminal session never depends on them.

use crate::commands::entities::{VAULT_CHANGED, VaultChanged};
use crate::commands::logs::update_history;
use crate::prefs::TerminalPrefs;
use crate::state::AppState;
use core_model::{Host, HostId, OsKind};
use ssh_engine::Connection;
use tauri::{AppHandle, Emitter as _, Manager};
use term_intel::history::{self, HistoryEntry, HistoryFormat};

/// Settings document mapping host ids to the time (unix ms) their history was imported.
pub const HISTORY_IMPORTED: &str = "historyImported";
/// Only the end of each history file is read.
const MAX_HISTORY_BYTES: usize = 4 * 1024 * 1024;

/// Starts the probes enabled in `prefs` for a saved host.
pub fn spawn(app: &AppHandle, conn: &Connection, host_id: Option<HostId>, prefs: &TerminalPrefs) {
    let Some(host_id) = host_id else { return };
    let (detect, import) = (prefs.detect_os, prefs.import_history);
    if !detect && !import {
        return;
    }
    let (app, conn) = (app.clone(), conn.clone());
    tauri::async_runtime::spawn(async move {
        if detect {
            detect_os(&app, &conn, host_id).await;
        }
        if import {
            import_history(&app, &conn, host_id).await;
        }
    });
}

fn notify(app: &AppHandle, kind: &str) {
    let _ = app.emit(
        VAULT_CHANGED,
        VaultChanged {
            kind: kind.to_owned(),
        },
    );
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

async fn detect_os(app: &AppHandle, conn: &Connection, host_id: HostId) {
    let state = app.state::<AppState>();
    let id = host_id.to_string();
    let unknown = state
        .with_vault(|v| Ok(v.get::<Host>(&id)?.is_some_and(|h| h.os == OsKind::Unknown)))
        .unwrap_or(false);
    if !unknown {
        return;
    }
    let os = conn.detect_os_kind().await;
    if os == OsKind::Unknown {
        return;
    }
    let changed = state
        .with_vault(|v| {
            let Some(mut host) = v.get::<Host>(&id)? else {
                return Ok(false);
            };
            if host.os != OsKind::Unknown {
                return Ok(false);
            }
            host.os = os;
            v.put(&host)?;
            Ok(true)
        })
        .unwrap_or(false);
    if changed {
        notify(app, "host");
    }
}

/// The command that prints the end of a history file in the remote home directory.
fn tail_command(format: HistoryFormat) -> String {
    format!(
        "tail -c {MAX_HISTORY_BYTES} -- \"$HOME/{}\" 2>/dev/null",
        format.default_path()
    )
}

/// Parses the output of [`tail_command`], dropping a first line that `tail` may have cut.
fn parse_tail(format: HistoryFormat, data: &[u8]) -> Vec<HistoryEntry> {
    let data = if data.len() >= MAX_HISTORY_BYTES {
        data.iter()
            .position(|&b| b == b'\n')
            .map_or(&[][..], |i| &data[i + 1..])
    } else {
        data
    };
    history::parse(format, data)
}

async fn import_history(app: &AppHandle, conn: &Connection, host_id: HostId) {
    let state = app.state::<AppState>();
    let key = host_id.to_string();
    let done = state
        .with_vault(|v| Ok(v.get_settings(HISTORY_IMPORTED)?))
        .ok()
        .flatten()
        .is_some_and(|doc| doc.get(&key).is_some());
    if done {
        return;
    }
    let mut entries = Vec::new();
    let mut reached = false;
    for format in [HistoryFormat::Bash, HistoryFormat::Zsh, HistoryFormat::Fish] {
        if let Ok(out) = conn.exec(&tail_command(format)).await {
            reached = true;
            entries.extend(parse_tail(format, &out.stdout));
        }
    }
    if !reached {
        // No channel could be opened (e.g. MaxSessions 1); try again next time.
        return;
    }
    let stored = state.with_vault(|v| {
        if !entries.is_empty() {
            update_history(v, Some(host_id), |index| index.import(&entries))?;
        }
        let mut doc = v
            .get_settings(HISTORY_IMPORTED)?
            .filter(serde_json::Value::is_object)
            .unwrap_or_else(|| serde_json::json!({}));
        doc[key.as_str()] = serde_json::json!(now_ms());
        v.put_settings(HISTORY_IMPORTED, &doc, true)?;
        Ok(())
    });
    if stored.is_ok() && !entries.is_empty() {
        notify(app, "history");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_reads_the_default_history_files() {
        assert_eq!(
            tail_command(HistoryFormat::Zsh),
            "tail -c 4194304 -- \"$HOME/.zsh_history\" 2>/dev/null"
        );
        assert!(tail_command(HistoryFormat::Fish).contains("$HOME/.local/share/fish/fish_history"));
    }

    #[test]
    fn truncated_output_drops_the_partial_first_line() {
        let mut data = vec![b'x'; MAX_HISTORY_BYTES - 8];
        data.extend_from_slice(b"\nls -la\n");
        let entries = parse_tail(HistoryFormat::Bash, &data);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].command, "ls -la");
        let entries = parse_tail(HistoryFormat::Bash, b"cd /tmp\nls\n");
        assert_eq!(entries.len(), 2);
    }
}
