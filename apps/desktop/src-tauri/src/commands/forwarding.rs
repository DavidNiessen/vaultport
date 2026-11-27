// SPDX-License-Identifier: GPL-3.0-or-later
//! Port forwarding rule activation (local, remote and dynamic/SOCKS tunnels).

use crate::error::{CmdError, CmdResult};
use crate::net::ConnectionPool;
use crate::state::AppState;
use core_model::{ForwardKind, PortForwardRule};
use serde::Serialize;
use specta::Type;
use ssh_engine::ForwardHandle;
use std::collections::HashMap;
use std::net::SocketAddr;
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ForwardStatus {
    pub rule_id: String,
    pub connections: u32,
    pub local_addr: String,
}

/// Running tunnels by rule id.
#[derive(Default)]
pub struct ForwardManager {
    active: tokio::sync::Mutex<HashMap<String, ForwardHandle>>,
}

impl ForwardManager {
    /// Number of running tunnels (0 while the map is busy).
    pub fn count(&self) -> usize {
        self.active.try_lock().map_or(0, |a| a.len())
    }

    pub async fn stop_all(&self) {
        let mut active = self.active.lock().await;
        for (_, h) in active.drain() {
            h.stop().await;
        }
    }
}

fn status(rule_id: &str, h: &ForwardHandle) -> ForwardStatus {
    ForwardStatus {
        rule_id: rule_id.to_owned(),
        connections: u32::try_from(h.stats().connections_active).unwrap_or(u32::MAX),
        local_addr: h
            .local_addr()
            .map(|a| a.to_string())
            .or_else(|| h.remote_bind().map(|(host, port)| format!("{host}:{port}")))
            .unwrap_or_default(),
    }
}

/// Active tunnels.
#[tauri::command]
#[specta::specta]
pub async fn forward_status(manager: State<'_, ForwardManager>) -> CmdResult<Vec<ForwardStatus>> {
    let mut active = manager.active.lock().await;
    active.retain(|_, h| h.is_running());
    Ok(active.iter().map(|(id, h)| status(id, h)).collect())
}

fn bind(rule: &PortForwardRule) -> CmdResult<SocketAddr> {
    let host = if rule.bind_address == "localhost" {
        "127.0.0.1"
    } else {
        rule.bind_address.as_str()
    };
    let ip: std::net::IpAddr = host
        .parse()
        .map_err(|_| CmdError::invalid("the local bind address must be an IP address"))?;
    Ok(SocketAddr::new(ip, rule.bind_port))
}

/// Starts a saved rule.
#[tauri::command]
#[specta::specta]
pub async fn forward_start(
    app: AppHandle,
    state: State<'_, AppState>,
    manager: State<'_, ForwardManager>,
    rule_id: String,
) -> CmdResult<ForwardStatus> {
    let rule = state.with_vault(|v| {
        v.get::<PortForwardRule>(&rule_id)?
            .ok_or_else(|| CmdError::not_found("rule not found"))
    })?;
    if let Some(h) = manager.active.lock().await.get(&rule_id)
        && h.is_running()
    {
        return Ok(status(&rule_id, h));
    }
    let pool = app.state::<ConnectionPool>();
    let conn = pool.get(&app, &rule.host.to_string()).await?;
    let dest_host = rule.dest_host.clone().unwrap_or_else(|| "localhost".into());
    let dest_port = rule.dest_port.unwrap_or(0);
    let handle = match rule.kind {
        ForwardKind::Local => conn.forward_local(bind(&rule)?, dest_host, dest_port).await,
        ForwardKind::Remote => {
            conn.forward_remote(
                rule.bind_address.clone(),
                rule.bind_port,
                dest_host,
                dest_port,
            )
            .await
        }
        ForwardKind::Dynamic => conn.forward_dynamic(bind(&rule)?).await,
    }
    .map_err(crate::net::map_ssh)?;
    let st = status(&rule_id, &handle);
    manager.active.lock().await.insert(rule_id, handle);
    Ok(st)
}

/// Stops a running rule.
#[tauri::command]
#[specta::specta]
pub async fn forward_stop(manager: State<'_, ForwardManager>, rule_id: String) -> CmdResult<()> {
    let handle = manager.active.lock().await.remove(&rule_id);
    if let Some(h) = handle {
        h.stop().await;
    }
    Ok(())
}

/// Starts every rule marked "start automatically" (called after unlocking).
pub async fn autostart(app: &AppHandle) {
    let state = app.state::<AppState>();
    let rules: Vec<PortForwardRule> = state
        .with_vault(|v| Ok(v.list::<PortForwardRule>()?))
        .unwrap_or_default();
    for r in rules.into_iter().filter(|r| r.auto_start) {
        let _ = forward_start(
            app.clone(),
            app.state::<AppState>(),
            app.state::<ForwardManager>(),
            r.id.to_string(),
        )
        .await;
    }
}
