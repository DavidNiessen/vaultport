// SPDX-License-Identifier: GPL-3.0-or-later
//! Port forwarding rule activation.

use crate::error::{CmdError, CmdResult, ErrorCode};
use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ForwardStatus {
    pub rule_id: String,
    pub connections: u32,
    pub local_addr: String,
}

/// Active tunnels.
#[tauri::command]
#[specta::specta]
pub fn forward_status() -> Vec<ForwardStatus> {
    Vec::new()
}

/// Starts a saved rule.
#[tauri::command]
#[specta::specta]
pub fn forward_start(rule_id: String) -> CmdResult<ForwardStatus> {
    let _ = rule_id;
    Err(CmdError::new(
        ErrorCode::Unsupported,
        "port forwarding requires the SSH engine",
    ))
}

/// Stops a running rule.
#[tauri::command]
#[specta::specta]
pub fn forward_stop(rule_id: String) -> CmdResult<()> {
    let _ = rule_id;
    Ok(())
}
