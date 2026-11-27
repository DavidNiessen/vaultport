// SPDX-License-Identifier: GPL-3.0-or-later
//! In-process, read-only SSH agent served over forwarded `auth-agent@openssh.com` channels.
//!
//! Only `REQUEST_IDENTITIES` and `SIGN_REQUEST` are implemented; adding/removing keys, locking
//! and extensions are refused, so a compromised server can never inject or extract keys. The
//! identities are the keys configured for the target hop of this connection. Every signature
//! can be confirmed by the user ([`crate::Interaction::confirm_agent_use`]). RSA signatures
//! with SHA-1 (`ssh-rsa`) are refused unless legacy algorithms are enabled.

use crate::fingerprint::sha256_fingerprint;
use crate::interaction::{AgentSignRequest, Interaction};
use crate::signer::AgentKey;
use russh::keys::ssh_encoding::{Decode, Encode};
use russh::keys::ssh_key::HashAlg;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAX_FRAME: usize = 256 * 1024;

const FAILURE: u8 = 5;
const REQUEST_IDENTITIES: u8 = 11;
const IDENTITIES_ANSWER: u8 = 12;
const SIGN_REQUEST: u8 = 13;
const SIGN_RESPONSE: u8 = 14;

const FLAG_RSA_SHA2_256: u32 = 2;
const FLAG_RSA_SHA2_512: u32 = 4;

/// The agent served to the remote side.
pub(crate) struct ForwardAgent {
    pub keys: Vec<AgentKey>,
    pub interaction: Arc<dyn Interaction>,
    pub confirm: bool,
    pub allow_sha1: bool,
    pub hop_label: String,
}

impl std::fmt::Debug for ForwardAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForwardAgent")
            .field("keys", &self.keys.len())
            .field("confirm", &self.confirm)
            .finish_non_exhaustive()
    }
}

impl ForwardAgent {
    /// Serves one agent connection until EOF or a protocol error.
    pub(crate) async fn serve<S: AsyncRead + AsyncWrite + Unpin>(&self, mut stream: S) {
        loop {
            let mut len = [0u8; 4];
            if stream.read_exact(&mut len).await.is_err() {
                return;
            }
            let len = u32::from_be_bytes(len) as usize;
            if len == 0 || len > MAX_FRAME {
                return;
            }
            let mut frame = vec![0u8; len];
            if stream.read_exact(&mut frame).await.is_err() {
                return;
            }
            let reply = self.handle(&frame).await;
            let mut out = Vec::with_capacity(reply.len() + 4);
            out.extend_from_slice(&(reply.len() as u32).to_be_bytes());
            out.extend_from_slice(&reply);
            if stream.write_all(&out).await.is_err() || stream.flush().await.is_err() {
                return;
            }
        }
    }

    async fn handle(&self, frame: &[u8]) -> Vec<u8> {
        match frame.split_first() {
            Some((&REQUEST_IDENTITIES, _)) => self.identities().unwrap_or_else(|| vec![FAILURE]),
            Some((&SIGN_REQUEST, rest)) => self.sign(rest).await.unwrap_or_else(|| vec![FAILURE]),
            _ => vec![FAILURE],
        }
    }

    fn identity_blobs(&self) -> Vec<(Vec<u8>, &AgentKey, String)> {
        let mut out = Vec::new();
        for k in &self.keys {
            let pk = k.key.public_key();
            if let Some(cert) = &k.certificate
                && let Ok(blob) = cert.to_bytes()
            {
                out.push((blob, k, format!("{} (certificate)", k.key.comment())));
            }
            if let Ok(blob) = pk.key_data().encode_vec() {
                out.push((blob, k, k.key.comment()));
            }
        }
        out
    }

    fn identities(&self) -> Option<Vec<u8>> {
        let ids = self.identity_blobs();
        let mut out = vec![IDENTITIES_ANSWER];
        (ids.len() as u32).encode(&mut out).ok()?;
        for (blob, _, comment) in ids {
            blob.encode(&mut out).ok()?;
            comment.encode(&mut out).ok()?;
        }
        Some(out)
    }

    async fn sign(&self, mut r: &[u8]) -> Option<Vec<u8>> {
        let blob = Vec::<u8>::decode(&mut r).ok()?;
        let data = Vec::<u8>::decode(&mut r).ok()?;
        let flags = u32::decode(&mut r).ok()?;
        let ids = self.identity_blobs();
        let (_, key, comment) = ids.into_iter().find(|(b, _, _)| *b == blob)?;

        let pk = key.key.public_key();
        let hash = if pk.algorithm().is_rsa() {
            if flags & FLAG_RSA_SHA2_512 != 0 {
                Some(HashAlg::Sha512)
            } else if flags & FLAG_RSA_SHA2_256 != 0 {
                Some(HashAlg::Sha256)
            } else if self.allow_sha1 {
                None
            } else {
                tracing::warn!("refusing ssh-rsa (SHA-1) agent signature");
                return None;
            }
        } else {
            None
        };

        if self.confirm {
            let req = AgentSignRequest {
                hop: self.hop_label.clone(),
                key_fingerprint: sha256_fingerprint(pk.key_data()),
                key_comment: comment,
            };
            if !self.interaction.confirm_agent_use(&req).await {
                return None;
            }
        }
        let sig = key.key.sign_blob(&data, hash).await.ok()?;
        let mut out = vec![SIGN_RESPONSE];
        sig.encode(&mut out).ok()?;
        Some(out)
    }
}

