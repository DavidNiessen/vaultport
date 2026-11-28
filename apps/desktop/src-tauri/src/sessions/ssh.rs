// SPDX-License-Identifier: GPL-3.0-or-later
//! SSH and Mosh sessions on top of `ssh-engine` and `mosh-proto`.

use super::record::Recorder;
use super::resolve::SshPlan;
use super::{EventSink, Input, SessionPhase};
use crate::error::CmdResult;
use crate::net::{self, Asker};
use crate::prefs::TerminalPrefs;
use ssh_engine::{CloseCategory, ShellEvent, ShellRequest};
use tauri::Manager;
use term_intel::scripts::ENABLE_VAR;
use tokio::sync::mpsc;

pub fn spawn(
    app: &tauri::AppHandle,
    id: String,
    plan: SshPlan,
    cols: u16,
    rows: u16,
    input: mpsc::UnboundedReceiver<Input>,
    sink: EventSink,
) -> CmdResult<()> {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        run(&app, &id, plan, cols, rows, input, &sink).await;
        super::finish(&app, &id);
    });
    Ok(())
}

async fn run(
    app: &tauri::AppHandle,
    id: &str,
    plan: SshPlan,
    cols: u16,
    rows: u16,
    mut input: mpsc::UnboundedReceiver<Input>,
    sink: &EventSink,
) {
    sink.status(SessionPhase::Connecting);
    let conn = match net::connect(app, &plan, Asker::Session(sink.clone())).await {
        Ok(c) => c,
        Err(e) => {
            sink.closed(Some(e.message), true);
            return;
        }
    };
    touch_host(app, &plan);
    if let Some(cmd) = plan.mosh.clone() {
        run_mosh(app, &plan, &conn, &cmd, cols, rows, input, sink).await;
        return;
    }
    let prefs: TerminalPrefs = crate::prefs::load(app, crate::prefs::TERMINAL);
    let request = ShellRequest {
        term: plan.term_type.clone(),
        cols: u32::from(cols),
        rows: u32::from(rows),
        env: shell_env(&plan.env, prefs.shell_integration),
        agent_forwarding: plan.agent_forwarding,
        command: None,
    };
    let (shell, mut events) = match conn.open_shell(request).await {
        Ok(s) => s,
        Err(e) => {
            sink.closed(Some(e.to_string()), true);
            conn.close().await;
            return;
        }
    };
    sink.status(SessionPhase::Connected);
    super::probe::spawn(app, &conn, plan.host_id, &prefs);
    if prefs.autocomplete {
        attach_files(app, id, &conn);
    }
    let mut rec = Recorder::start(app, plan.host_id, &plan.label, cols, rows);
    if let Some(script) = &plan.startup_script {
        let mut s = script.clone();
        if !s.ends_with('\n') {
            s.push('\n');
        }
        let _ = shell.write(s.as_bytes()).await;
    }
    let closed = loop {
        tokio::select! {
            ev = events.recv() => match ev {
                Some(ShellEvent::Data(d) | ShellEvent::ExtendedData { data: d, .. }) => {
                    rec.output(&d);
                    sink.output(&d);
                }
                Some(ShellEvent::ExitStatus(code)) => {
                    if code != 0 {
                        sink.output(format!("\r\n[exit status {code}]\r\n").as_bytes());
                    }
                }
                Some(ShellEvent::Closed | ShellEvent::Eof) | None => break None,
                Some(_) => {}
            },
            msg = input.recv() => match msg {
                Some(Input::Data(d)) => { let _ = shell.write(&d).await; }
                Some(Input::Resize { cols, rows }) => {
                    rec.resize(cols, rows);
                    let _ = shell.resize(u32::from(cols), u32::from(rows)).await;
                }
                Some(Input::Close) | None => {
                    let _ = shell.close().await;
                    break Some(());
                }
            },
            reason = conn.closed() => {
                let network = reason.category == CloseCategory::Network;
                sink.closed(network.then(|| "network".to_owned()), false);
                rec.finish();
                return;
            }
        }
    };
    let _ = closed;
    rec.finish();
    conn.close().await;
    sink.closed(None, false);
}

