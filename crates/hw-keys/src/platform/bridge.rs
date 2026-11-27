// SPDX-License-Identifier: GPL-3.0-or-later
//! A [`PlatformKeyBackend`] whose operations are callbacks into the app.
//!
//! On Android the key lives in the Android Keystore (StrongBox when available) and is
//! managed by a Kotlin Tauri plugin owned by the app. The plugin is expected to:
//!
//! * **create**: generate an `EC` key with `KeyGenParameterSpec` (`secp256r1`,
//!   `PURPOSE_SIGN`, `DIGEST_SHA256`, `setUserAuthenticationRequired(true)` with
//!   `AUTH_BIOMETRIC_STRONG` and a zero timeout, `setInvalidatedByBiometricEnrollment` per
//!   [`AccessPolicy`], `setIsStrongBoxBacked(true)` with fallback) and return its alias as
//!   `platform_ref` and `publicKey.getEncoded()` (X.509 `SubjectPublicKeyInfo`);
//! * **sign**: run `Signature.getInstance("SHA256withECDSA")` through a `BiometricPrompt`
//!   `CryptoObject` over the given bytes and return the DER signature;
//! * **delete**: `KeyStore.deleteEntry(alias)`.
//!
//! The same adapter works for any other app-provided store (tests use a software key).

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use ssh_key::PublicKey;

use super::{AccessPolicy, PlatformKeyBackend, PlatformKeyInfo, PlatformSignature};
use crate::error::{Error, Result};
use crate::pubkey::ecdsa_p256_public_key;

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;
type CreateFn = dyn Fn(String, AccessPolicy) -> BoxFuture<Result<BridgeKey>> + Send + Sync;
type SignFn = dyn Fn(String, Vec<u8>) -> BoxFuture<Result<Vec<u8>>> + Send + Sync;
type DeleteFn = dyn Fn(String) -> BoxFuture<Result<()>> + Send + Sync;
type PublicKeyFn = dyn Fn(String) -> BoxFuture<Result<Vec<u8>>> + Send + Sync;

/// A key created by the app's key store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeKey {
    /// The store's reference (Android: the Keystore alias).
    pub platform_ref: String,
    /// P-256 public key as DER `SubjectPublicKeyInfo` or SEC1 point.
    pub public_key: Vec<u8>,
}

/// How the app's sign callback encodes ECDSA signatures.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BridgeSignatureEncoding {
    /// ASN.1 DER (Java `SHA256withECDSA`).
    #[default]
    Der,
    /// IEEE P1363 `r || s` (Java `SHA256withECDSAinP1363Format`).
    P1363,
}

/// Adapter turning app callbacks into a [`PlatformKeyBackend`] for P-256 keys.
#[derive(Clone)]
pub struct BridgeBackend {
    name: &'static str,
    encoding: BridgeSignatureEncoding,
    sign: Arc<SignFn>,
    create: Option<Arc<CreateFn>>,
    delete: Option<Arc<DeleteFn>>,
    public_key: Option<Arc<PublicKeyFn>>,
    available: bool,
}

impl fmt::Debug for BridgeBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BridgeBackend")
            .field("name", &self.name)
            .field("encoding", &self.encoding)
            .field("can_create", &self.create.is_some())
            .field("can_delete", &self.delete.is_some())
            .finish_non_exhaustive()
    }
}

impl BridgeBackend {
    /// Creates a backend named `name` (e.g. `"Android Keystore"`) whose `sign(platform_ref,
    /// data)` callback returns an ECDSA P-256 signature over `SHA-256(data)`.
    pub fn new<F, Fut>(name: &'static str, sign: F) -> Self
    where
        F: Fn(String, Vec<u8>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Vec<u8>>> + Send + 'static,
    {
        Self {
            name,
            encoding: BridgeSignatureEncoding::Der,
            sign: Arc::new(move |r, d| Box::pin(sign(r, d))),
            create: None,
            delete: None,
            public_key: None,
            available: true,
        }
    }

    /// Sets the key creation callback.
    #[must_use]
    pub fn with_create<F, Fut>(mut self, create: F) -> Self
    where
        F: Fn(String, AccessPolicy) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<BridgeKey>> + Send + 'static,
    {
        self.create = Some(Arc::new(move |l, p| Box::pin(create(l, p))));
        self
    }

    /// Sets the key deletion callback.
    #[must_use]
    pub fn with_delete<F, Fut>(mut self, delete: F) -> Self
    where
        F: Fn(String) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.delete = Some(Arc::new(move |r| Box::pin(delete(r))));
        self
    }

    /// Sets the public key lookup callback (returns SPKI DER or a SEC1 point).
    #[must_use]
    pub fn with_public_key<F, Fut>(mut self, lookup: F) -> Self
    where
        F: Fn(String) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Vec<u8>>> + Send + 'static,
    {
        self.public_key = Some(Arc::new(move |r| Box::pin(lookup(r))));
        self
    }

    /// Sets the signature encoding returned by the sign callback.
    #[must_use]
    pub const fn with_signature_encoding(mut self, encoding: BridgeSignatureEncoding) -> Self {
        self.encoding = encoding;
        self
    }

    /// Sets what [`PlatformKeyBackend::is_available`] reports (default `true`).
    #[must_use]
    pub const fn with_availability(mut self, available: bool) -> Self {
        self.available = available;
        self
    }
}

#[async_trait::async_trait]
impl PlatformKeyBackend for BridgeBackend {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn is_available(&self) -> bool {
        self.available
    }

    async fn create(&self, label: &str, policy: AccessPolicy) -> Result<PlatformKeyInfo> {
        let create = self
            .create
            .as_ref()
            .ok_or(Error::Unsupported("key creation through this bridge"))?;
        let key = create(label.to_owned(), policy).await?;
        if key.platform_ref.is_empty() {
            return Err(Error::Platform(
                "bridge returned an empty key reference".into(),
            ));
        }
        Ok(PlatformKeyInfo {
            public_key: ecdsa_p256_public_key(&key.public_key)?,
            platform_ref: key.platform_ref,
        })
    }

    async fn sign(&self, platform_ref: &str, data: &[u8]) -> Result<PlatformSignature> {
        let signature = (self.sign)(platform_ref.to_owned(), data.to_vec()).await?;
        Ok(match self.encoding {
            BridgeSignatureEncoding::Der => PlatformSignature::EcdsaDer(signature),
            BridgeSignatureEncoding::P1363 => PlatformSignature::EcdsaP1363(signature),
        })
    }

    async fn delete(&self, platform_ref: &str) -> Result<()> {
        let delete = self
            .delete
            .as_ref()
            .ok_or(Error::Unsupported("key deletion through this bridge"))?;
        delete(platform_ref.to_owned()).await
    }

    async fn public_key(&self, platform_ref: &str) -> Result<PublicKey> {
        let lookup = self
            .public_key
            .as_ref()
            .ok_or(Error::Unsupported("public key lookup through this bridge"))?;
        ecdsa_p256_public_key(&lookup(platform_ref.to_owned()).await?)
    }
}
