// SPDX-License-Identifier: GPL-3.0-or-later
//! Per-session side channels: encrypted session logs, shell-integration tracking and command
//! history. Failures here never interrupt the terminal session.

use crate::commands::complete::CompletionState;
use crate::commands::logs::{LOG_KEY_PURPOSE, load_index, logs_dir, save_index, update_history};
use crate::prefs::LoggingPrefs;
use crate::state::AppState;
use core_model::HostId;
use session_log::{CastHeader, LogId, LogMetadata, LogWriter, RetentionPolicy, WriterOptions};
use std::fs::File;
use std::io::BufWriter;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use term_intel::shell::{ShellEvent, ShellIntegrationParser};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn device_name() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| std::env::consts::OS.to_owned())
}

/// Records one session.
pub struct Recorder {
    app: AppHandle,
    host_id: Option<HostId>,
    host_label: String,
    writer: Option<LogWriter<BufWriter<File>>>,
    id: LogId,
    started_at: i64,
    parser: ShellIntegrationParser,
    history: Vec<String>,
}

impl Recorder {
    /// Starts recording if session logging is enabled (Settings → Security → Session logs).
    pub fn start(
        app: &AppHandle,
        host_id: Option<HostId>,
        host_label: &str,
        cols: u16,
        rows: u16,
    ) -> Self {
        let id = LogId::new();
        let started_at = now_ms();
        let writer = (|| -> Option<LogWriter<BufWriter<File>>> {
            let state = app.state::<AppState>();
            let prefs: LoggingPrefs = crate::prefs::load(app, crate::prefs::LOGGING);
            if !prefs.enabled {
                return None;
            }
            let key = state
                .with_vault(|v| Ok(v.derive_subkey(LOG_KEY_PURPOSE)?))
                .ok()?;
            let dir = logs_dir(&state);
            std::fs::create_dir_all(&dir).ok()?;
            let file = File::create(dir.join(format!("{}.sshlog", id.0))).ok()?;
            let mut header = CastHeader::new(cols, rows, Some(started_at / 1000));
            header.title = Some(host_label.to_owned());
            LogWriter::create(
                BufWriter::new(file),
                &key,
                id,
                &header,
                WriterOptions::default(),
            )
            .ok()
        })();
        Self {
            app: app.clone(),
            host_id,
            host_label: host_label.to_owned(),
            writer,
            id,
            started_at,
            parser: ShellIntegrationParser::new(),
            history: Vec::new(),
        }
    }

    /// Feeds terminal output.
    pub fn output(&mut self, data: &[u8]) {
        if let Some(w) = self.writer.as_mut() {
            let _ = w.output(data);
            let _ = w.poll_flush();
        }
        for ev in self.parser.feed(data) {
            match ev {
                ShellEvent::CommandExecuted { command: Some(cmd) } => {
                    if !cmd.trim().is_empty() && self.history.len() < 10_000 {
                        self.app
                            .state::<CompletionState>()
                            .record(self.host_id, &cmd);
                        self.history.push(cmd);
                    }
                }
                ShellEvent::PasswordPrompt => {
                    if let Some(w) = self.writer.as_mut() {
                        w.set_sensitive(true);
                    }
                }
                ShellEvent::PasswordPromptEnded => {
                    if let Some(w) = self.writer.as_mut() {
                        w.set_sensitive(false);
                    }
                }
                _ => {}
            }
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        if let Some(w) = self.writer.as_mut() {
            let _ = w.resize(cols, rows);
        }
    }

    /// Finalizes the log, applies retention and stores command history.
    pub fn finish(self) {
        let state = self.app.state::<AppState>();
        let mut bytes = 0;
        let had_log = self.writer.is_some();
        if let Some(w) = self.writer
            && w.finish().is_ok()
        {
            bytes = std::fs::metadata(logs_dir(&state).join(format!("{}.sshlog", self.id.0)))
                .map_or(0, |m| m.len());
        }
        let dir = logs_dir(&state);
        let host_id = self.host_id;
        let history = self.history;
        let _ = state.with_vault(|v| {
            if had_log {
                let mut idx = load_index(v)?;
                idx.logs.push(LogMetadata {
                    id: self.id,
                    host_id,
                    host_label: self.host_label.clone(),
                    started_at: self.started_at,
                    ended_at: Some(now_ms()),
                    bytes,
                    bookmarked: false,
                    comment: None,
                    device_name: device_name(),
                });
                let policy = RetentionPolicy {
                    keep_per_host: Some(1),
                    max_total_bytes: Some(1024 * 1024 * 1024),
                };
                let delete = session_log::select_for_deletion(&idx.logs, &policy);
                for id in &delete {
                    let _ = std::fs::remove_file(dir.join(format!("{}.sshlog", id.0)));
                }
                idx.logs.retain(|m| !delete.contains(&m.id));
                save_index(v, &idx)?;
            }
            if !history.is_empty() {
                let ts = now_ms() / 1000;
                update_history(v, host_id, |index| {
                    for cmd in &history {
                        index.record(cmd, Some(ts));
                    }
                })?;
                self.app.state::<CompletionState>().stored(host_id);
            }
            Ok(())
        });
    }
}