/// Decodes the key blob of an identities answer (used by tests).
#[cfg(test)]
fn parse_identities(mut r: &[u8]) -> Vec<(russh::keys::ssh_key::public::KeyData, String)> {
    let mut out = Vec::new();
    let Ok(n) = u32::decode(&mut r) else {
        return out;
    };
    for _ in 0..n {
        let (Ok(blob), Ok(comment)) = (Vec::<u8>::decode(&mut r), String::decode(&mut r)) else {
            break;
        };
        if let Ok(k) = russh::keys::ssh_key::public::KeyData::decode(&mut blob.as_slice()) {
            out.push((k, comment));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::HostKeyDecision;
    use crate::known_hosts::HostKeyCheck;
    use crate::signer::SigningKey;
    use async_trait::async_trait;
    use russh::keys::ssh_key::{Algorithm, PrivateKey, Signature};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Confirm(bool, AtomicUsize);

    #[async_trait]
    impl Interaction for Confirm {
        async fn verify_host_key(&self, _: &HostKeyCheck) -> HostKeyDecision {
            HostKeyDecision::Reject
        }
        async fn confirm_agent_use(&self, _: &AgentSignRequest) -> bool {
            self.1.fetch_add(1, Ordering::SeqCst);
            self.0
        }
    }

    async fn roundtrip(agent: &ForwardAgent, msg: &[u8]) -> Vec<u8> {
        let (mut client, server) = tokio::io::duplex(4096);
        let serve = agent.serve(server);
        let talk = async {
            client
                .write_all(&(msg.len() as u32).to_be_bytes())
                .await
                .expect("w");
            client.write_all(msg).await.expect("w");
            let mut len = [0u8; 4];
            client.read_exact(&mut len).await.expect("r");
            let mut buf = vec![0u8; u32::from_be_bytes(len) as usize];
            client.read_exact(&mut buf).await.expect("r");
            drop(client);
            buf
        };
        let ((), out) = tokio::join!(serve, talk);
        out
    }

    #[tokio::test]
    async fn lists_and_signs_with_confirmation() {
        let key =
            PrivateKey::random(&mut russh::keys::key::safe_rng(), Algorithm::Ed25519).expect("key");
        let pk = key.public_key().clone();
        let confirm = Arc::new(Confirm(true, AtomicUsize::new(0)));
        let agent = ForwardAgent {
            keys: vec![AgentKey {
                key: SigningKey::Local(Arc::new(key)),
                certificate: None,
            }],
            interaction: confirm.clone(),
            confirm: true,
            allow_sha1: false,
            hop_label: "u@h:22".into(),
        };
        let ids = roundtrip(&agent, &[REQUEST_IDENTITIES]).await;
        assert_eq!(ids[0], IDENTITIES_ANSWER);
        let parsed = parse_identities(&ids[1..]);
        assert_eq!(parsed.len(), 1);
        assert_eq!(&parsed[0].0, pk.key_data());

        let mut req = vec![SIGN_REQUEST];
        pk.key_data()
            .encode_vec()
            .expect("blob")
            .encode(&mut req)
            .expect("e");
        b"challenge".to_vec().encode(&mut req).expect("e");
        0u32.encode(&mut req).expect("e");
        let resp = roundtrip(&agent, &req).await;
        assert_eq!(resp[0], SIGN_RESPONSE);
        let mut r = &resp[1..];
        let blob = Vec::<u8>::decode(&mut r).expect("blob");
        let sig = Signature::decode(&mut blob.as_slice()).expect("sig");
        russh::keys::signature::Verifier::verify(pk.key_data(), b"challenge", &sig)
            .expect("valid signature");
        assert_eq!(confirm.1.load(Ordering::SeqCst), 1);

        // Denied confirmation → failure; key management requests → failure.
        let deny = ForwardAgent {
            interaction: Arc::new(Confirm(false, AtomicUsize::new(0))),
            ..agent
        };
        assert_eq!(roundtrip(&deny, &req).await, vec![FAILURE]);
        assert_eq!(roundtrip(&deny, &[17, 0, 0]).await, vec![FAILURE]);
        assert_eq!(roundtrip(&deny, &[19]).await, vec![FAILURE]);
    }
}
