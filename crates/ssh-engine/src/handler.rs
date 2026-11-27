// SPDX-License-Identifier: GPL-3.0-or-later
//! The russh [`russh::client::Handler`] used for every hop.

use crate::agent::ForwardAgent;
use crate::algorithms::NegotiatedAlgorithms;
use crate::error::{CloseCategory, CloseReason, HostKeyRejection, classify_russh};
use crate::interaction::{HostKeyDecision, Interaction};
use crate::known_hosts::{
    HostKeyStatus, KnownHosts, Presented, TrustedBy, Verdict, evaluate, host_key_check, new_entry,
};
use core_model::{HostKeyPolicy, KnownHost};
use russh::ChannelOpenFailure;
use russh::client::{ChannelOpenHandle, DisconnectReason, Msg, Session};
use russh::keys::PublicKeyOrCertificate;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::watch;

/// First-writer-wins close reason shared by all hops of a connection.
#[derive(Debug)]
pub(crate) struct CloseSignal {
    tx: watch::Sender<Option<CloseReason>>,
}

impl CloseSignal {
    pub(crate) fn new() -> Self {
        Self {
            tx: watch::channel(None).0,
        }
    }

    pub(crate) fn set(&self, reason: CloseReason) {
        self.tx.send_if_modified(|cur| {
            if cur.is_none() {
                *cur = Some(reason);
                true
            } else {
                false
            }
        });
    }

    pub(crate) fn get(&self) -> Option<CloseReason> {
        self.tx.borrow().clone()
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<Option<CloseReason>> {
        self.tx.subscribe()
    }
}

/// Tracks time spent waiting for the user so that it is excluded from timeouts.
#[derive(Debug, Default)]
pub(crate) struct Pauser {
    active: AtomicUsize,
    paused_ms: AtomicU64,
}

impl Pauser {
    pub(crate) async fn run<F: Future>(&self, f: F) -> F::Output {
        self.active.fetch_add(1, Ordering::SeqCst);
        let start = Instant::now();
        let out = f.await;
        self.paused_ms
            .fetch_add(start.elapsed().as_millis() as u64, Ordering::SeqCst);
        self.active.fetch_sub(1, Ordering::SeqCst);
        out
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst) > 0
    }

    fn paused(&self) -> Duration {
        Duration::from_millis(self.paused_ms.load(Ordering::SeqCst))
    }

    /// Runs `fut` with `timeout`, not counting time spent in [`Self::run`].
    pub(crate) async fn timeout<F: Future>(&self, timeout: Duration, fut: F) -> Option<F::Output> {
        tokio::pin!(fut);
        let start = Instant::now();
        loop {
            let deadline = start + timeout + self.paused();
            let wake = if self.is_active() {
                Instant::now() + Duration::from_millis(200)
            } else {
                deadline
            };
            tokio::select! {
                out = &mut fut => return Some(out),
                () = tokio::time::sleep_until(wake.into()) => {
                    if !self.is_active() && Instant::now() >= start + timeout + self.paused() {
                        return None;
                    }
                }
            }
        }
    }
}

/// State shared between a [`crate::Connection`] and the handler of its target hop.
#[derive(Default)]
pub(crate) struct Shared {
    pub agent: Mutex<Option<Arc<ForwardAgent>>>,
    /// Set once a session channel requested agent forwarding.
    pub agent_requested: AtomicBool,
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shared").finish_non_exhaustive()
    }
}

/// Result of host key checking for one hop.
#[derive(Debug, Clone, Default)]
pub(crate) struct HopState {
    pub rejection: Option<HostKeyRejection>,
    pub negotiated: Option<NegotiatedAlgorithms>,
    pub host_key_fingerprint: Option<String>,
    pub trusted_by: Option<TrustedBy>,
}

pub(crate) struct ClientHandler {
    pub hop_index: usize,
    pub host: String,
    pub port: u16,
    pub label: String,
    pub policy: HostKeyPolicy,
    pub entries: Vec<KnownHost>,
    pub interaction: Arc<dyn Interaction>,
    pub known_hosts: Arc<dyn KnownHosts>,
    pub state: Arc<Mutex<HopState>>,
    pub shared: Arc<Shared>,
    pub close: Arc<CloseSignal>,
    pub pauser: Arc<Pauser>,
    pub is_target: bool,
}

impl ClientHandler {
    fn set_state(&self, f: impl FnOnce(&mut HopState)) {
        if let Ok(mut s) = self.state.lock() {
            f(&mut s);
        }
    }

