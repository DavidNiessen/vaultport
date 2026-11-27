// SPDX-License-Identifier: GPL-3.0-or-later
//! The local socket (Unix) / named pipe (Windows) that `vaultport mcp` relays to.

use std::io;
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, BufReader};

use super::{AgentState, endpoint, set_listener_error, tools};

const AUTH_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_AUTH_LINE: u64 = 512;

pub fn spawn(app: AppHandle, state: Arc<AgentState>) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run(&app, &state).await {
            set_listener_error(&state, e.to_string());
        }
    })
}

#[cfg(unix)]
async fn run(app: &AppHandle, state: &Arc<AgentState>) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

    endpoint::load_or_create_token(&state.data_dir)?;
    let path = endpoint::socket_path(&state.data_dir);
    endpoint::prepare_socket_dir(&state.data_dir, &path)?;
    // A stale socket from a crash; the single-instance plugin rules out a live one.
    let _ = std::fs::remove_file(&path);
    let listener = tokio::net::UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let owner = std::fs::metadata(&state.data_dir)?.uid();
    loop {
        let (stream, _) = listener.accept().await?;
        // Defense in depth on top of the file mode and the token: same user only.
        match stream.peer_cred() {
            Ok(c) if c.uid() == owner => {}
            _ => continue,
        }
        let (app, state) = (app.clone(), state.clone());
        tauri::async_runtime::spawn(async move { handle(app, state, stream).await });
    }
}

#[cfg(windows)]
async fn run(app: &AppHandle, state: &Arc<AgentState>) -> io::Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;

    endpoint::load_or_create_token(&state.data_dir)?;
    let name = endpoint::pipe_name();
    // `first_pipe_instance` fails if someone else already owns the name; remote clients are
    // rejected by default.
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .reject_remote_clients(true)
        .create(&name)?;
    loop {
        server.connect().await?;
        let client = server;
        server = ServerOptions::new()
            .reject_remote_clients(true)
            .create(&name)?;
        let (app, state) = (app.clone(), state.clone());
        tauri::async_runtime::spawn(async move { handle(app, state, client).await });
    }
}

#[cfg(not(any(unix, windows)))]
async fn run(_app: &AppHandle, _state: &Arc<AgentState>) -> io::Result<()> {
    Err(io::Error::other(
        "agent access is not supported on this platform",
    ))
}

#[derive(serde::Deserialize)]
struct Auth {
    auth: String,
}

async fn handle<S>(app: AppHandle, state: Arc<AgentState>, stream: S)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (r, w) = tokio::io::split(stream);
    let mut reader = BufReader::new(r);
    let mut line = String::new();
    let read = tokio::time::timeout(
        AUTH_TIMEOUT,
        (&mut reader).take(MAX_AUTH_LINE).read_line(&mut line),
    )
    .await;
    if !matches!(read, Ok(Ok(n)) if n > 0) {
        return;
    }
    let Ok(expected) = endpoint::read_token(&state.data_dir) else {
        return;
    };
    let ok = serde_json::from_str::<Auth>(line.trim())
        .is_ok_and(|a| endpoint::token_eq(&a.auth, &expected));
    if !ok {
        log::warn!("rejected an agent connection with a wrong token");
        return;
    }
    let id = state.connection_id();
    state.connected(id, "AI agent".to_owned());
    let mut epoch = state.epoch.subscribe();
    let handler = Arc::new(tools::AgentHandler::new(app, state.clone()));
    let info = mcp_server::ServerInfo {
        name: "vaultport".into(),
        title: "Vaultport".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        instructions: Some(tools::INSTRUCTIONS.into()),
    };
    tokio::select! {
        _ = mcp_server::serve(handler, info, id, reader, w) => {}
        _ = epoch.changed() => {}
    }
    state.disconnected(id);
}
