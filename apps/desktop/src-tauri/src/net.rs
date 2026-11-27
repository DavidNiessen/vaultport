// SPDX-License-Identifier: GPL-3.0-or-later
//! Glue between the vault, the UI and the SSH engine: host-key verification against Known
//! Hosts, interactive prompts, and turning a connection plan into engine options with
//! credentials loaded from the vault (decrypted only for the duration of the handshake).

use crate::commands::entities::{VAULT_CHANGED, VaultChanged};
use crate::error::{CmdError, CmdResult, ErrorCode};
use crate::sessions::resolve::{HopPlan, SshPlan};
use crate::sessions::{
    EventSink, HostKeyStatus as UiHostKeyStatus, PromptLine, PromptResponse, SessionEvent,
};
use crate::state::AppState;
use async_trait::async_trait;
use core_model::{KeyMeta, KeyStorage, KnownHost, SecretRef};
use parking_lot::Mutex;
use serde::Serialize;
use specta::Type;
use ssh_engine::{
    AuthMethod, ConnectOptions, Connection, ExternalSigner, Hop, HostKeyCheck, HostKeyDecision,
    HostKeyStatus, Interaction, KnownHosts, Prompt, Proxy, SignerError, Zeroizing,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;

/// Event carrying a prompt that is not tied to a terminal tab (SFTP, port forwarding).
pub const PROMPT_EVENT: &str = "prompt://request";

/// Global prompts for background connections.
#[derive(Default)]
pub struct PromptBroker {
    pending: Mutex<HashMap<String, oneshot::Sender<PromptResponse>>>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GlobalPrompt {
    pub event: SessionEvent,
}

impl PromptBroker {
    pub fn respond(&self, id: &str, response: PromptResponse) -> CmdResult<()> {
        let tx = self
            .pending
            .lock()
            .remove(id)
            .ok_or_else(|| CmdError::not_found("prompt expired"))?;
        let _ = tx.send(response);
        Ok(())
    }
}

/// Where prompts are shown.
#[derive(Clone)]
pub enum Asker {
    Session(EventSink),
    Global(AppHandle),
}

impl Asker {
    pub async fn ask(&self, make: impl FnOnce(String) -> SessionEvent) -> Option<PromptResponse> {
        match self {
            Self::Session(sink) => sink.ask(make).await,
            Self::Global(app) => {
                let broker = app.state::<PromptBroker>();
                let id = uuid::Uuid::now_v7().to_string();
                let (tx, rx) = oneshot::channel();
                broker.pending.lock().insert(id.clone(), tx);
                let _ = app.emit(
                    PROMPT_EVENT,
                    GlobalPrompt {
                        event: make(id.clone()),
                    },
                );
                let answer = tokio::time::timeout(Duration::from_secs(600), rx).await;
                broker.pending.lock().remove(&id);
                answer.ok().and_then(Result::ok)
            }
        }
    }

    fn banner(&self, message: String) {
        if let Self::Session(sink) = self {
            sink.event(SessionEvent::Banner { message });
        }
    }
}

/// [`Interaction`] implementation that asks the user through the UI.
pub struct UiInteraction {
    pub asker: Asker,
}

fn take_answers(resp: Option<PromptResponse>) -> Option<Vec<Zeroizing<String>>> {
    match resp {
        Some(mut r) => match &mut r {
            PromptResponse::Auth { answers } => answers
                .take()
                .map(|a| a.into_iter().map(Zeroizing::new).collect()),
            PromptResponse::HostKey { .. } => None,
        },
        None => None,
    }
}

impl UiInteraction {
    async fn secret_prompt(
        &self,
        target: &str,
        name: &str,
        text: &str,
    ) -> Option<Zeroizing<String>> {
        let target = target.to_owned();
        let name = name.to_owned();
        let text = text.to_owned();
        let resp = self
            .asker
            .ask(move |prompt_id| SessionEvent::Auth {
                prompt_id,
                target,
                name,
                instructions: String::new(),
                prompts: vec![PromptLine { text, echo: false }],
            })
            .await;
        take_answers(resp).and_then(|mut a| (!a.is_empty()).then(|| a.swap_remove(0)))
    }
}

#[async_trait]
impl Interaction for UiInteraction {
    async fn verify_host_key(&self, check: &HostKeyCheck) -> HostKeyDecision {
        let (status, previous) = match &check.status {
            HostKeyStatus::Changed {
                previous_fingerprints,
            } => (UiHostKeyStatus::Changed, previous_fingerprints.clone()),
            HostKeyStatus::Revoked => (UiHostKeyStatus::Revoked, Vec::new()),
            // `Unknown` and any future status.
            _ => (UiHostKeyStatus::Unknown, Vec::new()),
        };
        let c = check.clone();
        let resp = self
            .asker
            .ask(move |prompt_id| SessionEvent::HostKey {
                prompt_id,
                host: c.host,
                port: c.port,
                key_type: c.key_type,
                fingerprint: c.fingerprint,
                randomart: Some(c.randomart),
                status,
                previous_fingerprints: previous,
            })
            .await;
        match resp {
            Some(PromptResponse::HostKey {
                accept: true,
                remember: true,
            }) => HostKeyDecision::AcceptAndSave,
            Some(PromptResponse::HostKey {
                accept: true,
                remember: false,
            }) => HostKeyDecision::AcceptOnce,
            _ => HostKeyDecision::Reject,
        }
    }

    async fn keyboard_interactive(
        &self,
        hop: &str,
        name: &str,
        instructions: &str,
        prompts: &[Prompt],
    ) -> Option<Vec<Zeroizing<String>>> {
        let target = hop.to_owned();
        let name = name.to_owned();
        let instructions = instructions.to_owned();
        let prompts: Vec<PromptLine> = prompts
            .iter()
            .map(|p| PromptLine {
                text: p.text.clone(),
                echo: p.echo,
            })
            .collect();
        let resp = self
            .asker
            .ask(move |prompt_id| SessionEvent::Auth {
                prompt_id,
                target,
                name,
                instructions,
                prompts,
            })
            .await;
        take_answers(resp)
    }

    async fn password(&self, hop: &str, username: &str) -> Option<Zeroizing<String>> {
        self.secret_prompt(hop, "", &format!("Password for {username}"))
            .await
    }

    async fn key_passphrase(&self, hop: &str, key: &str) -> Option<Zeroizing<String>> {
        self.secret_prompt(hop, "", &format!("Passphrase for key {key}"))
            .await
    }

    async fn banner(&self, _hop: &str, message: &str) {
        self.asker.banner(message.to_owned());
    }

    async fn confirm_agent_use(&self, request: &ssh_engine::AgentSignRequest) -> bool {
        let target = request.hop.clone();
        let text = format!(
            "Allow {} to use key {} ({})? Type yes to allow.",
            request.hop, request.key_comment, request.key_fingerprint
        );
        let resp = self
            .asker
            .ask(move |prompt_id| SessionEvent::Auth {
                prompt_id,
                target,
                name: "Agent forwarding".into(),
                instructions: String::new(),
                prompts: vec![PromptLine { text, echo: true }],
            })
            .await;
        take_answers(resp)
            .and_then(|a| a.into_iter().next())
            .is_some_and(|s| s.trim().eq_ignore_ascii_case("yes"))
    }
}

/// Known hosts backed by the vault.
pub struct VaultKnownHosts {
    pub app: AppHandle,
}

impl KnownHosts for VaultKnownHosts {
    fn lookup(&self, _host: &str, _port: u16) -> Vec<KnownHost> {
        let state = self.app.state::<AppState>();
        state
            .with_vault(|v| Ok(v.list::<KnownHost>()?))
            .unwrap_or_default()
    }

    fn save(&self, entry: KnownHost) {
        let state = self.app.state::<AppState>();
        if state.with_vault(|v| Ok(v.put(&entry)?)).is_ok() {
            let _ = self.app.emit(
                VAULT_CHANGED,
                VaultChanged {
                    kind: "knownHost".into(),
                },
            );
        }
    }

    fn forget(&self, previous: &[KnownHost]) {
        let state = self.app.state::<AppState>();
        let _ = state.with_vault(|v| {
            for p in previous {
                v.delete::<KnownHost>(&p.id.to_string())?;
            }
            Ok(())
        });
    }
}

/// Adapts a hardware signer (FIDO2, Secure Enclave, Windows Hello) to the engine.
struct HwSigner(Arc<dyn hw_keys::HardwareSigner>);

#[async_trait]
impl ExternalSigner for HwSigner {
    fn public_key(&self) -> ssh_engine::ssh_key::PublicKey {
        self.0.public_key()
    }

    async fn sign(
        &self,
        data: &[u8],
        _hash: Option<ssh_engine::ssh_key::HashAlg>,
    ) -> Result<Vec<u8>, SignerError> {
        self.0
            .sign(data)
            .await
            .map_err(|e| SignerError(e.to_string()))
    }
}

/// PIN prompts for security keys go through the same UI prompt channel.
struct UiPin {
    asker: Asker,
}

impl std::fmt::Debug for UiPin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UiPin")
    }
}

