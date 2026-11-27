// SPDX-License-Identifier: GPL-3.0-or-later
//! User authentication for one hop.
//!
//! Methods are tried in the configured order, skipping those the server does not currently
//! accept. After a *partial success* (e.g. `AuthenticationMethods publickey,password`) the
//! not-yet-successful methods become eligible again. When everything configured failed and
//! [`crate::Hop::interactive_fallback`] is set, `keyboard-interactive` and `password` prompts
//! are offered (like OpenSSH).
//!
//! RSA keys always sign with `rsa-sha2-512`/`rsa-sha2-256` as announced by the server's
//! `server-sig-algs`; SHA-1 `ssh-rsa` is used only with `legacy_algorithms`.

use crate::error::SshError;
use crate::handler::{ClientHandler, Pauser};
use crate::interaction::{Interaction, Prompt};
use crate::keys::{KeyError, certificate_for_key, decode_private_key};
use crate::options::{AuthMethod, Hop};
use crate::signer::{AgentKey, AuthSigner, SigningKey};
use russh::client::AuthResult;
use russh::client::{Handle, KeyboardInteractiveAuthResponse};
use russh::keys::PrivateKeyWithHashAlg;
use russh::keys::ssh_key::{Certificate, HashAlg, PrivateKey, PublicKey};
use russh::{MethodKind, MethodSet};
use std::sync::Arc;
use zeroize::Zeroizing;

const MAX_PASSWORD_PROMPTS: usize = 3;
const MAX_PASSPHRASE_PROMPTS: usize = 3;
const MAX_KBD_ROUNDS: usize = 10;
const MAX_ATTEMPTS: usize = 32;

pub(crate) struct AuthContext<'a> {
    pub interaction: &'a Arc<dyn Interaction>,
    pub pauser: &'a Pauser,
    pub legacy: bool,
    pub label: String,
}

enum Outcome {
    Success,
    Failure {
        remaining: MethodSet,
        partial: bool,
    },
    /// Method not applicable / cancelled / not available.
    Skipped,
}

impl From<AuthResult> for Outcome {
    fn from(r: AuthResult) -> Self {
        match r {
            AuthResult::Success => Self::Success,
            AuthResult::Failure {
                remaining_methods,
                partial_success,
            } => Self::Failure {
                remaining: remaining_methods,
                partial: partial_success,
            },
        }
    }
}

fn kind_of(m: &AuthMethod) -> MethodKind {
    match m {
        AuthMethod::Password(_) => MethodKind::Password,
        AuthMethod::KeyboardInteractive => MethodKind::KeyboardInteractive,
        AuthMethod::PrivateKey { .. } | AuthMethod::External(_) | AuthMethod::Agent => {
            MethodKind::PublicKey
        }
    }
}

fn allowed(remaining: &MethodSet, kind: MethodKind) -> bool {
    remaining.is_empty() || remaining.contains(&kind)
}

/// RSA signature algorithms announced by the server (`server-sig-algs`).
#[derive(Debug, Clone, Copy)]
pub(crate) enum RsaSigAlgs {
    /// Best `rsa-sha2-*` hash.
    Sha2(HashAlg),
    /// Only legacy `ssh-rsa`.
    Sha1Only,
    /// No `server-sig-algs` extension.
    NotAnnounced,
}

