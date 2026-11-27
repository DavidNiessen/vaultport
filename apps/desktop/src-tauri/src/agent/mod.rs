// SPDX-License-Identifier: GPL-3.0-or-later
//! AI agent access over the Model Context Protocol.
//!
//! When the user turns it on (Settings → AI agents), the app listens on a local socket that
//! only the current user can open (a `0600` Unix socket in the app data directory, or a
//! local-only named pipe on Windows). AI tools start `vaultport mcp`, a stdio MCP server that
//! relays to that socket after presenting a random token stored next to it.
//!
//! Guarantees:
//! * Off by default, local only, no network listener.
//! * Agents never see secrets: passwords, keys and the vault stay in the core.
//! * Every action that changes something (and, by default, every action) needs the user's
//!   approval in the app; grants last at most for one agent connection.
//! * Agents can be limited to selected hosts or to read-only tools, and every call is
//!   recorded in the audit log.

pub mod endpoint;
#[cfg(desktop)]
mod listener;
#[cfg(desktop)]
pub mod proxy;
pub mod tools;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{oneshot, watch};

use crate::error::{CmdError, CmdResult};

/// Event asking the UI to approve an agent action.
pub const APPROVAL_EVENT: &str = "agent://approval";
/// Event emitted when an approval request is no longer pending (answered elsewhere or timed out).
pub const APPROVAL_DONE_EVENT: &str = "agent://approval-done";
/// Event carrying a new audit entry.
pub const AUDIT_EVENT: &str = "agent://audit";
/// Event asking the UI to open a terminal tab for a host.
pub const OPEN_TERMINAL_EVENT: &str = "agent://open-terminal";

const APPROVAL_TIMEOUT: Duration = Duration::from_secs(300);
const AUDIT_CAPACITY: usize = 500;

/// When the user is asked before an agent acts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalPolicy {
    /// Ask before every tool call, including reads.
    #[default]
    Always,
    /// Reads (listing hosts, directories, reading files) run without asking.
    ChangesOnly,
}

/// Device-local agent settings (never synced: granting an agent access is a per-device choice).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentSettings {
    pub enabled: bool,
    pub approval: ApprovalPolicy,
    /// Only offer tools that do not change anything.
    pub read_only: bool,
    /// `true`: every saved SSH host; `false`: only [`AgentSettings::hosts`].
    pub all_hosts: bool,
    pub hosts: Vec<String>,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            approval: ApprovalPolicy::Always,
            read_only: false,
            all_hosts: true,
            hosts: Vec::new(),
        }
    }
}

/// An agent connected right now.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentClient {
    pub id: String,
    pub name: String,
    pub connected_at: f64,
}

/// What a client needs to be configured.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentClientConfig {
    /// "claude-code", "claude-desktop", "cursor", "vscode", "generic".
    pub client: String,
    /// "shell" (a command to run) or "json" (a config file snippet).
    pub kind: String,
    /// Where the snippet goes, if it is a file.
    pub location: Option<String>,
    pub text: String,
}

/// Agent settings and live state for the UI.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    pub supported: bool,
    pub settings: AgentSettings,
    pub listening: bool,
    pub error: Option<String>,
    pub clients: Vec<AgentClient>,
    /// Command that starts the MCP server (`<exe> mcp`).
    pub command: String,
    pub configs: Vec<AgentClientConfig>,
}

/// An action waiting for the user's decision.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentApprovalRequest {
    pub id: String,
    pub client: String,
    pub tool: String,
    pub title: String,
    pub host: Option<String>,
    /// What exactly will happen (command, path, content preview).
    pub details: String,
    pub destructive: bool,
}

/// The user's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AgentDecision {
    Deny,
    AllowOnce,
    /// Allow this tool on this host until the agent disconnects.
    AllowSession,
}

/// How a call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AuditOutcome {
    Ok,
    Error,
    Denied,
}