#[async_trait]
impl hw_keys::PinProvider for UiPin {
    async fn pin(&self) -> Option<Zeroizing<String>> {
        UiInteraction {
            asker: self.asker.clone(),
        }
        .secret_prompt("Security key", "Security key", "PIN")
        .await
    }

    fn touch_prompt(&self) {
        self.asker
            .banner("Touch your security key to continue…".to_owned());
    }
}

fn secret_string(v: &vault::Vault, r: SecretRef) -> CmdResult<Zeroizing<String>> {
    let bytes = v.get_secret(r)?;
    Ok(Zeroizing::new(String::from_utf8_lossy(&bytes).into_owned()))
}

fn auth_for_key(v: &vault::Vault, meta: &KeyMeta, asker: &Asker) -> CmdResult<Option<AuthMethod>> {
    Ok(match &meta.storage {
        KeyStorage::Vault { secret } => Some(AuthMethod::PrivateKey {
            key_pem: secret_string(v, *secret)?,
            passphrase: match meta.passphrase {
                Some(p) => Some(secret_string(v, p)?),
                None => None,
            },
            certificate: meta.certificate.clone(),
        }),
        KeyStorage::Fido2 {
            secret,
            require_pin,
            require_presence,
            ..
        } => {
            let handle = v.get_secret(*secret)?.to_vec();
            let pk = ssh_engine::ssh_key::PublicKey::from_openssh(&meta.public_key)
                .map_err(|e| CmdError::invalid(e.to_string()))?;
            let flags = hw_keys::sk::key_flags::from_requirements(*require_presence, *require_pin);
            let key = hw_keys::SkKey::from_public_key(&pk, flags, handle)
                .map_err(|e| CmdError::invalid(e.to_string()))?;
            fido_signer(key, asker)?.map(|s| AuthMethod::External(Arc::new(HwSigner(s))))
        }
        KeyStorage::Hardware { platform_ref, .. } => {
            let pk = ssh_engine::ssh_key::PublicKey::from_openssh(&meta.public_key)
                .map_err(|e| CmdError::invalid(e.to_string()))?;
            let backend = crate::mobile::hardware_backend();
            match hw_keys::platform::PlatformSigner::new(backend, platform_ref.clone(), pk) {
                Ok(s) => Some(AuthMethod::External(Arc::new(HwSigner(Arc::new(s))))),
                Err(_) => None,
            }
        }
    })
}

