// SPDX-License-Identifier: GPL-3.0-or-later
//! Turns a saved host (or quick-connect target) into a concrete connection plan by resolving
//! group inheritance, identities, keys and jump hosts from the vault.

use super::telnet::TelnetTarget;
use crate::error::{CmdError, CmdResult, ErrorCode};
use crate::state::AppState;
use core_model::{
    Credential, Defaults, Group, Host, HostConfig, HostId, HostKeyPolicy, Identity, KeyId,
    ProxyConfig, Resolver, SecretRef, Snippet, Validate, validate_hostname,
};
use std::time::Duration;
use tauri::Manager;

/// Credentials and address of one SSH hop.
#[derive(Debug, Clone)]
pub struct HopPlan {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<SecretRef>,
    pub keys: Vec<KeyId>,
    pub host_key_policy: HostKeyPolicy,
}

/// Everything needed to open an SSH (or Mosh) session.
#[derive(Debug, Clone)]
pub struct SshPlan {
    pub host_id: Option<HostId>,
    pub label: String,
    /// Jump hosts first, target last.
    pub hops: Vec<HopPlan>,
    pub proxy: Option<ProxyConfig>,
    pub agent_forwarding: bool,
    pub agent_confirm: bool,
    pub env: Vec<(String, String)>,
    pub startup_script: Option<String>,
    pub keepalive: Option<Duration>,
    pub connect_timeout: Duration,
    pub legacy_algorithms: bool,
    pub post_quantum: bool,
    pub compression: bool,
    pub term_type: String,
    pub mosh: Option<String>,
}

#[derive(Debug)]
pub enum Plan {
    Ssh(Box<SshPlan>),
    Telnet(TelnetTarget),
}

/// Snapshot of the vault entities needed to plan connections.
pub struct Snapshot {
    pub hosts: Vec<Host>,
    pub groups: Vec<Group>,
    pub identities: Vec<Identity>,
    pub snippets: Vec<Snippet>,
}

pub fn snapshot(app: &tauri::AppHandle) -> CmdResult<Snapshot> {
    let state = app.state::<AppState>();
    state.with_vault(|v| {
        Ok(Snapshot {
            hosts: v.list()?,
            groups: v.list()?,
            identities: v.list()?,
            snippets: v.list()?,
        })
    })
}

fn local_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "root".to_owned())
}

fn hop_for(snap: &Snapshot, host: &Host, cfg: &HostConfig) -> HopPlan {
    let (username, password, keys) = match &cfg.ssh.credential {
        Some(Credential::Identity { id }) => snap
            .identities
            .iter()
            .find(|i| &i.id == id)
            .map_or((None, None, Vec::new()), |i| {
                (Some(i.username.clone()), i.password, i.keys.clone())
            }),
        Some(Credential::Inline {
            username,
            password,
            keys,
        }) => (username.clone(), *password, keys.clone()),
        None => (None, None, Vec::new()),
    };
    HopPlan {
        host: host.address.clone(),
        port: cfg.ssh.port.unwrap_or(Defaults::SSH_PORT),
        username: username
            .filter(|u| !u.is_empty())
            .unwrap_or_else(local_username),
        password,
        keys,
        host_key_policy: cfg.ssh.host_key_policy.unwrap_or_default(),
    }
}

