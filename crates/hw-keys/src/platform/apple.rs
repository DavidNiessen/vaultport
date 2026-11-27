// SPDX-License-Identifier: GPL-3.0-or-later
//! Secure Enclave keys on macOS and iOS through `security-framework` (no `unsafe` here).
//!
//! Keys are P-256, created with `SecKeyCreateRandomKey` and `kSecAttrTokenIDSecureEnclave`
//! as permanent items in the data protection keychain, with the access control
//! `privateKeyUsage | <policy>` and `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`, so they
//! never leave the device and every signature requires Touch ID / Face ID (or the
//! passcode, per [`AccessPolicy`]). Signing uses
//! `kSecKeyAlgorithmECDSASignatureMessageX962SHA256` (SHA-256 + DER output).
//!
//! The `platform_ref` is the hex-encoded `kSecAttrApplicationLabel` (SHA-1 of the public
//! key), which uniquely identifies the key in the keychain.
//!
//! Requirements for the app: it must be code signed with a `keychain-access-groups`
//! entitlement (errors otherwise with `errSecMissingEntitlement`), and on iOS needs
//! `NSFaceIDUsageDescription` in `Info.plist`. The biometric prompt is shown by the system
//! with a generic reason; it runs on a blocking thread, not the UI thread.

use core_foundation::base::CFOptionFlags;
use security_framework::access_control::{ProtectionMode, SecAccessControl};
use security_framework::item::{
    ItemClass, ItemSearchOptions, KeyClass, Location, Reference, SearchResult,
};
use security_framework::key::{Algorithm, GenerateKeyOptions, KeyType, SecKey, Token};
use security_framework_sys::access_control::{
    kSecAccessControlBiometryAny, kSecAccessControlBiometryCurrentSet,
    kSecAccessControlPrivateKeyUsage, kSecAccessControlUserPresence,
};
use ssh_key::PublicKey;

use super::{AccessPolicy, PlatformKeyBackend, PlatformKeyInfo, PlatformSignature, sanitize_label};
use crate::error::{Error, Result};
use crate::pubkey::ecdsa_p256_public_key;
use crate::task::blocking;

const ERR_SEC_USER_CANCELED: i64 = -128;
const ERR_SEC_AUTH_FAILED: i64 = -25293;
const ERR_SEC_ITEM_NOT_FOUND: i64 = -25300;
const ERR_SEC_MISSING_ENTITLEMENT: i64 = -34018;
const LA_ERROR_AUTHENTICATION_FAILED: i64 = -1;
const LA_ERROR_USER_CANCEL: i64 = -2;
const LA_ERROR_SYSTEM_CANCEL: i64 = -4;
const LA_ERROR_APP_CANCEL: i64 = -9;

/// Secure Enclave backend.
#[derive(Debug, Default, Clone, Copy)]
pub struct SecureEnclaveBackend {
    _private: (),
}

impl SecureEnclaveBackend {
    /// Creates the backend.
    pub const fn new() -> Self {
        Self { _private: () }
    }
}

fn map_status(code: i64, context: &str) -> Error {
    match code {
        ERR_SEC_USER_CANCELED
        | LA_ERROR_USER_CANCEL
        | LA_ERROR_SYSTEM_CANCEL
        | LA_ERROR_APP_CANCEL
        | LA_ERROR_AUTHENTICATION_FAILED
        | ERR_SEC_AUTH_FAILED => Error::Cancelled,
        ERR_SEC_ITEM_NOT_FOUND => Error::KeyNotFound,
        ERR_SEC_MISSING_ENTITLEMENT => {
            Error::Platform("the app is missing the keychain-access-groups entitlement".to_owned())
        }
        other => Error::Platform(format!("{context} failed (status {other})")),
    }
}

fn cf_error(error: &core_foundation::error::CFError, context: &str) -> Error {
    map_status(error.code() as i64, context)
}

fn access_flags(policy: AccessPolicy) -> CFOptionFlags {
    kSecAccessControlPrivateKeyUsage
        | match policy {
            AccessPolicy::BiometryCurrentSet => kSecAccessControlBiometryCurrentSet,
            AccessPolicy::BiometryAny => kSecAccessControlBiometryAny,
            AccessPolicy::UserPresence => kSecAccessControlUserPresence,
        }
}

