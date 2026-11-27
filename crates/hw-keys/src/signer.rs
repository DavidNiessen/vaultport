// SPDX-License-Identifier: GPL-3.0-or-later
//! The signer abstraction exposed to the SSH engine.

use ssh_key::PublicKey;

use crate::error::Result;
use crate::pubkey::{fingerprint_sha256, openssh_line};

/// A key whose private half lives in hardware and which signs SSH authentication data.
///
/// Adapting this to `ssh-engine`'s `ExternalSigner` is a one-liner per method:
/// `public_key()` maps directly and `sign()` already returns the complete SSH wire
/// signature (`string algorithm || string signature [|| sk flags/counter]`).
#[async_trait::async_trait]
pub trait HardwareSigner: Send + Sync + std::fmt::Debug {
    /// The SSH public key (offered to the server in `publickey` authentication).
    fn public_key(&self) -> PublicKey;

    /// The SSH signature algorithm of [`sign`](Self::sign)'s output, e.g.
    /// `sk-ssh-ed25519@openssh.com`, `ecdsa-sha2-nistp256` or `rsa-sha2-256`. For RSA keys
    /// this differs from the key algorithm (`ssh-rsa`).
    fn signature_algorithm(&self) -> &'static str;

    /// Signs `data` and returns the complete SSH wire signature blob for this key.
    ///
    /// May block on user interaction (touch, PIN, biometrics) for a long time; callers
    /// should apply their own timeout and expect [`Error::Cancelled`](crate::Error).
    async fn sign(&self, data: &[u8]) -> Result<Vec<u8>>;

    /// The OpenSSH `authorized_keys` line (without comment).
    fn public_key_openssh(&self) -> String {
        openssh_line(&self.public_key(), "")
    }

    /// The `SHA256:` fingerprint.
    fn fingerprint(&self) -> String {
        fingerprint_sha256(&self.public_key())
    }

    /// Like [`sign`](Self::sign) but returns a typed [`ssh_key::Signature`].
    async fn sign_signature(&self, data: &[u8]) -> Result<ssh_key::Signature> {
        let blob = self.sign(data).await?;
        Ok(ssh_key::Signature::try_from(blob.as_slice())?)
    }
}