fn ssh_plan(snap: &Snapshot, target: &Host) -> CmdResult<SshPlan> {
    let r = Resolver::new(&snap.hosts, &snap.groups);
    let resolved = r.resolve(target.id)?;
    let cfg = resolved.config;
    let mut hops = Vec::new();
    for hop_id in r.jump_chain(target.id)? {
        let hop = snap
            .hosts
            .iter()
            .find(|h| h.id == hop_id)
            .ok_or_else(|| CmdError::not_found("jump host not found"))?;
        let hop_cfg = r.resolve(hop_id)?.config;
        hops.push(hop_for(snap, hop, &hop_cfg));
    }
    hops.push(hop_for(snap, target, &cfg));
    let startup_script = cfg
        .ssh
        .startup_snippet
        .and_then(|sid| snap.snippets.iter().find(|s| s.id == sid))
        .map(|s| s.render(&[]));
    let keepalive = cfg.ssh.keepalive_secs.filter(|s| *s > 0);
    Ok(SshPlan {
        host_id: Some(target.id),
        label: target.label.clone(),
        hops,
        proxy: cfg.ssh.proxy.clone(),
        agent_forwarding: cfg.ssh.agent_forwarding.unwrap_or(false),
        agent_confirm: cfg.ssh.agent_confirm.unwrap_or(true),
        env: cfg
            .ssh
            .env
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|e| (e.name, e.value))
            .collect(),
        startup_script,
        keepalive: keepalive.map(|s| Duration::from_secs(u64::from(s))),
        connect_timeout: Duration::from_secs(u64::from(
            cfg.ssh
                .connect_timeout_secs
                .unwrap_or(Defaults::CONNECT_TIMEOUT_SECS),
        )),
        legacy_algorithms: cfg.ssh.legacy_algorithms.unwrap_or(false),
        post_quantum: cfg.ssh.post_quantum.unwrap_or(true),
        compression: cfg.ssh.compression.unwrap_or(false),
        term_type: cfg
            .terminal
            .term_type
            .clone()
            .unwrap_or_else(|| Defaults::TERM_TYPE.to_owned()),
        mosh: cfg.mosh.enabled.unwrap_or(false).then(|| {
            cfg.mosh
                .server_command
                .clone()
                .unwrap_or_else(|| Defaults::MOSH_SERVER.to_owned())
        }),
    })
}

/// Plans a connection to a saved host.
pub fn plan_for_host(app: &tauri::AppHandle, host_id: &str) -> CmdResult<Plan> {
    let id: HostId = host_id
        .parse()
        .map_err(|_| CmdError::invalid("invalid host id"))?;
    let snap = snapshot(app)?;
    let host = snap
        .hosts
        .iter()
        .find(|h| h.id == id)
        .ok_or_else(|| CmdError::not_found("host not found"))?
        .clone();
    host.validate()?;
    let r = Resolver::new(&snap.hosts, &snap.groups);
    let cfg = r.resolve(id)?.config;
    let telnet_only = cfg.telnet.enabled.unwrap_or(false) && !cfg.ssh.enabled.unwrap_or(true);
    if telnet_only {
        return Ok(Plan::Telnet(TelnetTarget {
            host: host.address.clone(),
            port: cfg.telnet.port.unwrap_or(Defaults::TELNET_PORT),
            username: cfg.telnet.username.clone(),
            term_type: cfg
                .terminal
                .term_type
                .clone()
                .unwrap_or_else(|| Defaults::TERM_TYPE.to_owned()),
            connect_timeout: Duration::from_secs(u64::from(Defaults::CONNECT_TIMEOUT_SECS)),
        }));
    }
    Ok(Plan::Ssh(Box::new(ssh_plan(&snap, &host)?)))
}

/// Plans an ad-hoc connection typed into the omnibox. Matching saved hosts are not used, so the
/// user gets exactly what they typed (secure defaults, host key verification still applies).
pub fn plan_for_quick(
    _app: &tauri::AppHandle,
    host: String,
    port: Option<u16>,
    username: Option<String>,
) -> CmdResult<Plan> {
    validate_hostname(&host)?;
    if port == Some(0) {
        return Err(CmdError::invalid("invalid port"));
    }
    Ok(Plan::Ssh(Box::new(SshPlan {
        host_id: None,
        label: host.clone(),
        hops: vec![HopPlan {
            host,
            port: port.unwrap_or(Defaults::SSH_PORT),
            username: username
                .filter(|u| !u.is_empty())
                .unwrap_or_else(local_username),
            password: None,
            keys: Vec::new(),
            host_key_policy: HostKeyPolicy::Ask,
        }],
        proxy: None,
        agent_forwarding: false,
        agent_confirm: true,
        env: Vec::new(),
        startup_script: None,
        keepalive: Some(Duration::from_secs(u64::from(Defaults::KEEPALIVE_SECS))),
        connect_timeout: Duration::from_secs(u64::from(Defaults::CONNECT_TIMEOUT_SECS)),
        legacy_algorithms: false,
        post_quantum: true,
        compression: false,
        term_type: Defaults::TERM_TYPE.to_owned(),
        mosh: None,
    })))
}

#[allow(dead_code, reason = "used once the SSH engine is wired")]
pub fn unsupported(what: &str) -> CmdError {
    CmdError::new(
        ErrorCode::Unsupported,
        format!("{what} is not available in this build"),
    )
}
