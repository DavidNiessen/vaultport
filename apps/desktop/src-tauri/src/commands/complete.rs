// SPDX-License-Identifier: GPL-3.0-or-later
//! Terminal autocomplete backed by the deterministic `term-intel` engine.
//!
//! The UI sends the input line at the prompt; this answers with suggestions plus how many
//! characters to delete before and after the cursor, so accepting one is a matter of typing
//! keys. Completers are cached per host and rebuilt when the vault changes.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State};
use term_intel::{Completer, Completion, PathEntry, SuggestionKind};

use crate::commands::files::FilesState;
use crate::commands::logs::history_doc;
use crate::error::CmdResult;
use crate::state::AppState;

/// How long a cached completer is trusted before it is rebuilt from the vault.
const MAX_AGE: Duration = Duration::from_secs(60);
/// How long a directory listing may take before suggestions go out without paths.
const LIST_TIMEOUT: Duration = Duration::from_millis(400);
/// Directory entries passed to the engine at most.
const MAX_ENTRIES: usize = 2000;
/// Commands remembered per host until the session that ran them stores them.
const MAX_RECENT: usize = 500;
/// Longest input line that is completed.
const MAX_LINE: usize = 4096;

struct Entry {
    completer: Completer,
    built: Instant,
}

/// Cached completers, keyed by host id (`""` for sessions without a saved host).
#[derive(Default)]
pub struct CompletionState {
    cache: Mutex<HashMap<String, Entry>>,
    /// Commands run in this app session that the vault history may not have yet.
    recent: Mutex<HashMap<String, Vec<(String, i64)>>>,
}

fn key(host_id: Option<core_model::HostId>) -> String {
    host_id.map(|h| h.0.to_string()).unwrap_or_default()
}

impl CompletionState {
    /// Drops every cached completer, e.g. after the vault changed.
    pub fn invalidate(&self) {
        self.cache.lock().clear();
    }

    /// Forgets everything, including recent commands (on vault lock).
    pub fn clear(&self) {
        self.cache.lock().clear();
        self.recent.lock().clear();
    }

    /// Records a command that just ran so it is suggested right away.
    pub fn record(&self, host_id: Option<core_model::HostId>, command: &str) {
        let command = command.trim();
        if command.is_empty() {
            return;
        }
        let k = key(host_id);
        let now = now_secs();
        if let Some(e) = self.cache.lock().get_mut(&k) {
            e.completer.history_mut().record(command, Some(now));
        }
        let mut recent = self.recent.lock();
        let list = recent.entry(k).or_default();
        if list.len() < MAX_RECENT {
            list.push((command.to_owned(), now));
        }
    }

    /// Called once the session's commands are stored in the vault history.
    pub fn stored(&self, host_id: Option<core_model::HostId>) {
        self.recent.lock().remove(&key(host_id));
        self.invalidate();
    }