/// Opens an SFTP channel on the terminal's own connection so autocomplete can list remote
/// directories and find commands without a second login. Servers without SFTP just get no
/// remote suggestions.
fn attach_files(app: &tauri::AppHandle, id: &str, conn: &ssh_engine::Connection) {
    let (app, id, conn) = (app.clone(), id.to_owned(), conn.clone());
    tauri::async_runtime::spawn(async move {
        match conn.sftp().await {
            Ok(sftp) => {
                let completions = app.state::<crate::commands::complete::CompletionState>();
                let fs = std::sync::Arc::new(sftp_engine::Remote::new(sftp));
                completions.attach(&id, fs);
                // The session may have ended (and been detached) while the channel opened:
                // check after attaching, so a late attach is undone either way.
                if app.state::<super::SessionManager>().get(&id).is_none() {
                    completions.detach(&id);
                }
            }
            Err(e) => log::debug!("no SFTP for autocomplete: {e}"),
        }
    });
}

/// Environment for the shell channel: the host's variables, plus the switch that activates
/// the shell integration scripts (when the server accepts it, see `AcceptEnv`).
fn shell_env(host_env: &[(String, String)], shell_integration: bool) -> Vec<(String, String)> {
    let mut env = host_env.to_vec();
    if shell_integration && !env.iter().any(|(k, _)| k == ENABLE_VAR) {
        env.push((ENABLE_VAR.to_owned(), "1".to_owned()));
    }
    env
}

#[allow(clippy::too_many_arguments)]
async fn run_mosh(
    app: &tauri::AppHandle,
    plan: &SshPlan,
    conn: &ssh_engine::Connection,
    server_command: &str,
    cols: u16,
    rows: u16,
    mut input: mpsc::UnboundedReceiver<Input>,
    sink: &EventSink,
) {
    let out = match conn.exec(server_command).await {
        Ok(o) => o,
        Err(e) => {
            sink.closed(Some(format!("could not start mosh-server: {e}")), true);
            return;
        }
    };
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let Some((port, key)) = mosh_proto::find_mosh_connect(&text) else {
        sink.closed(
            Some("mosh-server did not start (is mosh installed on the host?)".into()),
            true,
        );
        return;
    };
    let Some(target) = plan.hops.last() else {
        return;
    };
    let addr = match tokio::net::lookup_host((target.host.as_str(), port)).await {
        Ok(mut a) => a.next(),
        Err(_) => None,
    };
    let Some(addr) = addr else {
        sink.closed(
            Some("could not resolve the Mosh server address".into()),
            true,
        );
        return;
    };
    // The SSH connection is only needed for bootstrapping.
    conn.close().await;
    let (session, mut events) = match mosh_proto::Session::connect_with(
        addr,
        &key,
        cols,
        rows,
        mosh_proto::SessionConfig::default(),
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            sink.closed(Some(format!("mosh: {e}")), true);
            return;
        }
    };
    sink.status(SessionPhase::Connected);
    let _ = app.state::<crate::state::AppState>();
    loop {
        tokio::select! {
            ev = events.recv() => match ev {
                Some(mosh_proto::Event::Output(d)) => sink.output(&d),
                Some(mosh_proto::Event::Status(_)) => {}
                Some(mosh_proto::Event::Closed { .. }) | None => break,
            },
            msg = input.recv() => match msg {
                Some(Input::Data(d)) => { let _ = session.write(&d).await; }
                Some(Input::Resize { cols, rows }) => { let _ = session.resize(cols, rows).await; }
                Some(Input::Close) | None => {
                    let _ = session.close().await;
                    break;
                }
            }
        }
    }
    sink.closed(None, false);
}

fn touch_host(app: &tauri::AppHandle, plan: &SshPlan) {
    let Some(hid) = plan.host_id else { return };
    let state = app.state::<crate::state::AppState>();
    let _ = state.with_vault(|v| {
        if let Some(mut h) = v.get::<core_model::Host>(&hid.to_string())? {
            h.last_connected_at = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis() as i64),
            );
            v.put(&h)?;
        }
        Ok(())
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_integration_adds_the_switch_once() {
        let host = vec![("LANG".to_owned(), "C.UTF-8".to_owned())];
        assert_eq!(shell_env(&host, false), host);
        let env = shell_env(&host, true);
        assert_eq!(env.len(), 2);
        assert_eq!(env[1], (ENABLE_VAR.to_owned(), "1".to_owned()));
        assert_eq!(shell_env(&env, true), env);
    }
}