#[cfg(desktop)]
fn fido_signer(
    key: hw_keys::SkKey,
    asker: &Asker,
) -> CmdResult<Option<Arc<dyn hw_keys::HardwareSigner>>> {
    let pin: Arc<dyn hw_keys::PinProvider> = Arc::new(UiPin {
        asker: asker.clone(),
    });
    Ok(Some(Arc::new(hw_keys::fido2::Fido2Signer::new(key, pin))))
}

#[cfg(mobile)]
fn fido_signer(
    _key: hw_keys::SkKey,
    _asker: &Asker,
) -> CmdResult<Option<Arc<dyn hw_keys::HardwareSigner>>> {
    Ok(None)
}

fn hop_for(v: &vault::Vault, plan: &HopPlan, keys: &[KeyMeta], asker: &Asker) -> CmdResult<Hop> {
    let mut hop = Hop::new(plan.host.clone(), plan.port, plan.username.clone());
    hop.host_key_policy = plan.host_key_policy;
    hop.interactive_fallback = true;
    for kid in &plan.keys {
        if let Some(meta) = keys.iter().find(|k| &k.id == kid)
            && let Some(m) = auth_for_key(v, meta, asker)?
        {
            hop.auth.push(m);
        }
    }
    if plan.keys.is_empty() && cfg!(desktop) {
        hop.auth.push(AuthMethod::Agent);
    }
    if let Some(p) = plan.password {
        hop.auth.push(AuthMethod::Password(secret_string(v, p)?));
    }
    hop.auth.push(AuthMethod::KeyboardInteractive);
    Ok(hop)
}