/// One recorded agent action.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub id: String,
    pub at: f64,
    pub client: String,
    pub tool: String,
    pub host: Option<String>,
    pub summary: String,
    pub outcome: AuditOutcome,
    pub message: Option<String>,
}

/// Shared agent state (managed by Tauri).
pub struct AgentState {
    data_dir: PathBuf,
    settings: Mutex<AgentSettings>,
    audit: Mutex<VecDeque<AuditEntry>>,
    pending: Mutex<HashMap<String, oneshot::Sender<AgentDecision>>>,
    /// `(connection, tool, host id)` allowed for the rest of a connection.
    grants: Mutex<HashSet<(u64, String, String)>>,
    clients: Mutex<HashMap<u64, AgentClient>>,
    /// Bumped to drop every connection (disable, token rotation).
    epoch: watch::Sender<u64>,
    next_connection: AtomicU64,
    listener: Mutex<ListenerState>,
}

#[derive(Default)]
struct ListenerState {
    task: Option<tauri::async_runtime::JoinHandle<()>>,
    error: Option<String>,
}

impl std::fmt::Debug for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentState").finish_non_exhaustive()
    }
}

fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64)
}

impl AgentState {
    pub fn new(data_dir: &Path) -> Self {
        let settings = std::fs::read(data_dir.join(endpoint::SETTINGS_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            data_dir: data_dir.to_owned(),
            settings: Mutex::new(settings),
            audit: Mutex::default(),
            pending: Mutex::default(),
            grants: Mutex::default(),
            clients: Mutex::default(),
            epoch: watch::Sender::new(0),
            next_connection: AtomicU64::new(1),
            listener: Mutex::default(),
        }
    }

    pub fn settings(&self) -> AgentSettings {
        self.settings.lock().clone()
    }

    fn save_settings(&self, s: &AgentSettings) -> CmdResult<()> {
        let bytes = serde_json::to_vec_pretty(s).map_err(|e| CmdError::internal(e.to_string()))?;
        endpoint::write_private(&self.data_dir.join(endpoint::SETTINGS_FILE), &bytes)
            .map_err(|e| CmdError::internal(e.to_string()))
    }

    /// Drops every agent connection and pending request.
    pub fn disconnect_all(&self) {
        self.epoch.send_modify(|e| *e += 1);
        self.grants.lock().clear();
        for (id, tx) in self.pending.lock().drain() {
            let _ = tx.send(AgentDecision::Deny);
            let _ = id;
        }
    }

    /// Forgets "allow for this session" grants (on vault lock).
    pub fn clear_grants(&self) {
        self.grants.lock().clear();
    }

    fn connection_id(&self) -> u64 {
        self.next_connection.fetch_add(1, Ordering::Relaxed)
    }

    pub fn audit_log(&self) -> Vec<AuditEntry> {
        self.audit.lock().iter().rev().cloned().collect()
    }

    fn record(&self, app: &AppHandle, entry: AuditEntry) {
        log::info!(
            "agent {} called {} on {:?}: {:?}",
            entry.client,
            entry.tool,
            entry.host,
            entry.outcome
        );
        {
            let mut a = self.audit.lock();
            if a.len() == AUDIT_CAPACITY {
                a.pop_front();
            }
            a.push_back(entry.clone());
        }
        let _ = app.emit(AUDIT_EVENT, entry);
    }

    pub fn respond(&self, id: &str, decision: AgentDecision) -> bool {
        self.pending
            .lock()
            .remove(id)
            .is_some_and(|tx| tx.send(decision).is_ok())
    }

    /// Asks the user; resolves to `false` when denied, timed out or disconnected.
    async fn approve(
        &self,
        app: &AppHandle,
        connection: u64,
        mut request: AgentApprovalRequest,
        host_id: Option<&str>,
    ) -> bool {
        let grant = (
            connection,
            request.tool.clone(),
            host_id.unwrap_or_default().to_owned(),
        );
        if self.grants.lock().contains(&grant) {
            return true;
        }
        let id = uuid::Uuid::now_v7().to_string();
        request.id.clone_from(&id);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().insert(id.clone(), tx);
        let _ = app.emit(APPROVAL_EVENT, request);
        #[cfg(desktop)]
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.request_user_attention(Some(tauri::UserAttentionType::Critical));
        }
        let decision = tokio::time::timeout(APPROVAL_TIMEOUT, rx).await;
        self.pending.lock().remove(&id);
        let _ = app.emit(APPROVAL_DONE_EVENT, &id);
        match decision {
            Ok(Ok(AgentDecision::AllowOnce)) => true,
            Ok(Ok(AgentDecision::AllowSession)) => {
                self.grants.lock().insert(grant);
                true
            }
            _ => false,
        }
    }

