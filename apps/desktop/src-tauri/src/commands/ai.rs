// SPDX-License-Identifier: GPL-3.0-or-later
//! Optional local AI command generation (on-device model, no network after download).

use crate::error::{CmdError, CmdResult, ErrorCode};
use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, Copy, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AiState {
    NotInstalled,
    Downloading,
    Installed,
    Loading,
    Ready,
    Error,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    pub state: AiState,
    pub model_name: String,
    pub license: String,
    pub downloaded: f64,
    pub total: f64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiSuggestion {
    pub command: String,
    pub explanation: String,
    pub risk: String,
}

fn unavailable() -> CmdError {
    CmdError::new(
        ErrorCode::Unsupported,
        "local AI is not available in this build",
    )
}

/// Model installation status.
#[tauri::command]
#[specta::specta]
pub fn ai_status() -> AiStatus {
    AiStatus {
        state: AiState::NotInstalled,
        model_name: "Qwen2.5-Coder-1.5B-Instruct (Q4_K_M)".into(),
        license: "Apache-2.0".into(),
        downloaded: 0.0,
        total: 0.0,
        error: None,
    }
}

/// Downloads and verifies the model.
#[tauri::command]
#[specta::specta]
pub fn ai_install() -> CmdResult<()> {
    Err(unavailable())
}

/// Cancels a running download.
#[tauri::command]
#[specta::specta]
pub fn ai_cancel() {}

/// Deletes the downloaded model.
#[tauri::command]
#[specta::specta]
pub fn ai_remove() -> CmdResult<()> {
    Ok(())
}

/// Generates one shell command for a natural-language request.
#[tauri::command]
#[specta::specta]
pub fn ai_generate_command(
    prompt: String,
    tab_id: String,
    selection: Option<String>,
) -> CmdResult<AiSuggestion> {
    let _ = (prompt, tab_id, selection);
    Err(unavailable())
}

/// Generates a snippet script from a description.
#[tauri::command]
#[specta::specta]
pub fn ai_generate_snippet(description: String) -> CmdResult<String> {
    let _ = description;
    Err(unavailable())
}
