// SPDX-License-Identifier: GPL-3.0-or-later
//! Platform-bound keys: non-exportable keys in the device's secure hardware, unlocked by
//! biometrics for every signature.
//!
//! | Platform      | Backend                         | Key / signature                     |
//! | ------------- | ------------------------------- | ----------------------------------- |
//! | macOS, iOS    | [`SecureEnclaveBackend`]        | P-256 / `ecdsa-sha2-nistp256`       |
//! | Windows       | [`WindowsHelloBackend`]         | RSA-2048 / `rsa-sha2-256` (see below) |
//! | Android       | [`BridgeBackend`] (app plugin)  | P-256 / `ecdsa-sha2-nistp256`       |
//! | Linux, others | [`UnsupportedBackend`]          | —                                   |
//!
//! **Windows:** Windows Hello's `KeyCredentialManager` only creates RSA-2048 keys (TPM
//! backed where a TPM exists) and signs with RSASSA-PKCS1-v1_5/SHA-256, so these keys are
//! `ssh-rsa` keys producing `rsa-sha2-256` signatures (RFC 8332). Servers that only accept
//! the legacy SHA-1 `ssh-rsa` signature cannot be used with them. An NCrypt "Platform
//! Crypto Provider" P-256 key would give ECDSA, but its UI policy only offers a
//! consent/password prompt, not Hello biometrics, so it was not chosen.
//!
//! **Linux:** TPM2-backed keys are out of scope; [`default_backend`] returns
//! [`UnsupportedBackend`].
//!
//! [`PlatformSigner`] wraps any backend as a [`HardwareSigner`]: it converts the
//! backend's DER / P1363 / PKCS#1 output to an SSH signature and verifies ECDSA
//! signatures against the public key before returning them.

use std::fmt;
use std::sync::Arc;

use p256::ecdsa::signature::Verifier as _;
use ssh_key::PublicKey;
use ssh_key::public::{EcdsaPublicKey, KeyData};

use crate::ecdsa::{ECDSA_P256_ALGORITHM, EcdsaP256Signature};
use crate::error::{Error, Result};
use crate::pubkey::{RSA_SHA256_SIGNATURE_ALGORITHM, rsa_sha256_ssh_signature};
use crate::signer::HardwareSigner;

mod bridge;
pub use bridge::{BridgeBackend, BridgeKey, BridgeSignatureEncoding};

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple;
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use apple::SecureEnclaveBackend;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::WindowsHelloBackend;

/// Who may use the key, checked by the OS before every signature.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum AccessPolicy {
    /// Biometrics only, and the key becomes unusable when the enrolled biometrics change
    /// (Apple `biometryCurrentSet`; Android `setInvalidatedByBiometricEnrollment(true)`).
    #[default]
    BiometryCurrentSet,
    /// Any enrolled biometric (Apple `biometryAny`).
    BiometryAny,
    /// Biometrics or the device passcode (Apple `userPresence`).
    UserPresence,
}

/// A newly created platform key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformKeyInfo {
    /// Opaque backend reference; store it as `KeyStorage::Hardware { platform_ref }`.
    pub platform_ref: String,
    /// The SSH public key.
    pub public_key: PublicKey,
}

/// Raw signature as produced by a platform key store over `SHA-256(data)`.
#[derive(Clone, PartialEq, Eq)]
pub enum PlatformSignature {
    /// ASN.1 DER ECDSA P-256 signature (Secure Enclave, Android `SHA256withECDSA`).
    EcdsaDer(Vec<u8>),
    /// IEEE P1363 `r || s` ECDSA P-256 signature (Windows CNG).
    EcdsaP1363(Vec<u8>),
    /// RSASSA-PKCS1-v1_5 with SHA-256 (Windows Hello).
    RsaPkcs1Sha256(Vec<u8>),
}

impl fmt::Debug for PlatformSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (kind, len) = match self {
            Self::EcdsaDer(b) => ("EcdsaDer", b.len()),
            Self::EcdsaP1363(b) => ("EcdsaP1363", b.len()),
            Self::RsaPkcs1Sha256(b) => ("RsaPkcs1Sha256", b.len()),
        };
        write!(f, "{kind}(<{len} bytes>)")
    }
}

/// A platform key store. Implementations hash `data` with SHA-256 themselves.
#[async_trait::async_trait]
pub trait PlatformKeyBackend: Send + Sync + fmt::Debug {
    /// Human readable store name, e.g. `"Secure Enclave"` (store it as the key's device).
    fn name(&self) -> &'static str;

    /// Whether keys can be created here (hardware present, biometrics enrolled, ...).
    async fn is_available(&self) -> bool;

    /// Creates a new non-exportable key. May prompt the user.
    async fn create(&self, label: &str, policy: AccessPolicy) -> Result<PlatformKeyInfo>;

    /// Signs `data` (hashed with SHA-256 by the backend). Prompts for biometrics.
    async fn sign(&self, platform_ref: &str, data: &[u8]) -> Result<PlatformSignature>;

    /// Deletes the key. Deleting a missing key returns [`Error::KeyNotFound`].
    async fn delete(&self, platform_ref: &str) -> Result<()>;

    /// Looks up the public key of an existing key.
    async fn public_key(&self, platform_ref: &str) -> Result<PublicKey> {
        let _ = platform_ref;
        Err(Error::Unsupported("public key lookup"))
    }
}

/// Backend for platforms without a supported key store.
#[derive(Debug, Clone, Copy)]
pub struct UnsupportedBackend {
    reason: &'static str,
}

impl UnsupportedBackend {
    /// A backend that fails every operation with [`Error::Unsupported`]`(reason)`.
    pub const fn new(reason: &'static str) -> Self {
        Self { reason }
    }
}

