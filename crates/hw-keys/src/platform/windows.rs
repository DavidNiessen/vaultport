// SPDX-License-Identifier: GPL-3.0-or-later
//! Windows Hello keys through `Windows.Security.Credentials.KeyCredentialManager` (WinRT,
//! safe bindings from the `windows` crate; no `unsafe` here).
//!
//! `KeyCredentialManager` creates an RSA-2048 key in the Microsoft Passport key storage
//! provider, which is TPM-backed when a TPM is present (software otherwise, decided by
//! the OS and Hello policy). Every `RequestSignAsync` shows the Windows Hello prompt
//! (face, fingerprint or Hello PIN) and returns an RSASSA-PKCS1-v1_5 / SHA-256 signature,
//! which we encode as `rsa-sha2-256`. The key's public half is retrieved as a
//! `BCRYPT_RSAPUBLIC_BLOB`.
//!
//! [`AccessPolicy`] cannot be expressed here: Windows Hello decides which gestures are
//! accepted. The `platform_ref` is the credential name.
//!
//! Desktop apps: the Hello dialog is owned by the system and may open behind the app
//! window; the app should bring itself to the foreground before calling `sign`.

use windows::Security::Credentials::{
    KeyCredential, KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus,
};
use windows::Security::Cryptography::Core::CryptographicPublicKeyBlobType;
use windows::Security::Cryptography::CryptographicBuffer;
use windows::Storage::Streams::IBuffer;
use windows::core::{Array, HSTRING};

use ssh_key::PublicKey;

use super::{AccessPolicy, PlatformKeyBackend, PlatformKeyInfo, PlatformSignature, sanitize_label};
use crate::error::{Error, Result, fill_random};
use crate::pubkey::rsa_public_key_from_bcrypt_blob;
use crate::task::blocking;

/// Windows Hello backend.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsHelloBackend {
    _private: (),
}

impl WindowsHelloBackend {
    /// Creates the backend.
    pub const fn new() -> Self {
        Self { _private: () }
    }
}

fn win_error(error: &windows::core::Error, context: &str) -> Error {
    Error::Platform(format!(
        "{context} failed (HRESULT {:#010x})",
        error.code().0
    ))
}

fn map_status(status: KeyCredentialStatus) -> Result<()> {
    match status {
        KeyCredentialStatus::Success => Ok(()),
        KeyCredentialStatus::UserCanceled | KeyCredentialStatus::UserPrefersPassword => {
            Err(Error::Cancelled)
        }
        KeyCredentialStatus::NotFound => Err(Error::KeyNotFound),
        KeyCredentialStatus::CredentialAlreadyExists => Err(Error::AlreadyExists),
        KeyCredentialStatus::SecurityDeviceLocked => Err(Error::Platform(
            "the security device is locked; unlock Windows Hello and retry".to_owned(),
        )),
        _ => Err(Error::Platform(
            "Windows Hello reported an unknown error".to_owned(),
        )),
    }
}

fn buffer_to_vec(buffer: &IBuffer) -> Result<Vec<u8>> {
    let mut out = Array::<u8>::new();
    CryptographicBuffer::CopyToByteArray(buffer, &mut out)
        .map_err(|e| win_error(&e, "reading a buffer"))?;
    Ok(out.to_vec())
}

fn public_key_of(credential: &KeyCredential) -> Result<PublicKey> {
    let buffer = credential
        .RetrievePublicKeyWithBlobType(CryptographicPublicKeyBlobType::BCryptPublicKey)
        .map_err(|e| win_error(&e, "retrieving the public key"))?;
    rsa_public_key_from_bcrypt_blob(&buffer_to_vec(&buffer)?)
}

fn open(name: &str) -> Result<KeyCredential> {
    let result = KeyCredentialManager::OpenAsync(&HSTRING::from(name))
        .and_then(|op| op.join())
        .map_err(|e| win_error(&e, "opening the credential"))?;
    map_status(
        result
            .Status()
            .map_err(|e| win_error(&e, "opening the credential"))?,
    )?;
    result
        .Credential()
        .map_err(|e| win_error(&e, "opening the credential"))
}

fn is_supported() -> bool {
    KeyCredentialManager::IsSupportedAsync()
        .and_then(|op| op.join())
        .unwrap_or(false)
}

fn create_blocking(label: &str) -> Result<PlatformKeyInfo> {
    if !is_supported() {
        return Err(Error::Unsupported(
            "Windows Hello is not set up on this device",
        ));
    }
    let mut suffix = [0u8; 8];
    fill_random(&mut suffix)?;
    let name = format!(
        "ssh-client {} {}",
        sanitize_label(label),
        hex::encode(suffix)
    );
    let result = KeyCredentialManager::RequestCreateAsync(
        &HSTRING::from(name.as_str()),
        KeyCredentialCreationOption::FailIfExists,
    )
    .and_then(|op| op.join())
    .map_err(|e| win_error(&e, "creating the credential"))?;
    map_status(
        result
            .Status()
            .map_err(|e| win_error(&e, "creating the credential"))?,
    )?;
    let credential = result
        .Credential()
        .map_err(|e| win_error(&e, "creating the credential"))?;
    Ok(PlatformKeyInfo {
        public_key: public_key_of(&credential)?,
        platform_ref: name,
    })
}

fn sign_blocking(name: &str, data: &[u8]) -> Result<PlatformSignature> {
    let credential = open(name)?;
    let buffer =
        CryptographicBuffer::CreateFromByteArray(data).map_err(|e| win_error(&e, "signing"))?;
    let result = credential
        .RequestSignAsync(&buffer)
        .and_then(|op| op.join())
        .map_err(|e| win_error(&e, "signing"))?;
    map_status(result.Status().map_err(|e| win_error(&e, "signing"))?)?;
    let signature = result.Result().map_err(|e| win_error(&e, "signing"))?;
    Ok(PlatformSignature::RsaPkcs1Sha256(buffer_to_vec(
        &signature,
    )?))
}

#[async_trait::async_trait]
impl PlatformKeyBackend for WindowsHelloBackend {
    fn name(&self) -> &'static str {
        "Windows Hello"
    }

    async fn is_available(&self) -> bool {
        blocking(|| Ok(is_supported())).await.unwrap_or(false)
    }

    async fn create(&self, label: &str, _policy: AccessPolicy) -> Result<PlatformKeyInfo> {
        let label = label.to_owned();
        blocking(move || create_blocking(&label)).await
    }

    async fn sign(&self, platform_ref: &str, data: &[u8]) -> Result<PlatformSignature> {
        let name = platform_ref.to_owned();
        let data = data.to_vec();
        blocking(move || sign_blocking(&name, &data)).await
    }

    async fn delete(&self, platform_ref: &str) -> Result<()> {
        let name = platform_ref.to_owned();
        blocking(move || {
            // Fail with `KeyNotFound` rather than silently succeeding for unknown names.
            open(&name)?;
            KeyCredentialManager::DeleteAsync(&HSTRING::from(name.as_str()))
                .and_then(|op| op.join())
                .map_err(|e| win_error(&e, "deleting the credential"))
        })
        .await
    }

    async fn public_key(&self, platform_ref: &str) -> Result<PublicKey> {
        let name = platform_ref.to_owned();
        blocking(move || public_key_of(&open(&name)?)).await
    }
}