    async fn check_key(&self, presented: &Presented<'_>) -> bool {
        let key = presented.key_data().clone();
        let fp = crate::fingerprint::sha256_fingerprint(&key);
        self.set_state(|s| s.host_key_fingerprint = Some(fp.clone()));
        let verdict = evaluate(presented, &self.entries, &self.host, self.port);
        let (status, mismatched, problem) = match verdict {
            Verdict::Trusted(by) => {
                tracing::debug!(host = %self.host, %fp, "host key trusted");
                self.set_state(|s| s.trusted_by = Some(by));
                return true;
            }
            Verdict::Untrusted {
                status,
                mismatched,
                certificate_problem,
            } => (status, mismatched, certificate_problem),
        };
        let check = host_key_check(
            self.hop_index,
            &self.host,
            self.port,
            presented,
            status.clone(),
            problem,
        );
        let rejection = match (&status, self.policy) {
            (HostKeyStatus::Revoked, _) => {
                tracing::warn!(host = %self.host, %fp, "host key is revoked");
                self.pauser
                    .run(self.interaction.verify_host_key(&check))
                    .await;
                Some(HostKeyRejection::Revoked)
            }
            (HostKeyStatus::Unknown, HostKeyPolicy::Strict) => {
                Some(HostKeyRejection::UnknownStrict)
            }
            (HostKeyStatus::Changed { .. }, HostKeyPolicy::Strict) => {
                tracing::warn!(host = %self.host, %fp, "host key CHANGED (strict policy)");
                Some(HostKeyRejection::ChangedStrict)
            }
            (_, HostKeyPolicy::Ask) => {
                if matches!(status, HostKeyStatus::Changed { .. }) {
                    tracing::warn!(host = %self.host, %fp, "host key CHANGED");
                }
                match self
                    .pauser
                    .run(self.interaction.verify_host_key(&check))
                    .await
                {
                    HostKeyDecision::Reject => Some(HostKeyRejection::Declined),
                    HostKeyDecision::AcceptOnce => None,
                    HostKeyDecision::AcceptAndSave => {
                        if !mismatched.is_empty() {
                            self.known_hosts.forget(&mismatched);
                        }
                        self.known_hosts
                            .save(new_entry(&self.host, self.port, &key, None));
                        None
                    }
                }
            }
        };
        match rejection {
            Some(r) => {
                self.set_state(|s| s.rejection = Some(r));
                false
            }
            None => true,
        }
    }
}

impl russh::client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let ok = match server_public_key {
            PublicKeyOrCertificate::PublicKey { key, .. } => {
                self.check_key(&Presented::Key(key)).await
            }
            PublicKeyOrCertificate::Certificate(cert) => {
                self.check_key(&Presented::Certificate(cert)).await
            }
        };
        Ok(ok)
    }

    async fn auth_banner(
        &mut self,
        banner: &str,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.pauser
            .run(self.interaction.banner(&self.label, banner))
            .await;
        Ok(())
    }

    async fn kex_done(
        &mut self,
        _shared_secret: Option<&[u8]>,
        names: &russh::Names,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let n = NegotiatedAlgorithms::from_names(names);
        tracing::debug!(host = %self.host, kex = %n.kex, cipher = %n.cipher, "key exchange done");
        self.set_state(|s| s.negotiated = Some(n));
        Ok(())
    }

    async fn server_channel_open_agent_forward(
        &mut self,
        channel: russh::Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let agent = if self.is_target && self.shared.agent_requested.load(Ordering::SeqCst) {
            self.shared.agent.lock().ok().and_then(|a| a.clone())
        } else {
            None
        };
        if let Some(agent) = agent {
            reply.accept().await;
            tokio::spawn(async move { agent.serve(channel.into_stream()).await });
        } else {
            tracing::warn!("server opened an agent channel without agent forwarding");
            reply
                .reject(ChannelOpenFailure::AdministrativelyProhibited)
                .await;
        }
        Ok(())
    }

    // Everything else a server might try to open towards the client is refused
    // (forwarded-tcpip is refused by the default implementation).

    async fn server_channel_open_session(
        &mut self,
        _channel: russh::Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply
            .reject(ChannelOpenFailure::AdministrativelyProhibited)
            .await;
        Ok(())
    }

    async fn server_channel_open_direct_tcpip(
        &mut self,
        _channel: russh::Channel<Msg>,
        _host_to_connect: &str,
        _port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply
            .reject(ChannelOpenFailure::AdministrativelyProhibited)
            .await;
        Ok(())
    }

    async fn server_channel_open_direct_streamlocal(
        &mut self,
        _channel: russh::Channel<Msg>,
        _socket_path: &str,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply
            .reject(ChannelOpenFailure::AdministrativelyProhibited)
            .await;
        Ok(())
    }

    async fn server_channel_open_forwarded_streamlocal(
        &mut self,
        _channel: russh::Channel<Msg>,
        _socket_path: &str,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply
            .reject(ChannelOpenFailure::AdministrativelyProhibited)
            .await;
        Ok(())
    }

    async fn server_channel_open_x11(
        &mut self,
        _channel: russh::Channel<Msg>,
        _originator_address: &str,
        _originator_port: u32,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply
            .reject(ChannelOpenFailure::AdministrativelyProhibited)
            .await;
        Ok(())
    }

    async fn disconnected(
        &mut self,
        reason: DisconnectReason<Self::Error>,
    ) -> Result<(), Self::Error> {
        match reason {
            DisconnectReason::ReceivedDisconnect(info) => {
                self.close.set(CloseReason::new(
                    CloseCategory::Remote,
                    format!("{} closed the connection: {}", self.label, info.message),
                ));
                Ok(())
            }
            DisconnectReason::Error(e) => {
                self.close.set(CloseReason::new(
                    classify_russh(&e),
                    format!("{}: {e}", self.label),
                ));
                Err(e)
            }
        }
    }
}

impl Drop for ClientHandler {
    fn drop(&mut self) {
        self.close.set(CloseReason::new(
            CloseCategory::Network,
            format!("{}: session ended", self.label),
        ));
    }
}