    fn connected(&self, id: u64, name: String) {
        self.clients.lock().insert(
            id,
            AgentClient {
                id: id.to_string(),
                name,
                connected_at: now_ms(),
            },
        );
    }

    fn connected_name(&self, id: u64, name: &str) {
        if let Some(c) = self.clients.lock().get_mut(&id)
            && c.name != name
        {
            name.clone_into(&mut c.name);
        }
    }

    fn disconnected(&self, id: u64) {
        self.clients.lock().remove(&id);
        self.grants.lock().retain(|g| g.0 != id);
    }
}

/// Applies new settings: saves them and starts or stops the listener.
pub fn update_settings(app: &AppHandle, settings: AgentSettings) -> CmdResult<()> {
    let state = app.state::<Arc<AgentState>>();
    let was_enabled = state.settings().enabled;
    state.save_settings(&settings)?;
    *state.settings.lock() = settings.clone();
    if was_enabled && !settings.enabled {
        stop(app);
    } else if settings.enabled {
        start(app);
    }
    Ok(())
}

/// Starts listening if agent access is enabled (idempotent).
pub fn start(app: &AppHandle) {
    #[cfg(desktop)]
    {
        let state = app.state::<Arc<AgentState>>().inner().clone();
        if !state.settings().enabled {
            return;
        }
        let mut l = state.listener.lock();
        if l.task.as_ref().is_some_and(|t| !t.inner().is_finished()) {
            return;
        }
        l.error = None;
        l.task = Some(listener::spawn(app.clone(), state.clone()));
    }
    #[cfg(not(desktop))]
    let _ = app;
}

/// Stops listening and drops every connection.
pub fn stop(app: &AppHandle) {
    let state = app.state::<Arc<AgentState>>();
    if let Some(t) = state.listener.lock().task.take() {
        t.abort();
    }
    state.disconnect_all();
    #[cfg(unix)]
    let _ = std::fs::remove_file(endpoint::socket_path(&state.data_dir));
}

/// Replaces the access token, disconnecting every agent.
pub fn rotate_token(app: &AppHandle) -> CmdResult<()> {
    let state = app.state::<Arc<AgentState>>();
    endpoint::create_token(&state.data_dir).map_err(|e| CmdError::internal(e.to_string()))?;
    state.disconnect_all();
    Ok(())
}

/// The UI snapshot.
pub fn status(app: &AppHandle) -> AgentStatus {
    let state = app.state::<Arc<AgentState>>();
    let command = std::env::current_exe()
        .map_or_else(|_| "vaultport".to_owned(), |p| p.display().to_string());
    let (listening, error) = {
        let l = state.listener.lock();
        (
            l.task.as_ref().is_some_and(|t| !t.inner().is_finished()),
            l.error.clone(),
        )
    };
    let mut clients: Vec<_> = state.clients.lock().values().cloned().collect();
    clients.sort_by(|a, b| a.connected_at.total_cmp(&b.connected_at));
    AgentStatus {
        supported: cfg!(desktop),
        settings: state.settings(),
        listening,
        error,
        clients,
        configs: endpoint::client_configs(&command),
        command,
    }
}

fn set_listener_error(state: &AgentState, error: String) {
    log::warn!("agent listener: {error}");
    state.listener.lock().error = Some(error);
}