fn names(remaining: &MethodSet) -> Vec<String> {
    remaining
        .iter()
        .map(|k| <&'static str>::from(k).to_owned())
        .collect()
}

/// Authenticates `hop` on `handle`. Returns the keys usable by the forwarded agent.
pub(crate) async fn authenticate(
    handle: &mut Handle<ClientHandler>,
    hop: &Hop,
    ctx: &AuthContext<'_>,
) -> Result<Vec<AgentKey>, SshError> {
    let user = hop.username.clone();
    let mut agent_keys: Vec<AgentKey> = Vec::new();
    let mut rsa_hash: Option<RsaSigAlgs> = None;

    let mut remaining = match Outcome::from(handle.authenticate_none(user.clone()).await?) {
        Outcome::Success => return Ok(agent_keys),
        Outcome::Failure { remaining, .. } => remaining,
        Outcome::Skipped => MethodSet::empty(),
    };

    let mut done = vec![false; hop.auth.len()];
    let mut succeeded_partially = vec![false; hop.auth.len()];
    let mut attempts = 0;
    let mut tried_kbd = false;

    while let Some(i) =
        (0..hop.auth.len()).find(|&i| !done[i] && allowed(&remaining, kind_of(&hop.auth[i])))
    {
        attempts += 1;
        if attempts > MAX_ATTEMPTS {
            break;
        }
        done[i] = true;
        let method = &hop.auth[i];
        if matches!(method, AuthMethod::KeyboardInteractive) {
            tried_kbd = true;
        }
        let outcome =
            try_method(handle, &user, method, ctx, &mut rsa_hash, &mut agent_keys).await?;
        match outcome {
            Outcome::Success => return Ok(agent_keys),
            Outcome::Failure {
                remaining: r,
                partial,
            } => {
                remaining = r;
                if partial {
                    tracing::debug!(hop = %ctx.label, "partial authentication success");
                    succeeded_partially[i] = true;
                    // Methods that failed only because they were not yet allowed may be tried
                    // again now.
                    for (d, s) in done.iter_mut().zip(&succeeded_partially) {
                        *d = *s;
                    }
                }
            }
            Outcome::Skipped => {}
        }
    }

    if hop.interactive_fallback {
        if !tried_kbd
            && allowed(&remaining, MethodKind::KeyboardInteractive)
            && !remaining.is_empty()
        {
            match keyboard_interactive(handle, &user, ctx).await? {
                Outcome::Success => return Ok(agent_keys),
                Outcome::Failure { remaining: r, .. } => remaining = r,
                Outcome::Skipped => {}
            }
        }
        let mut asked = 0;
        while asked < MAX_PASSWORD_PROMPTS && remaining.contains(&MethodKind::Password) {
            asked += 1;
            let Some(pw) = ctx
                .pauser
                .run(ctx.interaction.password(&ctx.label, &user))
                .await
            else {
                break;
            };
            match Outcome::from(
                handle
                    .authenticate_password(user.clone(), pw.as_str())
                    .await?,
            ) {
                Outcome::Success => return Ok(agent_keys),
                Outcome::Failure { remaining: r, .. } => remaining = r,
                Outcome::Skipped => {}
            }
        }
    }

    Err(SshError::AuthFailed {
        hop: ctx.label.clone(),
        remaining: names(&remaining),
    })
}

async fn try_method(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    method: &AuthMethod,
    ctx: &AuthContext<'_>,
    rsa_hash: &mut Option<RsaSigAlgs>,
    agent_keys: &mut Vec<AgentKey>,
) -> Result<Outcome, SshError> {
    match method {
        AuthMethod::Password(pw) => Ok(handle
            .authenticate_password(user.to_owned(), pw.as_str())
            .await?
            .into()),
        AuthMethod::KeyboardInteractive => keyboard_interactive(handle, user, ctx).await,
        AuthMethod::PrivateKey {
            key_pem,
            passphrase,
            certificate,
        } => {
            let Some(key) = load_key(key_pem, passphrase.as_ref(), ctx).await? else {
                return Ok(Outcome::Skipped);
            };
            let key = Arc::new(key);
            let cert = match certificate {
                Some(c) => Some(certificate_for_key(c, key.public_key())?),
                None => None,
            };
            agent_keys.push(AgentKey {
                key: SigningKey::Local(key.clone()),
                certificate: cert.clone(),
            });
            let signer = SigningKey::Local(key.clone());
            publickey(handle, user, &signer, cert, ctx, rsa_hash).await
        }
        AuthMethod::External(s) => {
            let signer = SigningKey::External(s.clone());
            let cert = s.certificate();
            agent_keys.push(AgentKey {
                key: signer.clone(),
                certificate: cert.clone(),
            });
            publickey(handle, user, &signer, cert, ctx, rsa_hash).await
        }
        AuthMethod::Agent => system_agent(handle, user, ctx, rsa_hash).await,
    }
}

async fn load_key(
    pem: &Zeroizing<String>,
    passphrase: Option<&Zeroizing<String>>,
    ctx: &AuthContext<'_>,
) -> Result<Option<PrivateKey>, SshError> {
    match decode_private_key(pem, passphrase.map(|p| p.as_str())) {
        Ok(k) => return Ok(Some(k)),
        Err(KeyError::NeedsPassphrase | KeyError::WrongPassphrase) => {}
        Err(e) => return Err(e.into()),
    }
    let desc = crate::keys::inspect_private_key(pem, None)
        .ok()
        .and_then(|i| i.comment.or(i.fingerprint))
        .unwrap_or_else(|| "private key".to_owned());
    for _ in 0..MAX_PASSPHRASE_PROMPTS {
        let Some(p) = ctx
            .pauser
            .run(ctx.interaction.key_passphrase(&ctx.label, &desc))
            .await
        else {
            return Ok(None);
        };
        match decode_private_key(pem, Some(p.as_str())) {
            Ok(k) => return Ok(Some(k)),
            Err(KeyError::WrongPassphrase | KeyError::NeedsPassphrase) => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(None)
}

/// Hash algorithms to try for an RSA key, best first.
async fn rsa_hashes(
    handle: &Handle<ClientHandler>,
    cache: &mut Option<RsaSigAlgs>,
    legacy: bool,
) -> Vec<Option<HashAlg>> {
    let algs = if let Some(a) = *cache {
        a
    } else {
        let a = match handle.best_supported_rsa_hash().await {
            Ok(Some(Some(h))) => RsaSigAlgs::Sha2(h),
            Ok(Some(None)) => RsaSigAlgs::Sha1Only,
            _ => RsaSigAlgs::NotAnnounced,
        };
        *cache = Some(a);
        a
    };
    match algs {
        RsaSigAlgs::Sha2(h) => vec![Some(h)],
        RsaSigAlgs::Sha1Only => {
            if legacy {
                vec![None]
            } else {
                Vec::new()
            }
        }
        // No server-sig-algs (very old server or not announced): try SHA-2 first.
        RsaSigAlgs::NotAnnounced => {
            let mut v = vec![Some(HashAlg::Sha512), Some(HashAlg::Sha256)];
            if legacy {
                v.push(None);
            }
            v
        }
    }
}

async fn publickey(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    signer: &SigningKey,
    cert: Option<Certificate>,
    ctx: &AuthContext<'_>,
    rsa_hash: &mut Option<RsaSigAlgs>,
) -> Result<Outcome, SshError> {
    let pk: PublicKey = signer.public_key();
    let hashes = if pk.algorithm().is_rsa() {
        let h = rsa_hashes(handle, rsa_hash, ctx.legacy).await;
        if h.is_empty() {
            tracing::warn!(hop = %ctx.label, "server only accepts ssh-rsa (SHA-1); enable legacy algorithms");
            return Ok(Outcome::Skipped);
        }
        h
    } else {
        vec![None]
    };
    let mut last = Outcome::Skipped;
    for hash in hashes {
        if let Some(cert) = &cert {
            let mut s = AuthSigner(signer.clone());
            match handle
                .authenticate_certificate_with(user.to_owned(), cert.clone(), hash, &mut s)
                .await
            {
                Ok(r) => {
                    let o = Outcome::from(r);
                    if !matches!(o, Outcome::Failure { partial: false, .. }) {
                        return Ok(o);
                    }
                    last = o;
                }
                Err(e) => tracing::debug!(hop = %ctx.label, "certificate signing failed: {e}"),
            }
        }
        let result = match signer {
            SigningKey::Local(k) => handle
                .authenticate_publickey(
                    user.to_owned(),
                    PrivateKeyWithHashAlg::new(k.clone(), hash),
                )
                .await
                .map_err(SshError::from),
            SigningKey::External(_) => {
                let mut s = AuthSigner(signer.clone());
                handle
                    .authenticate_publickey_with(user.to_owned(), pk.clone(), hash, &mut s)
                    .await
                    .map_err(|e| SshError::Refused(e.to_string()))
            }
        };
        match result {
            Ok(r) => {
                let o = Outcome::from(r);
                if !matches!(o, Outcome::Failure { partial: false, .. }) {
                    return Ok(o);
                }
                last = o;
            }
            Err(e) => {
                tracing::debug!(hop = %ctx.label, "public key signing failed: {e}");
            }
        }
    }
    Ok(last)
}

async fn keyboard_interactive(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    ctx: &AuthContext<'_>,
) -> Result<Outcome, SshError> {
    let mut resp = handle
        .authenticate_keyboard_interactive_start(user.to_owned(), None)
        .await?;
    for _ in 0..MAX_KBD_ROUNDS {
        match resp {
            KeyboardInteractiveAuthResponse::Success => return Ok(Outcome::Success),
            KeyboardInteractiveAuthResponse::Failure {
                remaining_methods,
                partial_success,
            } => {
                return Ok(Outcome::Failure {
                    remaining: remaining_methods,
                    partial: partial_success,
                });
            }
            KeyboardInteractiveAuthResponse::InfoRequest {
                name,
                instructions,
                prompts,
            } => {
                let answers: Vec<String> = if prompts.is_empty() {
                    Vec::new()
                } else {
                    let prompts: Vec<Prompt> = prompts
                        .into_iter()
                        .map(|p| Prompt {
                            text: p.prompt,
                            echo: p.echo,
                        })
                        .collect();
                    let answer = ctx
                        .pauser
                        .run(ctx.interaction.keyboard_interactive(
                            &ctx.label,
                            &name,
                            &instructions,
                            &prompts,
                        ))
                        .await;
                    match answer {
                        Some(a) if a.len() == prompts.len() => {
                            a.iter().map(|s| s.as_str().to_owned()).collect()
                        }
                        // Cancelled: answer with empty strings so the server fails this round.
                        _ => {
                            let r = handle
                                .authenticate_keyboard_interactive_respond(vec![
                                    String::new();
                                    prompts.len()
                                ])
                                .await?;
                            return Ok(match r {
                                KeyboardInteractiveAuthResponse::Success => Outcome::Success,
                                KeyboardInteractiveAuthResponse::Failure {
                                    remaining_methods,
                                    partial_success,
                                } => Outcome::Failure {
                                    remaining: remaining_methods,
                                    partial: partial_success,
                                },
                                KeyboardInteractiveAuthResponse::InfoRequest { .. } => {
                                    Outcome::Skipped
                                }
                            });
                        }
                    }
                };
                resp = handle
                    .authenticate_keyboard_interactive_respond(answers)
                    .await?;
            }
        }
    }
    Ok(Outcome::Skipped)
}

#[cfg(unix)]
async fn agent_client() -> Option<
    russh::keys::agent::client::AgentClient<
        Box<dyn russh::keys::agent::client::AgentStream + Send + Unpin>,
    >,
> {
    let c = russh::keys::agent::client::AgentClient::connect_env()
        .await
        .ok()?;
    Some(c.dynamic())
}

#[cfg(windows)]
async fn agent_client() -> Option<
    russh::keys::agent::client::AgentClient<
        Box<dyn russh::keys::agent::client::AgentStream + Send + Unpin>,
    >,
> {
    let pipe = std::env::var("SSH_AUTH_SOCK")
        .ok()
        .filter(|p| p.starts_with(r"\\.\pipe\"))
        .unwrap_or_else(|| r"\\.\pipe\openssh-ssh-agent".to_owned());
    if let Ok(c) = russh::keys::agent::client::AgentClient::connect_named_pipe(pipe).await {
        return Some(c.dynamic());
    }
    let c = russh::keys::agent::client::AgentClient::connect_pageant()
        .await
        .ok()?;
    Some(c.dynamic())
}

#[cfg(not(any(unix, windows)))]
async fn agent_client() -> Option<
    russh::keys::agent::client::AgentClient<
        Box<dyn russh::keys::agent::client::AgentStream + Send + Unpin>,
    >,
> {
    None
}

async fn system_agent(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    ctx: &AuthContext<'_>,
    rsa_hash: &mut Option<RsaSigAlgs>,
) -> Result<Outcome, SshError> {
    let Some(mut agent) = agent_client().await else {
        tracing::debug!("no SSH agent reachable");
        return Ok(Outcome::Skipped);
    };
    let Ok(identities) = agent.request_identities().await else {
        return Ok(Outcome::Skipped);
    };
    let mut last = Outcome::Skipped;
    for id in identities {
        let pk = id.public_key().into_owned();
        let hashes = if pk.algorithm().is_rsa() {
            rsa_hashes(handle, rsa_hash, ctx.legacy).await
        } else {
            vec![None]
        };
        for hash in hashes {
            let r = match &id {
                russh::keys::agent::AgentIdentity::Certificate { certificate, .. } => {
                    handle
                        .authenticate_certificate_with(
                            user.to_owned(),
                            certificate.clone(),
                            hash,
                            &mut agent,
                        )
                        .await
                }
                russh::keys::agent::AgentIdentity::PublicKey { key, .. } => {
                    handle
                        .authenticate_publickey_with(user.to_owned(), key.clone(), hash, &mut agent)
                        .await
                }
            };
            match r {
                Ok(r) => {
                    let o = Outcome::from(r);
                    if !matches!(o, Outcome::Failure { partial: false, .. }) {
                        return Ok(o);
                    }
                    last = o;
                }
                Err(e) => tracing::debug!("agent signing failed: {e}"),
            }
        }
    }
    Ok(last)
}