#[async_trait::async_trait]
impl PlatformKeyBackend for UnsupportedBackend {
    fn name(&self) -> &'static str {
        "unsupported"
    }

    async fn is_available(&self) -> bool {
        false
    }

    async fn create(&self, _label: &str, _policy: AccessPolicy) -> Result<PlatformKeyInfo> {
        Err(Error::Unsupported(self.reason))
    }

    async fn sign(&self, _platform_ref: &str, _data: &[u8]) -> Result<PlatformSignature> {
        Err(Error::Unsupported(self.reason))
    }

    async fn delete(&self, _platform_ref: &str) -> Result<()> {
        Err(Error::Unsupported(self.reason))
    }

    async fn public_key(&self, _platform_ref: &str) -> Result<PublicKey> {
        Err(Error::Unsupported(self.reason))
    }
}

/// The built-in backend for the current OS.
///
/// On Android this is [`UnsupportedBackend`]: the app constructs a [`BridgeBackend`] wired
/// to its Android Keystore plugin instead.
pub fn default_backend() -> Arc<dyn PlatformKeyBackend> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        Arc::new(SecureEnclaveBackend::new())
    }
    #[cfg(windows)]
    {
        Arc::new(WindowsHelloBackend::new())
    }
    #[cfg(target_os = "android")]
    {
        Arc::new(UnsupportedBackend::new(
            "Android Keystore keys are provided by the app's bridge",
        ))
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios", windows, target_os = "android")))]
    {
        Arc::new(UnsupportedBackend::new(
            "hardware-bound keys are not supported on this platform (TPM2 is not implemented)",
        ))
    }
}

/// A platform key used as a [`HardwareSigner`].
#[derive(Clone)]
pub struct PlatformSigner {
    backend: Arc<dyn PlatformKeyBackend>,
    platform_ref: String,
    public_key: PublicKey,
    kind: SignerKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SignerKind {
    EcdsaP256,
    Rsa,
}

impl fmt::Debug for PlatformSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlatformSigner")
            .field("backend", &self.backend.name())
            .field("platform_ref", &self.platform_ref)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl PlatformSigner {
    /// Wraps the key `platform_ref` of `backend`, whose public key is `public_key` (as
    /// stored in `KeyMeta`). Only P-256 ECDSA and RSA keys are accepted.
    pub fn new(
        backend: Arc<dyn PlatformKeyBackend>,
        platform_ref: impl Into<String>,
        public_key: PublicKey,
    ) -> Result<Self> {
        let kind = match public_key.key_data() {
            KeyData::Ecdsa(EcdsaPublicKey::NistP256(_)) => SignerKind::EcdsaP256,
            KeyData::Rsa(_) => SignerKind::Rsa,
            _ => return Err(Error::Unsupported("platform key type")),
        };
        Ok(Self {
            backend,
            platform_ref: platform_ref.into(),
            public_key,
            kind,
        })
    }

    /// Creates a key in `backend` and returns its signer.
    pub async fn create(
        backend: Arc<dyn PlatformKeyBackend>,
        label: &str,
        policy: AccessPolicy,
    ) -> Result<Self> {
        let info = backend.create(label, policy).await?;
        Self::new(backend, info.platform_ref, info.public_key)
    }

    /// The backend reference of the key.
    pub fn platform_ref(&self) -> &str {
        &self.platform_ref
    }

    fn ecdsa_ssh_signature(&self, data: &[u8], signature: EcdsaP256Signature) -> Result<Vec<u8>> {
        let KeyData::Ecdsa(point) = self.public_key.key_data() else {
            return Err(Error::InvalidSignature);
        };
        let verifying_key = p256::ecdsa::VerifyingKey::from_sec1_bytes(point.as_sec1_bytes())
            .map_err(|_| Error::InvalidSignature)?;
        verifying_key
            .verify(data, &signature.to_p256()?)
            .map_err(|_| Error::InvalidSignature)?;
        Ok(signature.to_ssh_signature())
    }
}

#[async_trait::async_trait]
impl HardwareSigner for PlatformSigner {
    fn public_key(&self) -> PublicKey {
        self.public_key.clone()
    }

    fn signature_algorithm(&self) -> &'static str {
        match self.kind {
            SignerKind::EcdsaP256 => ECDSA_P256_ALGORITHM,
            SignerKind::Rsa => RSA_SHA256_SIGNATURE_ALGORITHM,
        }
    }

    async fn sign(&self, data: &[u8]) -> Result<Vec<u8>> {
        let raw = self.backend.sign(&self.platform_ref, data).await?;
        match (self.kind, raw) {
            (SignerKind::EcdsaP256, PlatformSignature::EcdsaDer(der)) => {
                self.ecdsa_ssh_signature(data, EcdsaP256Signature::from_der(&der)?)
            }
            (SignerKind::EcdsaP256, PlatformSignature::EcdsaP1363(raw)) => {
                self.ecdsa_ssh_signature(data, EcdsaP256Signature::from_p1363(&raw)?)
            }
            // RSA signatures are not verified locally (that would need an RSA
            // implementation); the length is checked against the modulus.
            (SignerKind::Rsa, PlatformSignature::RsaPkcs1Sha256(raw)) => {
                rsa_sha256_ssh_signature(&self.public_key, &raw)
            }
            _ => Err(Error::malformed(
                "platform signature",
                "signature type does not match the key type",
            )),
        }
    }
}

/// Keeps a user-supplied label safe for use in OS key names.
#[cfg_attr(
    not(any(target_os = "macos", target_os = "ios", windows)),
    allow(dead_code)
)]
pub(crate) fn sanitize_label(label: &str) -> String {
    let cleaned: String = label
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '@'))
        .take(64)
        .collect();
    if cleaned.trim().is_empty() {
        "ssh key".to_owned()
    } else {
        cleaned
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
