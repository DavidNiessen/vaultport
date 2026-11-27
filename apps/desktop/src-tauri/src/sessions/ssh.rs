// SPDX-License-Identifier: GPL-3.0-or-later
//! SSH and Mosh sessions. Wired to the `ssh-engine` crate.

use super::resolve::{SshPlan, unsupported};
use super::{EventSink, Input};
use crate::error::CmdResult;
use tokio::sync::mpsc;

pub fn spawn(
    _app: &tauri::AppHandle,
    _id: String,
    _plan: SshPlan,
    _cols: u16,
    _rows: u16,
    _input: mpsc::UnboundedReceiver<Input>,
    sink: EventSink,
) -> CmdResult<()> {
    sink.closed(Some("SSH engine not linked".into()), true);
    Err(unsupported("SSH"))
}
