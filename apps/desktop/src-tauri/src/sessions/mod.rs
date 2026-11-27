// SPDX-License-Identifier: GPL-3.0-or-later
//! Interactive terminal sessions (SSH, Mosh, Telnet, serial, local shell).
//!
//! Every session runs as a tokio task that owns its protocol connection. The UI talks to it
//! through an input queue (keystrokes, resizes) and receives [`SessionEvent`]s on a Tauri
//! channel. Prompts (host keys, keyboard-interactive auth) are request/response pairs keyed by
//! a prompt id.

pub mod types;

#[cfg(desktop)]
mod local;
mod record;
pub mod resolve;
mod serial;
mod ssh;
mod telnet;

use crate::error::{CmdError, CmdResult, ErrorCode};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tauri::ipc::Channel;
use tokio::sync::{mpsc, oneshot};
pub use types::*;

/// Input from the UI to a running session.
#[derive(Debug)]
pub enum Input {
    Data(Vec<u8>),
    Resize { cols: u16, rows: u16 },
    Close,
}

type PromptMap = Arc<Mutex<HashMap<String, oneshot::Sender<PromptResponse>>>>;

struct Handle {
    input: mpsc::UnboundedSender<Input>,
    prompts: PromptMap,
}

/// Registry of live sessions.
#[derive(Default)]
pub struct SessionManager {
    sessions: Mutex<HashMap<String, Handle>>,
}

/// Output batching: coalesce small writes into at most one IPC message per 8 ms.
const FLUSH_INTERVAL: Duration = Duration::from_millis(8);
const MAX_BATCH: usize = 64 * 1024;

/// What a protocol backend uses to talk back to the UI.
#[derive(Clone)]
pub struct EventSink {
    channel: Channel<SessionEvent>,
    output: mpsc::UnboundedSender<Vec<u8>>,
    prompts: PromptMap,
}

impl EventSink {
    fn new(channel: Channel<SessionEvent>, prompts: PromptMap) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let out_channel = channel.clone();
        tauri::async_runtime::spawn(async move {
            let mut buf: Vec<u8> = Vec::with_capacity(MAX_BATCH);
            loop {
                let Some(first) = rx.recv().await else { break };
                buf.extend_from_slice(&first);
                let deadline = tokio::time::Instant::now() + FLUSH_INTERVAL;
                while buf.len() < MAX_BATCH {
                    match tokio::time::timeout_at(deadline, rx.recv()).await {
                        Ok(Some(more)) => buf.extend_from_slice(&more),
                        Ok(None) | Err(_) => break,
                    }
                }
                let data = STANDARD.encode(&buf);
                buf.clear();
                if out_channel.send(SessionEvent::Output { data }).is_err() {
                    break;
                }
            }
        });
        Self {
            channel,
            output: tx,
            prompts,
        }
    }

    /// Queues terminal output.
    pub fn output(&self, data: &[u8]) {
        if !data.is_empty() {
            let _ = self.output.send(data.to_vec());
        }
    }

    /// Sends a non-output event.
    pub fn event(&self, event: SessionEvent) {
        let _ = self.channel.send(event);
    }

    pub fn status(&self, status: SessionPhase) {
        self.event(SessionEvent::Status { status });
    }

    pub fn closed(&self, reason: Option<String>, error: bool) {
        self.event(SessionEvent::Closed { reason, error });
    }

    /// Sends a prompt built by `make` (which receives the prompt id) and waits for the answer.
    pub async fn ask(&self, make: impl FnOnce(String) -> SessionEvent) -> Option<PromptResponse> {
        let id = uuid::Uuid::now_v7().to_string();
        let (tx, rx) = oneshot::channel();
        self.prompts.lock().insert(id.clone(), tx);
        self.event(make(id.clone()));
        let answer = tokio::time::timeout(Duration::from_secs(600), rx).await;
        self.prompts.lock().remove(&id);
        answer.ok().and_then(Result::ok)
    }
}

impl SessionManager {
    /// Registers a new session and returns its id, input receiver and event sink.
    pub fn register(
        &self,
        channel: Channel<SessionEvent>,
    ) -> (String, mpsc::UnboundedReceiver<Input>, EventSink) {
        let id = uuid::Uuid::now_v7().to_string();
        let (tx, rx) = mpsc::unbounded_channel();
        let prompts: PromptMap = Arc::default();
        let sink = EventSink::new(channel, prompts.clone());
        self.sessions
            .lock()
            .insert(id.clone(), Handle { input: tx, prompts });
        (id, rx, sink)
    }

    pub fn remove(&self, id: &str) {
        self.sessions.lock().remove(id);
    }

    /// Number of open sessions.
    pub fn count(&self) -> usize {
        self.sessions.lock().len()
    }

    pub fn send(&self, id: &str, input: Input) -> CmdResult<()> {
        let guard = self.sessions.lock();
        let h = guard
            .get(id)
            .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "session not found"))?;
        h.input
            .send(input)
            .map_err(|_| CmdError::new(ErrorCode::NotFound, "session has ended"))
    }

    pub fn respond(&self, id: &str, prompt_id: &str, response: PromptResponse) -> CmdResult<()> {
        let tx = {
            let guard = self.sessions.lock();
            let h = guard
                .get(id)
                .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "session not found"))?;
            h.prompts.lock().remove(prompt_id)
        };
        let tx = tx.ok_or_else(|| CmdError::new(ErrorCode::NotFound, "prompt expired"))?;
        let _ = tx.send(response);
        Ok(())
    }

    /// Closes every session (e.g. when the vault locks).
    pub fn close_all(&self) {
        for h in self.sessions.lock().values() {
            let _ = h.input.send(Input::Close);
        }
    }
}

/// Spawns the backend for `target`.
pub fn spawn(
    app: &tauri::AppHandle,
    id: String,
    request: OpenSessionRequest,
    input: mpsc::UnboundedReceiver<Input>,
    sink: EventSink,
) -> CmdResult<()> {
    let (cols, rows) = (request.cols, request.rows);
    match request.target {
        #[cfg(desktop)]
        SessionTarget::Local => local::spawn(app, id, cols, rows, input, sink),
        #[cfg(mobile)]
        SessionTarget::Local => Err(CmdError::new(
            ErrorCode::Unsupported,
            "local terminals are not available on this platform",
        )),
        SessionTarget::Host { host_id } => {
            let plan = resolve::plan_for_host(app, &host_id)?;
            launch(app, id, plan, cols, rows, input, sink)
        }
        SessionTarget::Quick {
            host,
            port,
            username,
        } => {
            let plan = resolve::plan_for_quick(app, host, port, username)?;
            launch(app, id, plan, cols, rows, input, sink)
        }
        SessionTarget::Serial { .. } => serial::spawn(app, id, request.target, input, sink),
    }
}

fn launch(
    app: &tauri::AppHandle,
    id: String,
    plan: resolve::Plan,
    cols: u16,
    rows: u16,
    input: mpsc::UnboundedReceiver<Input>,
    sink: EventSink,
) -> CmdResult<()> {
    match plan {
        resolve::Plan::Telnet(t) => {
            telnet::spawn(app, id, t, cols, rows, input, sink);
            Ok(())
        }
        resolve::Plan::Ssh(s) => ssh::spawn(app, id, *s, cols, rows, input, sink),
    }
}

/// Removes a finished session from the manager.
pub fn finish(app: &tauri::AppHandle, id: &str) {
    use tauri::Manager;
    if let Some(m) = app.try_state::<SessionManager>() {
        m.remove(id);
    }
    crate::mobile::update_keep_alive(app);
}
