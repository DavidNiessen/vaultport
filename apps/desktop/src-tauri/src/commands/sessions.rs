// SPDX-License-Identifier: GPL-3.0-or-later
//! Terminal session commands.

use crate::error::{CmdError, CmdResult};
use crate::sessions::{
    self, Input, OpenSessionRequest, PromptResponse, SessionEvent, SessionManager,
};
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

const MAX_INPUT: usize = 1024 * 1024;

/// Opens a terminal session. Events (output, prompts, closure) arrive on `events`.
#[tauri::command]
#[specta::specta]
pub fn session_open(
    app: AppHandle,
    manager: State<'_, SessionManager>,
    request: OpenSessionRequest,
    events: Channel<SessionEvent>,
) -> CmdResult<String> {
    if request.cols == 0 || request.rows == 0 || request.cols > 1000 || request.rows > 1000 {
        return Err(CmdError::invalid("invalid terminal size"));
    }
    let (id, input, sink) = manager.register(events);
    if let Err(e) = sessions::spawn(&app, id.clone(), request, input, sink) {
        manager.remove(&id);
        return Err(e);
    }
    crate::mobile::update_keep_alive(&app);
    Ok(id)
}

/// Sends keystrokes / pasted text to a session.
#[tauri::command]
#[specta::specta]
pub fn session_write(
    manager: State<'_, SessionManager>,
    id: String,
    data: String,
) -> CmdResult<()> {
    if data.len() > MAX_INPUT {
        return Err(CmdError::invalid("input too large"));
    }
    manager.send(&id, Input::Data(data.into_bytes()))
}

/// Resizes the remote pseudo terminal.
#[tauri::command]
#[specta::specta]
pub fn session_resize(
    manager: State<'_, SessionManager>,
    id: String,
    cols: u16,
    rows: u16,
) -> CmdResult<()> {
    if cols == 0 || rows == 0 || cols > 1000 || rows > 1000 {
        return Err(CmdError::invalid("invalid terminal size"));
    }
    manager.send(&id, Input::Resize { cols, rows })
}

/// Closes a session.
#[tauri::command]
#[specta::specta]
pub fn session_close(manager: State<'_, SessionManager>, id: String) -> CmdResult<()> {
    // Closing an already-finished session is not an error.
    let _ = manager.send(&id, Input::Close);
    Ok(())
}

/// Answers a prompt (host key decision, keyboard-interactive answers).
#[tauri::command]
#[specta::specta]
pub fn session_respond(
    manager: State<'_, SessionManager>,
    id: String,
    prompt_id: String,
    response: PromptResponse,
) -> CmdResult<()> {
    manager.respond(&id, &prompt_id, response)
}

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SerialPort {
    pub path: String,
    pub description: Option<String>,
}

/// Serial ports available on this device.
#[tauri::command]
#[specta::specta]
pub fn serial_list_ports() -> CmdResult<Vec<SerialPort>> {
    let ports = serial_io::list_ports().map_err(|e| CmdError::internal(e.to_string()))?;
    Ok(ports
        .into_iter()
        .map(|p| SerialPort {
            path: p.path,
            description: p.description,
        })
        .collect())
}