    fn completer(
        &self,
        state: &AppState,
        host_id: Option<core_model::HostId>,
    ) -> CmdResult<Completer> {
        let k = key(host_id);
        if let Some(e) = self.cache.lock().get(&k)
            && e.built.elapsed() < MAX_AGE
        {
            return Ok(e.completer.clone());
        }
        let (history, snippets) = state.with_vault(|v| {
            let history: term_intel::HistoryIndex = v
                .get_settings(&history_doc(host_id))?
                .and_then(|j| serde_json::from_value(j).ok())
                .unwrap_or_default();
            let snippets = v.list::<core_model::Snippet>()?;
            Ok((history, snippets))
        })?;
        let mut completer = Completer::new();
        completer.set_history(history);
        completer.set_snippets(&snippets);
        if let Some(recent) = self.recent.lock().get(&k) {
            for (cmd, ts) in recent {
                completer.history_mut().record(cmd, Some(*ts));
            }
        }
        self.cache.lock().insert(
            k,
            Entry {
                completer: completer.clone(),
                built: Instant::now(),
            },
        );
        Ok(completer)
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Where path suggestions come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PathSource {
    /// No listings (e.g. telnet or an unsaved host).
    None,
    /// This computer (local terminals).
    Local,
    /// The saved host, through an SFTP session that is already open.
    Remote,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CompleteRequest {
    pub host_id: Option<String>,
    /// The input line at the prompt.
    pub line: String,
    /// Cursor position in UTF-16 code units, as JavaScript counts.
    pub cursor: u32,
    /// The shell's working directory (from OSC 7), for relative paths.
    pub cwd: Option<String>,
    pub paths: PathSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CompletionKind {
    Path,
    Value,
    Subcommand,
    Option,
    Command,
    Snippet,
    History,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    /// Text to type after deleting (already shell-quoted).
    pub text: String,
    pub label: String,
    pub description: Option<String>,
    pub kind: CompletionKind,
    /// Characters to delete before the cursor (Backspace presses).
    pub delete_before: u32,
    /// Characters to delete after the cursor (Delete presses).
    pub delete_after: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CompletionList {
    /// Best first.
    pub items: Vec<CompletionItem>,
}

/// Byte offset of the UTF-16 position `cursor` in `line`, clamped to the line.
fn byte_offset(line: &str, cursor: u32) -> usize {
    let mut units = 0usize;
    for (i, c) in line.char_indices() {
        if units >= cursor as usize {
            return i;
        }
        units += c.len_utf16();
    }
    line.len()
}

fn chars(line: &str, from: usize, to: usize) -> u32 {
    line.get(from..to)
        .map_or(0, |s| u32::try_from(s.chars().count()).unwrap_or(u32::MAX))
}

fn kind(k: SuggestionKind) -> CompletionKind {
    match k {
        SuggestionKind::Path => CompletionKind::Path,
        SuggestionKind::Subcommand => CompletionKind::Subcommand,
        SuggestionKind::Option => CompletionKind::Option,
        SuggestionKind::Command => CompletionKind::Command,
        SuggestionKind::Snippet => CompletionKind::Snippet,
        SuggestionKind::History => CompletionKind::History,
        SuggestionKind::Value => CompletionKind::Value,
    }
}

/// Turns an engine result into keystroke-ready items. Suggestions that span lines are left
/// out: typing a newline would run the command.
fn to_list(line: &str, cursor: usize, completion: Completion) -> CompletionList {
    let items = completion
        .suggestions
        .into_iter()
        .filter(|s| !s.text.contains(['\n', '\r']) && !s.text.is_empty())
        .filter(|s| s.replace.start <= cursor && s.replace.end <= line.len())
        .map(|s| CompletionItem {
            delete_before: chars(line, s.replace.start, cursor),
            delete_after: chars(line, cursor, s.replace.end.max(cursor)),
            text: s.text,
            label: s.label,
            description: s.description,
            kind: kind(s.kind),
        })
        .collect();
    CompletionList { items }
}

/// The directory to list for `dir` as typed (see [`term_intel::NeedsPaths::dir`]), or `None`
/// when it cannot be resolved (relative without a known cwd, `~user/`).
fn resolve_dir(dir: &str, cwd: Option<&str>, home: Option<&str>) -> Option<String> {
    let join = |base: &str, rest: &str| {
        let base = base.trim_end_matches('/');
        if rest.is_empty() {
            if base.is_empty() {
                "/".to_owned()
            } else {
                base.to_owned()
            }
        } else {
            format!("{base}/{rest}")
        }
    };
    if dir.starts_with('/') {
        return Some(dir.to_owned());
    }
    if dir == "~" || dir.starts_with("~/") {
        return home.map(|h| join(h, dir.trim_start_matches('~').trim_start_matches('/')));
    }
    if dir.starts_with('~') {
        return None;
    }
    cwd.filter(|c| c.starts_with('/')).map(|c| join(c, dir))
}

/// Suggestions for the input line at the prompt.
#[tauri::command]
#[specta::specta]
pub async fn term_complete(
    state: State<'_, AppState>,
    completions: State<'_, CompletionState>,
    files: State<'_, FilesState>,
    req: CompleteRequest,
) -> CmdResult<CompletionList> {
    if req.line.len() > MAX_LINE {
        return Ok(CompletionList::default());
    }
    let host_id = req
        .host_id
        .as_deref()
        .and_then(|h| uuid::Uuid::parse_str(h).ok())
        .map(core_model::HostId);
    let completer = completions.completer(&state, host_id)?;
    let line = req.line;
    let cursor = byte_offset(&line, req.cursor);
    let now = now_secs();
    let mut completion = completer.complete(&line, cursor, now);

    if let Some(needs) = completion.needs_paths.clone() {
        let fs: Option<Arc<dyn sftp_engine::FileSystem>> = match req.paths {
            PathSource::None => None,
            PathSource::Local => Some(Arc::new(sftp_engine::Local)),
            PathSource::Remote => req.host_id.as_deref().and_then(|h| files.cached(h)),
        };
        if let Some(fs) = fs {
            let listed = tokio::time::timeout(LIST_TIMEOUT, async {
                let home = if needs.dir.starts_with('~') {
                    fs.home().await.ok()
                } else {
                    None
                };
                let dir = resolve_dir(&needs.dir, req.cwd.as_deref(), home.as_deref())?;
                fs.list(&dir).await.ok()
            })
            .await
            .ok()
            .flatten();
            if let Some(entries) = listed {
                let entries: Vec<PathEntry> = entries
                    .into_iter()
                    .filter(|e| {
                        !needs.directories_only || matches!(e.kind, sftp_engine::EntryKind::Dir)
                    })
                    .take(MAX_ENTRIES)
                    .map(|e| PathEntry {
                        is_dir: matches!(e.kind, sftp_engine::EntryKind::Dir),
                        name: e.name,
                    })
                    .collect();
                completion =
                    completer.complete_with_entries(&line, cursor, now, &needs.dir, &entries);
            }
        }
    }
    Ok(to_list(&line, cursor, completion))
}

/// Rebuilds completers whenever the vault reports a change (history, snippets, sync).
pub fn watch_vault(app: &AppHandle) {
    use tauri::Listener as _;
    let handle = app.clone();
    app.listen(crate::commands::entities::VAULT_CHANGED, move |_| {
        handle.state::<CompletionState>().invalidate();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_cursor_maps_to_bytes() {
        let line = "cd ä🙂x";
        // c d ␠ ä 🙂(2 units) x
        assert_eq!(byte_offset(line, 0), 0);
        assert_eq!(byte_offset(line, 3), 3);
        assert_eq!(byte_offset(line, 4), 5);
        assert_eq!(byte_offset(line, 6), 9);
        assert_eq!(byte_offset(line, 7), 10);
        assert_eq!(byte_offset(line, 99), line.len());
    }

    #[test]
    fn items_count_characters_to_delete() {
        let completer = Completer::new();
        let line = "git chx";
        let cursor = "git ch".len();
        let list = to_list(line, cursor, completer.complete(line, cursor, 0));
        let checkout = list
            .items
            .iter()
            .find(|i| i.text.starts_with("checkout"))
            .expect("git subcommands are suggested");
        assert_eq!(checkout.kind, CompletionKind::Subcommand);
        assert_eq!(checkout.delete_before, 2);
        // The engine replaces the word up to the cursor, so nothing after it is deleted.
        assert_eq!(checkout.delete_after, 0);
    }

    #[test]
    fn deletions_count_code_points() {
        let line = "cat ä🙂.txt";
        assert_eq!(chars(line, 4, line.len()), 6);
        assert_eq!(chars(line, 4, 4), 0);
        // A range inside a character is treated as nothing to delete rather than a panic.
        assert_eq!(chars(line, 5, 6), 0);
    }

    #[test]
    fn multi_line_suggestions_are_left_out() {
        let mut completer = Completer::new();
        completer
            .history_mut()
            .record("echo one\necho two", Some(0));
        completer.history_mut().record("echo three", Some(0));
        let list = to_list("ech", 3, completer.complete("ech", 3, 0));
        assert!(list.items.iter().any(|i| i.text == "echo three"));
        assert!(list.items.iter().all(|i| !i.text.contains('\n')));
    }

    #[test]
    fn directories_resolve_against_cwd_and_home() {
        assert_eq!(resolve_dir("/etc/", None, None).as_deref(), Some("/etc/"));
        assert_eq!(
            resolve_dir("", Some("/home/u"), None).as_deref(),
            Some("/home/u")
        );
        assert_eq!(
            resolve_dir("src/", Some("/home/u/"), None).as_deref(),
            Some("/home/u/src/")
        );
        assert_eq!(
            resolve_dir("~/", None, Some("/home/u")).as_deref(),
            Some("/home/u")
        );
        assert_eq!(
            resolve_dir("~/a/", None, Some("/home/u")).as_deref(),
            Some("/home/u/a/")
        );
        assert_eq!(resolve_dir("src/", None, None), None);
        assert_eq!(resolve_dir("~bob/", None, Some("/home/u")), None);
        assert_eq!(resolve_dir("src/", Some("relative"), None), None);
    }

    #[test]
    fn recorded_commands_survive_invalidation() {
        let state = CompletionState::default();
        state.record(None, "make deploy");
        state.invalidate();
        let recent: Vec<String> = state.recent.lock()[""]
            .iter()
            .map(|(c, _)| c.clone())
            .collect();
        assert_eq!(recent, ["make deploy"]);
        state.stored(None);
        assert!(state.recent.lock().is_empty());
        state.record(None, "  ");
        assert!(state.recent.lock().is_empty());
    }
}
