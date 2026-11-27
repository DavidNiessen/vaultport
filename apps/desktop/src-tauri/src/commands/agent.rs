// SPDX-License-Identifier: GPL-3.0-or-later
//! Settings, approvals and audit log for AI agent access (see [`crate::agent`]).

use std::sync::Arc;
use tauri::{AppHandle, State};

use crate::agent::{self, AgentDecision, AgentSettings, AgentState, AgentStatus, AuditEntry};
use crate::error::{CmdError, CmdResult, ErrorCode};

/// Agent settings, connected clients and setup snippets.
#[tauri::command]
#[specta::specta]
pub fn agent_status(app: AppHandle) -> AgentStatus {
    agent::status(&app)
}

/// Saves agent settings and starts or stops the local endpoint.
#[tauri::command]
#[specta::specta]
pub fn agent_save_settings(app: AppHandle, settings: AgentSettings) -> CmdResult<AgentStatus> {
    if !cfg!(desktop) && settings.enabled {
        return Err(CmdError::new(
            ErrorCode::Unsupported,
            "AI agents connect to the desktop app",
        ));
    }
    agent::update_settings(&app, settings)?;
    Ok(agent::status(&app))
}

/// Answers an approval request.
#[tauri::command]
#[specta::specta]
pub fn agent_respond(
    state: State<'_, Arc<AgentState>>,
    id: String,
    decision: AgentDecision,
) -> CmdResult<()> {
    if state.respond(&id, decision) {
        Ok(())
    } else {
        Err(CmdError::not_found("the request is no longer pending"))
    }
}

/// Recent agent actions, newest first.
#[tauri::command]
#[specta::specta]
pub fn agent_audit_log(state: State<'_, Arc<AgentState>>) -> Vec<AuditEntry> {
    state.audit_log()
}

/// Replaces the access token and disconnects every agent.
#[tauri::command]
#[specta::specta]
pub fn agent_rotate_token(app: AppHandle) -> CmdResult<AgentStatus> {
    agent::rotate_token(&app)?;
    Ok(agent::status(&app))
}

/// Disconnects every agent without changing settings.
#[tauri::command]
#[specta::specta]
pub fn agent_disconnect_all(state: State<'_, Arc<AgentState>>) {
    state.disconnect_all();
}
