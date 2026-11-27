// SPDX-License-Identifier: GPL-3.0-or-later
//! Local shell sessions through a pseudo terminal (ConPTY on Windows).

use super::{EventSink, Input, SessionPhase};
use crate::error::{CmdError, CmdResult};
use crate::prefs::TerminalPrefs;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use tokio::sync::mpsc;

/// Program and arguments for a local terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ShellCmd {
    program: String,
    args: Vec<String>,
}

/// Shells that accept `-l` to start as a login shell.
#[cfg(not(windows))]
const LOGIN_SHELLS: &[&str] = &["bash", "zsh", "fish", "sh", "dash", "ksh", "mksh", "yash"];

/// Picks the local shell: the configured program (Settings → Terminal → Local shell), else the
/// system default (`$SHELL` on Unix, Windows PowerShell on Windows).
#[cfg(not(windows))]
fn shell_cmd(configured: &str, env_shell: Option<String>) -> ShellCmd {
    let configured = configured.trim();
    let program = if configured.is_empty() {
        env_shell
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/bin/sh".into())
    } else {
        configured.to_owned()
    };
    let name = program.rsplit('/').next().unwrap_or(&program);
    let args = if LOGIN_SHELLS.contains(&name) {
        vec!["-l".to_owned()]
    } else {
        Vec::new()
    };
    ShellCmd { program, args }
}

#[cfg(windows)]
fn shell_cmd(configured: &str, system_root: Option<String>) -> ShellCmd {
    let configured = configured.trim();
    if !configured.is_empty() {
        return ShellCmd {
            program: configured.to_owned(),
            args: Vec::new(),
        };
    }
    let program = system_root
        .map(|root| format!("{root}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"))
        .unwrap_or_else(|| "powershell.exe".into());
    ShellCmd {
        program,
        args: vec!["-NoLogo".to_owned()],
    }
}

fn system_shell_env() -> Option<String> {
    #[cfg(windows)]
    {
        std::env::var("SystemRoot").ok()
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").ok()
    }
}

/// Loads the shell integration script into bash, zsh or fish (Settings → Terminal → Shell
/// integration) by rewriting the arguments and returning extra environment variables. Other
/// shells, and failures to write the startup files, leave the command unchanged.
fn with_integration(app: &tauri::AppHandle, shell: &mut ShellCmd) -> Vec<(String, String)> {
    use tauri::Manager;
    let Some(kind) = term_intel::scripts::Shell::from_path(&shell.program) else {
        return Vec::new();
    };
    let dir = app
        .state::<crate::state::AppState>()
        .data_dir
        .join("shell-integration");
    let zdotdir = std::env::var("ZDOTDIR").ok();
    let launch = term_intel::scripts::local_launch(kind, &dir, zdotdir.as_deref());
    let written = std::fs::create_dir_all(&dir).is_ok()
        && launch
            .files
            .iter()
            .all(|(path, body)| std::fs::write(path, body).is_ok());
    if !written {
        log::warn!(
            "could not write the shell integration files to {}",
            dir.display()
        );
        return Vec::new();
    }
    shell.args = launch.args;
    launch.env
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
    let prefs: TerminalPrefs = crate::prefs::load(app, crate::prefs::TERMINAL);
    let mut shell = shell_cmd(&prefs.local_shell, system_shell_env());
    let mut extra_env = Vec::new();
    if prefs.shell_integration {
        extra_env = with_integration(app, &mut shell);
    }
    let mut cmd = CommandBuilder::new(&shell.program);
    cmd.args(&shell.args);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", "vaultport");
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        cmd.cwd(home);
    }
    let mut child = pair.slave.spawn_command(cmd).map_err(|e| {
        CmdError::internal(format!("could not start the shell {}: {e}", shell.program))
    })?;
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

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn system_default_is_a_login_shell() {
        let c = shell_cmd("", Some("/usr/bin/zsh".into()));
        assert_eq!(c.program, "/usr/bin/zsh");
        assert_eq!(c.args, ["-l"]);
        assert_eq!(shell_cmd("  ", None).program, "/bin/sh");
        assert_eq!(shell_cmd("", Some(String::new())).program, "/bin/sh");
    }

    #[test]
    fn configured_shell_wins() {
        let c = shell_cmd(" /opt/homebrew/bin/fish ", Some("/bin/zsh".into()));
        assert_eq!(c.program, "/opt/homebrew/bin/fish");
        assert_eq!(c.args, ["-l"]);
        let c = shell_cmd("/usr/bin/nu", Some("/bin/zsh".into()));
        assert_eq!(c.program, "/usr/bin/nu");
        assert!(c.args.is_empty());
    }
}
