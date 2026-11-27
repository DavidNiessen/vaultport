// SPDX-License-Identifier: GPL-3.0-or-later
//! `vaultport mcp`: the stdio MCP server that AI tools start.
//!
//! It relays stdin/stdout to the running app's agent socket after presenting the access
//! token. If the app is not running it is started (agent access must have been enabled in
//! it); if agent access is off, a placeholder server explains how to turn it on.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use super::{AgentSettings, endpoint};

const START_TIMEOUT: Duration = Duration::from_secs(30);

/// Runs the relay; returns the process exit code.
pub fn run() -> i32 {
    let Ok(rt) = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    else {
        return 1;
    };
    let code = rt.block_on(async {
        match relay().await {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("vaultport mcp: {e}");
                1
            }
        }
    });
    // Do not wait for the blocking stdin reader.
    rt.shutdown_background();
    code
}

fn enabled(data_dir: &Path) -> bool {
    std::fs::read(data_dir.join(endpoint::SETTINGS_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<AgentSettings>(&b).ok())
        .is_some_and(|s| s.enabled)
}

async fn relay() -> std::io::Result<()> {
    let Some(data_dir) = endpoint::default_data_dir() else {
        return placeholder("Vaultport could not find its data directory.").await;
    };
    if !enabled(&data_dir) {
        return placeholder(
            "Agent access is turned off. Ask the user to open Vaultport → Settings → AI agents \
             and turn on \"Allow AI agents\", then reconnect this MCP server.",
        )
        .await;
    }
    let stream = if let Ok(s) = connect(&data_dir).await {
        s
    } else {
        {
            launch_app();
            let deadline = tokio::time::Instant::now() + START_TIMEOUT;
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if let Ok(s) = connect(&data_dir).await {
                    break s;
                }
                if tokio::time::Instant::now() > deadline {
                    return placeholder(
                        "Vaultport is not running and could not be started. Ask the user to \
                         open Vaultport, then reconnect this MCP server.",
                    )
                    .await;
                }
            }
        }
    };
    let token = endpoint::read_token(&data_dir)?;
    pipe(stream, &token).await
}

async fn pipe<S: AsyncRead + AsyncWrite + Send + 'static>(
    stream: S,
    token: &str,
) -> std::io::Result<()> {
    let (mut sr, mut sw) = tokio::io::split(stream);
    let mut hello = serde_json::to_vec(&serde_json::json!({ "auth": token }))?;
    hello.push(b'\n');
    sw.write_all(&hello).await?;
    let upstream = async move {
        let mut stdin = tokio::io::stdin();
        tokio::io::copy(&mut stdin, &mut sw).await?;
        sw.shutdown().await
    };
    let downstream = async move {
        let mut stdout = tokio::io::stdout();
        tokio::io::copy(&mut sr, &mut stdout).await?;
        stdout.flush().await
    };
    // When the client closes stdin, half-close and let the app finish answering; when the
    // app closes the connection, stop.
    let mut downstream = tokio::spawn(downstream);
    tokio::select! {
        r = upstream => {
            r?;
            downstream.await.map_err(std::io::Error::other)?
        }
        r = &mut downstream => r.map_err(std::io::Error::other)?,
    }
}

#[cfg(unix)]
async fn connect(data_dir: &Path) -> std::io::Result<tokio::net::UnixStream> {
    tokio::net::UnixStream::connect(endpoint::socket_path(data_dir)).await
}

#[cfg(windows)]
async fn connect(
    _data_dir: &Path,
) -> std::io::Result<tokio::net::windows::named_pipe::NamedPipeClient> {
    use tokio::net::windows::named_pipe::ClientOptions;
    let name = endpoint::pipe_name();
    for _ in 0..20 {
        match ClientOptions::new().open(&name) {
            Ok(c) => return Ok(c),
            // ERROR_PIPE_BUSY: every instance is in use; the server creates a new one.
            Err(e) if e.raw_os_error() == Some(231) => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("the agent pipe stayed busy"))
}

fn launch_app() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        // Keep the app running when the MCP client stops this relay.
        cmd.process_group(0);
    }
    let _ = cmd.spawn();
}

/// Serves a single `status` tool that explains why Vaultport is unavailable.
async fn placeholder(message: &'static str) -> std::io::Result<()> {
    struct Placeholder(&'static str);

    #[async_trait::async_trait]
    impl mcp_server::Handler for Placeholder {
        fn tools(&self) -> Vec<mcp_server::Tool> {
            vec![mcp_server::Tool {
                name: "vaultport_status".into(),
                title: Some("Vaultport status".into()),
                description: "Explains why Vaultport's tools are unavailable.".into(),
                input_schema: serde_json::json!({"type": "object", "properties": {}}),
                output_schema: None,
                annotations: Some(mcp_server::ToolAnnotations {
                    read_only_hint: Some(true),
                    ..mcp_server::ToolAnnotations::default()
                }),
            }]
        }

        async fn call(
            &self,
            _ctx: mcp_server::CallContext,
            _name: String,
            _arguments: serde_json::Value,
        ) -> mcp_server::ToolResult {
            mcp_server::ToolResult::error(self.0)
        }
    }

    let info = mcp_server::ServerInfo {
        name: "vaultport".into(),
        title: "Vaultport (unavailable)".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        instructions: Some(message.into()),
    };
    let stdin = tokio::io::BufReader::new(tokio::io::stdin());
    mcp_server::serve(
        Arc::new(Placeholder(message)),
        info,
        0,
        stdin,
        tokio::io::stdout(),
    )
    .await
}
