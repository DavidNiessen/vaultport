// SPDX-License-Identifier: GPL-3.0-or-later
//! Establishing connections (proxy → jump hosts → target) and the [`Connection`] handle.

use crate::agent::ForwardAgent;
use crate::algorithms::{NegotiatedAlgorithms, preferred};
use crate::auth::{AuthContext, authenticate};
use crate::error::{CloseCategory, CloseReason, SshError};
use crate::handler::{ClientHandler, CloseSignal, HopState, Pauser, Shared};
use crate::interaction::Interaction;
use crate::known_hosts::{KnownHosts, has_cert_authority, known_algorithms};
use crate::options::{ConnectOptions, Hop};
use crate::proxy;
use russh::client::{self, Handle};
use std::borrow::Cow;
use std::sync::{Arc, Mutex};
use tokio::net::TcpStream;

/// One authenticated hop.
pub(crate) struct HopSession {
    pub handle: Handle<ClientHandler>,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub state: Arc<Mutex<HopState>>,
}

pub(crate) struct Inner {
    /// Jump hosts first, target last (never empty).
    pub hops: Vec<HopSession>,
    pub shared: Arc<Shared>,
    pub close: Arc<CloseSignal>,
}

impl Inner {
    pub(crate) fn target(&self) -> &HopSession {
        // `connect` guarantees at least one hop.
        &self.hops[self.hops.len() - 1]
    }
}

/// An authenticated SSH connection (possibly through jump hosts). Cheap to clone; all clones
/// refer to the same connection.
#[derive(Clone)]
pub struct Connection {
    pub(crate) inner: Arc<Inner>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("target", &self.inner.target().label)
            .field("hops", &self.inner.hops.len())
            .field("closed", &self.is_closed())
            .finish()
    }
}

/// Information about one hop of an established connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HopInfo {
    pub label: String,
    pub host: String,
    pub port: u16,
    pub host_key_fingerprint: Option<String>,
    /// Whether the host key was trusted through a `@cert-authority` (vs. a plain entry or
    /// an interactive decision).
    pub trusted_by_certificate: bool,
    pub algorithms: Option<NegotiatedAlgorithms>,
}

/// Establishes a connection: TCP (optionally via proxy) to the first hop, then every further
/// hop through a `direct-tcpip` channel of the previous one. Host keys of every hop are
/// verified and every hop is authenticated before the next is contacted.
pub async fn connect(
    opts: ConnectOptions,
    interaction: Arc<dyn Interaction>,
    known_hosts: Arc<dyn KnownHosts>,
) -> Result<Connection, SshError> {
    validate(&opts)?;
    let close = Arc::new(CloseSignal::new());
    let shared = Arc::new(Shared::default());
    let mut hops: Vec<HopSession> = Vec::with_capacity(opts.hops.len());
    let last = opts.hops.len() - 1;

    for (i, hop) in opts.hops.iter().enumerate() {
        let pauser = Arc::new(Pauser::default());
        let is_target = i == last;
        let result = pauser
            .timeout(
                opts.connect_timeout,
                connect_hop(
                    i,
                    hop,
                    &opts,
                    hops.last(),
                    &interaction,
                    &known_hosts,
                    &shared,
                    &close,
                    &pauser,
                    is_target,
                ),
            )
            .await;
        let session = match result {
            Some(Ok(s)) => s,
            Some(Err(e)) => {
                close_all(&hops).await;
                return Err(e);
            }
            None => {
                close_all(&hops).await;
                return Err(SshError::Timeout(opts.connect_timeout));
            }
        };
        hops.push(session);
    }

    Ok(Connection {
        inner: Arc::new(Inner {
            hops,
            shared,
            close,
        }),
    })
}

fn validate(opts: &ConnectOptions) -> Result<(), SshError> {
    if opts.hops.is_empty() {
        return Err(SshError::Config("at least one hop is required".into()));
    }
    for h in &opts.hops {
        if h.host.trim().is_empty() || h.host.chars().any(char::is_control) {
            return Err(SshError::Config("invalid host name".into()));
        }
        if h.port == 0 {
            return Err(SshError::Config(format!("invalid port for {}", h.host)));
        }
        if h.username.is_empty() || h.username.chars().any(char::is_control) {
            return Err(SshError::Config(format!("invalid username for {}", h.host)));
        }
    }
    Ok(())
}

