// SPDX-License-Identifier: GPL-3.0-or-later
//! Signing with in-memory keys and app-provided external signers, shared by user
//! authentication and the forwarded agent.

use crate::options::{ExternalSigner, SignerError};
use russh::keys::agent::AgentIdentity;
use russh::keys::ssh_encoding::Encode;
use russh::keys::ssh_key::private::KeypairData;
use russh::keys::ssh_key::{Certificate, HashAlg, PrivateKey, PublicKey, Signature};
use std::fmt;
use std::sync::Arc;

/// A key that can sign: either decoded in memory or held by the app.
#[derive(Clone)]
pub(crate) enum SigningKey {
    Local(Arc<PrivateKey>),
    External(Arc<dyn ExternalSigner>),
}

impl fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local(k) => write!(f, "Local({})", k.algorithm()),
            Self::External(s) => write!(f, "External({})", s.public_key().algorithm()),
        }
    }
}

impl SigningKey {
    pub(crate) fn public_key(&self) -> PublicKey {
        match self {
            Self::Local(k) => k.public_key().clone(),
            Self::External(s) => s.public_key(),
        }
    }

    pub(crate) fn comment(&self) -> String {
        match self {
            Self::Local(k) => k.comment().to_string(),
            Self::External(s) => s.label(),
        }
    }

    /// Produces an SSH signature blob (`string alg || string sig`).
    pub(crate) async fn sign_blob(
        &self,
        data: &[u8],
        hash: Option<HashAlg>,
    ) -> Result<Vec<u8>, SignerError> {
        match self {
            Self::Local(k) => sign_local(k, data, hash),
            Self::External(s) => s.sign(data, hash).await,
        }
    }
}

fn sign_local(
    key: &PrivateKey,
    data: &[u8],
    hash: Option<HashAlg>,
) -> Result<Vec<u8>, SignerError> {
    use russh::keys::signature::Signer as _;
    let sig: Signature = match key.key_data() {
        KeypairData::Rsa(rsa) => (rsa, hash).try_sign(data),
        other => other.try_sign(data),
    }
    .map_err(|e| SignerError(e.to_string()))?;
    sig.encode_vec().map_err(|e| SignerError(e.to_string()))
}

/// Adapter implementing russh's [`russh::Signer`] for user authentication.
///
/// russh expects the data to sign followed by the length-prefixed signature blob.
pub(crate) struct AuthSigner(pub SigningKey);

impl russh::Signer for AuthSigner {
    type Error = SignerFailure;

    fn auth_sign(
        &mut self,
        _key: &AgentIdentity,
        hash_alg: Option<HashAlg>,
        to_sign: Vec<u8>,
    ) -> impl Future<Output = Result<Vec<u8>, Self::Error>> + Send {
        let key = self.0.clone();
        async move {
            let blob = key
                .sign_blob(&to_sign, hash_alg)
                .await
                .map_err(SignerFailure::Signer)?;
            let mut out = to_sign;
            blob.encode(&mut out)
                .map_err(|e| SignerFailure::Signer(SignerError(e.to_string())))?;
            Ok(out)
        }
    }
}

/// Error type of [`AuthSigner`].
#[derive(Debug, thiserror::Error)]
pub(crate) enum SignerFailure {
    #[error("signing failed: {0}")]
    Signer(SignerError),
    #[error("session closed")]
    Send,
}

impl From<russh::SendError> for SignerFailure {
    fn from(_: russh::SendError) -> Self {
        Self::Send
    }
}

/// An identity served by the forwarded agent.
#[derive(Clone, Debug)]
pub(crate) struct AgentKey {
    pub key: SigningKey,
    pub certificate: Option<Certificate>,
}