/// Builds engine options for a plan, decrypting the needed secrets.
pub fn connect_options(
    app: &AppHandle,
    plan: &SshPlan,
    asker: &Asker,
) -> CmdResult<ConnectOptions> {
    let state = app.state::<AppState>();
    state.with_vault(|v| {
        let keys = v.list::<KeyMeta>()?;
        let mut hops = Vec::with_capacity(plan.hops.len());
        for h in &plan.hops {
            hops.push(hop_for(v, h, &keys, asker)?);
        }
        let target = hops
            .pop()
            .ok_or_else(|| CmdError::invalid("no target host"))?;
        let mut opts = ConnectOptions::new(target);
        let mut chain = hops;
        chain.append(&mut opts.hops);
        opts.hops = chain;
        opts.proxy = match &plan.proxy {
            Some(p) => {
                let pw = match p.password {
                    Some(r) => Some(secret_string(v, r)?),
                    None => None,
                };
                Some(Proxy::from_config(p, pw))
            }
            None => None,
        };
        opts.connect_timeout = plan.connect_timeout;
        opts.keepalive = plan.keepalive;
        opts.legacy_algorithms = plan.legacy_algorithms;
        opts.post_quantum = plan.post_quantum;
        opts.compression = plan.compression;
        opts.agent_confirm = plan.agent_confirm;
        opts.client_id = format!("SSH-2.0-vaultport_{}", env!("CARGO_PKG_VERSION"));
        Ok(opts)
    })
}

/// Connects according to `plan`, asking the user through `asker`.
pub async fn connect(app: &AppHandle, plan: &SshPlan, asker: Asker) -> CmdResult<Connection> {
    let opts = connect_options(app, plan, &asker)?;
    let interaction = Arc::new(UiInteraction { asker });
    let known = Arc::new(VaultKnownHosts { app: app.clone() });
    ssh_engine::connect(opts, interaction, known)
        .await
        .map_err(map_ssh)
}

pub fn map_ssh(e: ssh_engine::SshError) -> CmdError {
    use ssh_engine::CloseCategory as C;
    let code = match e.category() {
        C::AuthFailed => ErrorCode::AuthFailed,
        C::HostKeyRejected => ErrorCode::HostKeyMismatch,
        C::Network => ErrorCode::Network,
        _ => ErrorCode::Internal,
    };
    CmdError::new(code, e.to_string())
}

/// Shared connections for SFTP panes and port forwarding, keyed by host id.
#[derive(Default)]
pub struct ConnectionPool {
    conns: tokio::sync::Mutex<HashMap<String, Connection>>,
}

impl ConnectionPool {
    /// Returns a live connection to the saved host, connecting if needed.
    pub async fn get(&self, app: &AppHandle, host_id: &str) -> CmdResult<Connection> {
        let mut conns = self.conns.lock().await;
        if let Some(c) = conns.get(host_id)
            && !c.is_closed()
        {
            return Ok(c.clone());
        }
        let plan = match crate::sessions::resolve::plan_for_host(app, host_id)? {
            crate::sessions::resolve::Plan::Ssh(p) => p,
            crate::sessions::resolve::Plan::Telnet(_) => {
                return Err(CmdError::invalid("this host only supports Telnet"));
            }
        };
        let conn = connect(app, &plan, Asker::Global(app.clone())).await?;
        conns.insert(host_id.to_owned(), conn.clone());
        Ok(conn)
    }

    /// Closes all pooled connections (e.g. on vault lock).
    pub async fn close_all(&self) {
        let mut conns = self.conns.lock().await;
        for (_, c) in conns.drain() {
            c.close().await;
        }
    }
}

/// PIN / touch prompts for security keys.
pub fn ui_pin(asker: Asker) -> Arc<dyn hw_keys::PinProvider> {
    Arc::new(UiPin { asker })
}

/// Answers a prompt raised for a background connection.
#[tauri::command]
#[specta::specta]
pub fn prompt_respond(
    broker: tauri::State<'_, PromptBroker>,
    prompt_id: String,
    response: PromptResponse,
) -> CmdResult<()> {
    broker.respond(&prompt_id, response)
}
