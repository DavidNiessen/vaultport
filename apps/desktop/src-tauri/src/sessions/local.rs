// SPDX-License-Identifier: GPL-3.0-or-later
//! Local shell sessions through a pseudo terminal (ConPTY on Windows).

use super::{EventSink, Input, SessionPhase};
use crate::error::{CmdError, CmdResult};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use tokio::sync::mpsc;

fn default_shell() -> CommandBuilder {
    #[cfg(windows)]
    {
        let ps = std::env::var("SystemRoot")
            .map(|root| format!("{root}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"))
            .unwrap_or_else(|_| "powershell.exe".into());
        let mut cmd = CommandBuilder::new(ps);
        cmd.arg("-NoLogo");
        cmd
    }
    #[cfg(not(windows))]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let mut cmd = CommandBuilder::new(shell);
        cmd.arg("-l");
        cmd
    }
}

pub fn spawn(
    app: &tauri::AppHandle,
    id: String,
    cols: u16,
    rows: u16,
    mut input: mpsc::UnboundedReceiver<Input>,
    sink: EventSink,
) -> CmdResult<()> {
    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize {
            rows: rows.max(1),
            cols: cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| CmdError::internal(format!("could not open a pseudo terminal: {e}")))?;
    let mut cmd = default_shell();
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", "vaultport");
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        cmd.cwd(home);
    }
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| CmdError::internal(format!("could not start the shell: {e}")))?;
    drop(pair.slave);
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| CmdError::internal(e.to_string()))?;
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|e| CmdError::internal(e.to_string()))?;
    let master = pair.master;
    sink.status(SessionPhase::Connected);

    // Blocking reader thread → sink.
    let out = sink.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => out.output(&buf[..n]),
            }
        }
    });

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut killer = child.clone_killer();
        let (exit_tx, mut exit_rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let status = child.wait().ok();
            let _ = exit_tx.send(status.map(|s| s.exit_code()));
        });
        loop {
            tokio::select! {
                msg = input.recv() => match msg {
                    Some(Input::Data(d)) => {
                        if writer.write_all(&d).and_then(|()| writer.flush()).is_err() {
                            break;
                        }
                    }
                    Some(Input::Resize { cols, rows }) => {
                        let _ = master.resize(PtySize { rows: rows.max(1), cols: cols.max(1), pixel_width: 0, pixel_height: 0 });
                    }
                    Some(Input::Close) | None => {
                        let _ = killer.kill();
                        break;
                    }
                },
                code = &mut exit_rx => {
                    let code = code.ok().flatten();
                    sink.closed(code.filter(|c| *c != 0).map(|c| format!("shell exited with status {c}")), false);
                    super::finish(&app, &id);
                    return;
                }
            }
        }
        sink.closed(None, false);
        super::finish(&app, &id);
    });
    Ok(())
}