fn public_key_of(key: &SecKey) -> Result<PublicKey> {
    let public = key
        .public_key()
        .ok_or_else(|| Error::Platform("Secure Enclave key has no public key".to_owned()))?;
    let encoded = public
        .external_representation()
        .ok_or_else(|| Error::Platform("cannot export the public key".to_owned()))?;
    // X9.63 uncompressed point: 04 || x || y.
    ecdsa_p256_public_key(encoded.bytes())
}

fn create_blocking(label: &str, policy: AccessPolicy) -> Result<PlatformKeyInfo> {
    let access = SecAccessControl::create_with_protection(
        Some(ProtectionMode::AccessibleWhenUnlockedThisDeviceOnly),
        access_flags(policy),
    )
    .map_err(|e| map_status(i64::from(e.code()), "creating the access control"))?;

    let mut options = GenerateKeyOptions::default();
    options
        .set_key_type(KeyType::ec_sec_prime_random())
        .set_size_in_bits(256)
        .set_label(format!("vaultport: {}", sanitize_label(label)))
        .set_token(Token::SecureEnclave)
        .set_location(Location::DataProtectionKeychain)
        .set_access_control(access);
    let key = SecKey::new(&options).map_err(|e| cf_error(&e, "creating the key"))?;

    let label = key
        .application_label()
        .ok_or_else(|| Error::Platform("key has no application label".to_owned()))?;
    Ok(PlatformKeyInfo {
        platform_ref: hex::encode(label),
        public_key: public_key_of(&key)?,
    })
}

fn find_key(platform_ref: &str) -> Result<SecKey> {
    let label = hex::decode(platform_ref).map_err(|_| Error::KeyNotFound)?;
    let mut search = ItemSearchOptions::new();
    search
        .class(ItemClass::key())
        .key_class(KeyClass::private())
        .application_label(&label)
        .load_refs(true)
        .limit(1);
    // Secure Enclave keys live in the data protection keychain (the only one on iOS).
    #[cfg(target_os = "macos")]
    search.ignore_legacy_keychains();
    let results = search
        .search()
        .map_err(|e| map_status(i64::from(e.code()), "looking up the key"))?;
    results
        .into_iter()
        .find_map(|r| match r {
            SearchResult::Ref(Reference::Key(key)) => Some(key),
            _ => None,
        })
        .ok_or(Error::KeyNotFound)
}

#[async_trait::async_trait]
impl PlatformKeyBackend for SecureEnclaveBackend {
    fn name(&self) -> &'static str {
        "Secure Enclave"
    }

    async fn is_available(&self) -> bool {
        // There is no cheap probe without creating a key; Apple silicon Macs, T2 Macs and
        // every supported iPhone/iPad have a Secure Enclave. Creation reports precise errors.
        true
    }

    async fn create(&self, label: &str, policy: AccessPolicy) -> Result<PlatformKeyInfo> {
        let label = label.to_owned();
        blocking(move || create_blocking(&label, policy)).await
    }

    async fn sign(&self, platform_ref: &str, data: &[u8]) -> Result<PlatformSignature> {
        let platform_ref = platform_ref.to_owned();
        let data = data.to_vec();
        blocking(move || {
            let key = find_key(&platform_ref)?;
            key.create_signature(Algorithm::ECDSASignatureMessageX962SHA256, &data)
                .map(PlatformSignature::EcdsaDer)
                .map_err(|e| cf_error(&e, "signing"))
        })
        .await
    }

    async fn delete(&self, platform_ref: &str) -> Result<()> {
        let platform_ref = platform_ref.to_owned();
        blocking(move || {
            find_key(&platform_ref)?
                .delete()
                .map_err(|e| map_status(i64::from(e.code()), "deleting the key"))
        })
        .await
    }

    async fn public_key(&self, platform_ref: &str) -> Result<PublicKey> {
        let platform_ref = platform_ref.to_owned();
        blocking(move || public_key_of(&find_key(&platform_ref)?)).await
    }
}
