// SPDX-License-Identifier: GPL-3.0-or-later
//! Serial console sessions. Wired to the `serial-io` crate.

use super::resolve::unsupported;
use super::{EventSink, Input, SessionTarget};
use crate::error::CmdResult;
use tokio::sync::mpsc;

pub fn spawn(
    _app: &tauri::AppHandle,
    _id: String,
    _target: SessionTarget,
    _input: mpsc::UnboundedReceiver<Input>,
    sink: EventSink,
) -> CmdResult<()> {
    sink.closed(Some("serial support not linked".into()), true);
    Err(unsupported("Serial"))
}
