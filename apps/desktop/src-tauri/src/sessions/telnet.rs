// SPDX-License-Identifier: GPL-3.0-or-later
//! Telnet sessions (plaintext; the UI warns before enabling Telnet on a host).

use super::{EventSink, Input, SessionPhase};
use std::time::Duration;
use telnet_proto::{CloseReason, Event, Session, SessionConfig};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct TelnetTarget {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub term_type: String,
    pub connect_timeout: Duration,
}

pub fn spawn(
    app: &tauri::AppHandle,
    id: String,
    target: TelnetTarget,
    cols: u16,
    rows: u16,
    mut input: mpsc::UnboundedReceiver<Input>,
    sink: EventSink,
) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut config = SessionConfig {
            connect_timeout: target.connect_timeout,
            ..SessionConfig::default()
        };
        config.codec.terminal_type.clone_from(&target.term_type);
        config.codec.user.clone_from(&target.username);
        config.codec.window_size = (cols, rows);
        let addr = (target.host.as_str(), target.port);
        let (session, mut events) = match Session::connect(addr, config).await {
            Ok(s) => s,
            Err(e) => {
                sink.closed(Some(format!("could not connect: {e}")), true);
                super::finish(&app, &id);
                return;
            }
        };
        sink.status(SessionPhase::Connected);
        loop {
            tokio::select! {
                ev = events.recv() => match ev {
                    Some(Event::Output(data)) => sink.output(&data),
                    Some(Event::Status(_)) => {}
                    Some(Event::Closed { reason }) => {
                        let (msg, error) = match reason {
                            CloseReason::Io(e) => (Some(format!("network: {e}")), false),
                            _ => (None, false),
                        };
                        sink.closed(msg.map(|_| "network".to_owned()), error);
                        break;
                    }
                    None => {
                        sink.closed(None, false);
                        break;
                    }
                },
                msg = input.recv() => match msg {
                    Some(Input::Data(d)) => { let _ = session.write(&d).await; }
                    Some(Input::Resize { cols, rows }) => { let _ = session.resize(cols, rows).await; }
                    Some(Input::Close) | None => {
                        let _ = session.close().await;
                        sink.closed(None, false);
                        break;
                    }
                }
            }
        }
        super::finish(&app, &id);
    });
}