async fn close_all(hops: &[HopSession]) {
    for h in hops.iter().rev() {
        let _ = h
            .handle
            .disconnect(russh::Disconnect::ByApplication, "", "en")
            .await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn connect_hop(
    index: usize,
    hop: &Hop,
    opts: &ConnectOptions,
    previous: Option<&HopSession>,
    interaction: &Arc<dyn Interaction>,
    known_hosts: &Arc<dyn KnownHosts>,
    shared: &Arc<Shared>,
    close: &Arc<CloseSignal>,
    pauser: &Arc<Pauser>,
    is_target: bool,
) -> Result<HopSession, SshError> {
    let label = hop.label();
    let entries = known_hosts.lookup(&hop.host, hop.port);
    let known = known_algorithms(&entries, &hop.host, hop.port);
    let want_certs = has_cert_authority(&entries, &hop.host, hop.port);
    let config = Arc::new(client::Config {
        client_id: russh::SshId::Standard(Cow::Owned(format!("SSH-2.0-{}", opts.client_id))),
        preferred: preferred(opts.algorithm_options(), &known, want_certs),
        keepalive_interval: opts.keepalive,
        keepalive_max: 3,
        inactivity_timeout: None,
        nodelay: true,
        ..client::Config::default()
    });
    let state = Arc::new(Mutex::new(HopState::default()));
    let handler = ClientHandler {
        hop_index: index,
        host: hop.host.clone(),
        port: hop.port,
        label: label.clone(),
        policy: hop.host_key_policy,
        entries,
        interaction: interaction.clone(),
        known_hosts: known_hosts.clone(),
        state: state.clone(),
        shared: shared.clone(),
        close: close.clone(),
        pauser: pauser.clone(),
        is_target,
    };

    let connected = match previous {
        None => {
            let stream = if let Some(p) = &opts.proxy {
                proxy::connect(p, &hop.host, hop.port)
                    .await
                    .map_err(|e| SshError::Proxy(e.to_string()))?
            } else {
                let s = TcpStream::connect((hop.host.as_str(), hop.port)).await?;
                let _ = s.set_nodelay(true);
                s
            };
            client::connect_stream(config, stream, handler).await
        }
        Some(prev) => {
            let channel = prev
                .handle
                .channel_open_direct_tcpip(hop.host.clone(), u32::from(hop.port), "127.0.0.1", 0)
                .await
                .map_err(|e| {
                    SshError::Refused(format!(
                        "{} could not open a tunnel to {}:{}: {e}",
                        prev.label, hop.host, hop.port
                    ))
                })?;
            client::connect_stream(config, channel.into_stream(), handler).await
        }
    };
    let mut handle = match connected {
        Ok(h) => h,
        Err(e) => {
            let rejection = state.lock().ok().and_then(|s| s.rejection.clone());
            return Err(match rejection {
                Some(reason) => SshError::HostKeyRejected {
                    host: hop.host.clone(),
                    port: hop.port,
                    reason,
                },
                None => SshError::Protocol(e),
            });
        }
    };

    let ctx = AuthContext {
        interaction,
        pauser,
        legacy: opts.legacy_algorithms,
        label: label.clone(),
    };
    let keys = authenticate(&mut handle, hop, &ctx).await?;
    if is_target {
        let agent = ForwardAgent {
            keys,
            interaction: interaction.clone(),
            confirm: opts.agent_confirm,
            allow_sha1: opts.legacy_algorithms,
            hop_label: label.clone(),
        };
        if let Ok(mut a) = shared.agent.lock() {
            *a = Some(Arc::new(agent));
        }
    }
    tracing::info!(hop = %label, "authenticated");
    Ok(HopSession {
        handle,
        label,
        host: hop.host.clone(),
        port: hop.port,
        state,
    })
}

impl Connection {
    pub(crate) fn handle(&self) -> &Handle<ClientHandler> {
        &self.inner.target().handle
    }

    /// Whether the connection (any hop) is closed.
    pub fn is_closed(&self) -> bool {
        self.inner.close.get().is_some() || self.inner.hops.iter().any(|h| h.handle.is_closed())
    }

    /// Why the connection closed, if it did.
    pub fn close_reason(&self) -> Option<CloseReason> {
        self.inner.close.get().or_else(|| {
            self.inner
                .hops
                .iter()
                .any(|h| h.handle.is_closed())
                .then(|| CloseReason::new(CloseCategory::Network, "session ended"))
        })
    }

    /// Resolves when the connection closes, with the first recorded reason (e.g. the jump
    /// host's network error rather than the resulting target disconnect).
    pub async fn closed(&self) -> CloseReason {
        let mut rx = self.inner.close.subscribe();
        loop {
            if let Some(r) = rx.borrow_and_update().clone() {
                return r;
            }
            if rx.changed().await.is_err() {
                return CloseReason::new(CloseCategory::Local, "connection dropped");
            }
        }
    }

    /// Closes all hops (target first). Idempotent.
    pub async fn close(&self) {
        self.inner
            .close
            .set(CloseReason::new(CloseCategory::Local, "closed by user"));
        close_all(&self.inner.hops).await;
    }

    /// Algorithms negotiated with the target.
    pub fn negotiated_algorithms(&self) -> Option<NegotiatedAlgorithms> {
        self.hop_info().last().and_then(|h| h.algorithms.clone())
    }

    /// Per-hop details (jump hosts first).
    pub fn hop_info(&self) -> Vec<HopInfo> {
        self.inner
            .hops
            .iter()
            .map(|h| {
                let s = h.state.lock().map(|s| s.clone()).unwrap_or_default();
                HopInfo {
                    label: h.label.clone(),
                    host: h.host.clone(),
                    port: h.port,
                    host_key_fingerprint: s.host_key_fingerprint,
                    trusted_by_certificate: matches!(
                        s.trusted_by,
                        Some(crate::known_hosts::TrustedBy::CertificateAuthority(_))
                    ),
                    algorithms: s.negotiated,
                }
            })
            .collect()
    }

    /// Label (`user@host:port`) of the target.
    pub fn label(&self) -> &str {
        &self.inner.target().label
    }

    /// Opens an SFTP session on the target.
    pub async fn sftp(&self) -> Result<russh_sftp::client::SftpSession, SshError> {
        self.sftp_with_config(russh_sftp::client::Config::default())
            .await
    }

    /// Opens an SFTP session with custom limits (packet size, requests in flight, timeout).
    pub async fn sftp_with_config(
        &self,
        cfg: russh_sftp::client::Config,
    ) -> Result<russh_sftp::client::SftpSession, SshError> {
        let channel = self.handle().channel_open_session().await?;
        channel.request_subsystem(true, "sftp").await?;
        Ok(russh_sftp::client::SftpSession::new_with_config(channel.into_stream(), cfg).await?)
    }

    /// Detects the remote operating system (`/etc/os-release` `ID`, else `uname -s`,
    /// lower-cased). Map it with [`core_model::OsKind::from_os_release_id`].
    pub async fn detect_os(&self) -> Option<String> {
        let out = self
            .exec("cat /etc/os-release 2>/dev/null; echo __SSHC_UNAME__; uname -s 2>/dev/null")
            .await
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(os) = parse_os(&text) {
            return Some(os);
        }
        let ver = self.exec("ver").await.ok()?;
        String::from_utf8_lossy(&ver.stdout)
            .contains("Windows")
            .then(|| "windows".to_owned())
    }

    /// [`Self::detect_os`] mapped to the model's [`core_model::OsKind`].
    pub async fn detect_os_kind(&self) -> core_model::OsKind {
        self.detect_os()
            .await
            .map_or(core_model::OsKind::Unknown, |s| {
                core_model::OsKind::from_os_release_id(&s)
            })
    }
}

fn parse_os(text: &str) -> Option<String> {
    let (release, uname) = text.split_once("__SSHC_UNAME__").unwrap_or((text, ""));
    for line in release.lines() {
        if let Some(id) = line.strip_prefix("ID=") {
            let id = id
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_ascii_lowercase();
            if !id.is_empty() {
                return Some(id);
            }
        }
    }
    let u = uname.trim().to_ascii_lowercase();
    (!u.is_empty()).then_some(u)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_parsing() {
        assert_eq!(
            parse_os("NAME=\"Alpine Linux\"\nID=alpine\n__SSHC_UNAME__\nLinux\n").as_deref(),
            Some("alpine")
        );
        assert_eq!(
            parse_os("__SSHC_UNAME__\nDarwin\n").as_deref(),
            Some("darwin")
        );
        assert_eq!(parse_os("__SSHC_UNAME__\n"), None);
    }

    #[test]
    fn validation() {
        let mut o = ConnectOptions::new(Hop::new("h", 22, "u"));
        assert!(validate(&o).is_ok());
        o.hops.clear();
        assert!(validate(&o).is_err());
        let o = ConnectOptions::new(Hop::new("h\n", 22, "u"));
        assert!(validate(&o).is_err());
        let o = ConnectOptions::new(Hop::new("h", 0, "u"));
        assert!(validate(&o).is_err());
    }
}
