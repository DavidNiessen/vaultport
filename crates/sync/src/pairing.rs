// SPDX-License-Identifier: GPL-3.0-or-later
//! Pairing a new device with an existing vault.
//!
//! # Flow
//!
//! 1. Device A (unlocked) calls [`create_offer`]. It shows the [`PairingPayload`] as a QR code
//!    ([`PairingPayload::to_qr_string`]) and, **separately**, the 8-character pairing code
//!    ([`PairingOffer::code`], e.g. `K7QM-2XPA`).
//! 2. Device B scans the QR code, the user types the code, and B calls [`accept_offer`]. B
//!    obtains the vault id, the vault key, the backend configuration and (optionally) the
//!    backend secret, creates its replica with `Vault::create_replica` and syncs.
//!
//! # Cryptography
//!
//! `transfer_key = Argon2id(code, salt')` with `salt' = salt || SHA-256(binding)[..16]`, where
//! `binding = vault id | expiry | backend config`. The vault key is wrapped with
//! [`vault::Vault::export_wrapped_key`]; the optional backend secret is sealed with the same
//! transfer key. Because the binding is part of the salt, changing the expiry or the backend
//! configuration in the QR code makes the code useless.
//!
//! # Threat model
//!
//! A full PAKE is not needed because both halves are displayed locally on device A:
//!
//! * An attacker needs **both** the QR payload and the code. The QR code alone does not
//!   reveal the vault key without guessing the code: 40 bits of OS randomness, each guess
//!   costing one Argon2id evaluation (64 MiB by default), i.e. far beyond practical offline
//!   brute force for a vault key that is only useful together with access to the storage.
//! * Codes expire after [`PAIRING_TTL_MS`] (10 minutes); device B refuses expired offers.
//!   Expiry is enforced by honest clients (an attacker who photographed the QR code can
//!   ignore it, which is why the code entropy and KDF cost matter, see above).
//! * Someone who sees both the QR code and the code (shoulder surfing, screen recording)
//!   obtains the vault key. Users must pair in private; the app should hide both after use.
//! * Backend secrets are only included if the app passes them explicitly.

use crate::error::SyncError;
use crate::keys::{now_ms, random};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use uuid::Uuid;
use vault::{KdfParams, SymmetricKey, Vault, unwrap_exported_key};
use zeroize::{Zeroize, Zeroizing};

/// Lifetime of a pairing offer.
pub const PAIRING_TTL_MS: i64 = 10 * 60 * 1000;
/// Prefix of the QR code string.
pub const QR_PREFIX: &str = "sshclient-pair:1:";
const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Argon2id cost of the transfer key (the salt is generated per offer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingKdf {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl Default for PairingKdf {
    /// 64 MiB, t=3: affordable on phones.
    fn default() -> Self {
        Self {
            memory_kib: 64 * 1024,
            iterations: 3,
            parallelism: 1,
        }
    }
}

/// The QR code content. Contains no secret that is usable without the pairing code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingPayload {
    pub version: u32,
    pub vault_id: Uuid,
    /// Non-secret backend configuration chosen by the app (type, URL, bucket, ...).
    pub backend: serde_json::Value,
    /// The vault key wrapped with the transfer key.
    pub wrapped_vk: String,
    /// Random 16-byte salt (hex).
    pub salt: String,
    pub kdf: PairingKdf,
    /// Unix ms after which the offer is refused.
    pub expires_at: i64,
    /// Backend secret (password, token) sealed with the transfer key (base64url).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_secret: Option<String>,
}

/// An offer created on device A.
pub struct PairingOffer {
    pub payload: PairingPayload,
    code: Zeroizing<String>,
}

impl fmt::Debug for PairingOffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PairingOffer")
            .field("payload", &self.payload)
            .field("code", &"<redacted>")
            .finish()
    }
}

impl PairingOffer {
    /// The pairing code to display, formatted as `XXXX-XXXX`.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }
}

/// What device B obtains.
pub struct AcceptedPairing {
    pub vault_id: Uuid,
    pub vault_key: SymmetricKey,
    pub backend: serde_json::Value,
    pub backend_secret: Option<Zeroizing<Vec<u8>>>,
}

impl fmt::Debug for AcceptedPairing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AcceptedPairing")
            .field("vault_id", &self.vault_id)
            .field("backend", &self.backend)
            .finish_non_exhaustive()
    }
}

fn generate_code() -> Result<Zeroizing<String>, SyncError> {
    let bytes = Zeroizing::new(random::<5>()?);
    let mut bits = 0u64;
    for b in bytes.iter() {
        bits = (bits << 8) | u64::from(*b);
    }
    let mut code = Zeroizing::new(String::with_capacity(9));
    for i in 0..8 {
        if i == 4 {
            code.push('-');
        }
        let idx = ((bits >> (35 - 5 * i)) & 0x1f) as usize;
        code.push(char::from(ALPHABET[idx]));
    }
    bits.zeroize();
    Ok(code)
}

/// Normalizes user input: case-insensitive, ignores spaces and dashes, maps 0/1/8 to O/I/B.
fn normalize_code(input: &str) -> Result<Zeroizing<String>, SyncError> {
    let mut out = Zeroizing::new(String::with_capacity(8));
    for c in input.chars() {
        let c = match c.to_ascii_uppercase() {
            ' ' | '-' => continue,
            '0' => 'O',
            '1' => 'I',
            '8' => 'B',
            c => c,
        };
        if !c.is_ascii() || !ALPHABET.contains(&(c as u8)) {
            return Err(SyncError::Authentication);
        }
        out.push(c);
    }
    if out.len() != 8 {
        return Err(SyncError::Authentication);
    }
    Ok(out)
}

fn binding_salt(
    salt_hex: &str,
    vault_id: Uuid,
    expires_at: i64,
    backend: &serde_json::Value,
) -> Result<String, SyncError> {
    let salt = hex::decode(salt_hex).map_err(|_| SyncError::Format("bad salt".into()))?;
    if salt.len() != 16 {
        return Err(SyncError::Format("bad salt".into()));
    }
    let backend = serde_json::to_vec(backend).map_err(|e| SyncError::Format(e.to_string()))?;
    let mut h = Sha256::new();
    h.update(b"ssh-client/pairing/v1|");
    h.update(vault_id.as_bytes());
    h.update(b"|");
    h.update(expires_at.to_be_bytes());
    h.update(b"|");
    h.update(&backend);
    let digest = h.finalize();
    let mut full = salt;
    full.extend_from_slice(&digest[..16]);
    Ok(hex::encode(full))
}

fn transfer_key(payload: &PairingPayload, code: &str) -> Result<SymmetricKey, SyncError> {
    let kdf = KdfParams {
        memory_kib: payload.kdf.memory_kib,
        iterations: payload.kdf.iterations,
        parallelism: payload.kdf.parallelism,
        salt: binding_salt(
            &payload.salt,
            payload.vault_id,
            payload.expires_at,
            &payload.backend,
        )?,
    };
    Ok(kdf.derive(normalize_code(code)?.as_bytes())?)
}

fn secret_aad(vault_id: Uuid) -> String {
    format!("ssh-client/pairing/secret/v1|{vault_id}")
}

/// Creates a pairing offer for an unlocked vault (device A).
pub fn create_offer(
    vault: &Vault,
    backend: serde_json::Value,
    backend_secret: Option<&[u8]>,
    kdf: PairingKdf,
) -> Result<PairingOffer, SyncError> {
    create_offer_at(vault, backend, backend_secret, kdf, now_ms())
}

/// [`create_offer`] with an explicit current time (tests).
pub fn create_offer_at(
    vault: &Vault,
    backend: serde_json::Value,
    backend_secret: Option<&[u8]>,
    kdf: PairingKdf,
    now_ms: i64,
) -> Result<PairingOffer, SyncError> {
    let code = generate_code()?;
    let mut payload = PairingPayload {
        version: 1,
        vault_id: vault.vault_id(),
        backend,
        wrapped_vk: String::new(),
        salt: hex::encode(random::<16>()?),
        kdf,
        expires_at: now_ms + PAIRING_TTL_MS,
        backend_secret: None,
    };
    let key = transfer_key(&payload, &code)?;
    payload.wrapped_vk = vault.export_wrapped_key(&key)?;
    if let Some(secret) = backend_secret {
        let sealed = key.seal(secret_aad(payload.vault_id).as_bytes(), secret)?;
        payload.backend_secret = Some(URL_SAFE_NO_PAD.encode(sealed));
    }
    Ok(PairingOffer { payload, code })
}

/// Accepts an offer on device B.
pub fn accept_offer(payload: &PairingPayload, code: &str) -> Result<AcceptedPairing, SyncError> {
    accept_offer_at(payload, code, now_ms())
}

/// [`accept_offer`] with an explicit current time (tests).
pub fn accept_offer_at(
    payload: &PairingPayload,
    code: &str,
    now_ms: i64,
) -> Result<AcceptedPairing, SyncError> {
    if payload.version != 1 {
        return Err(SyncError::Format(format!(
            "unsupported pairing version {}",
            payload.version
        )));
    }
    if now_ms > payload.expires_at || payload.expires_at - now_ms > PAIRING_TTL_MS + 60_000 {
        return Err(SyncError::Expired);
    }
    let key = transfer_key(payload, code)?;
    let vault_key = unwrap_exported_key(&key, &payload.wrapped_vk, payload.vault_id)?;
    let backend_secret = match &payload.backend_secret {
        None => None,
        Some(s) => {
            let sealed = URL_SAFE_NO_PAD
                .decode(s)
                .map_err(|_| SyncError::Format("bad backend secret".into()))?;
            let plain = key.open(secret_aad(payload.vault_id).as_bytes(), &sealed)?;
            Some(Zeroizing::new(plain.to_vec()))
        }
    };
    Ok(AcceptedPairing {
        vault_id: payload.vault_id,
        vault_key,
        backend: payload.backend.clone(),
        backend_secret,
    })
}

impl PairingPayload {
    /// Encodes the payload for a QR code.
    pub fn to_qr_string(&self) -> Result<String, SyncError> {
        let json = serde_json::to_vec(self).map_err(|e| SyncError::Format(e.to_string()))?;
        Ok(format!("{QR_PREFIX}{}", URL_SAFE_NO_PAD.encode(json)))
    }

    /// Decodes a scanned QR code.
    pub fn from_qr_string(s: &str) -> Result<Self, SyncError> {
        let bad = || SyncError::Format("not a pairing code".into());
        let b64 = s.trim().strip_prefix(QR_PREFIX).ok_or_else(bad)?;
        if b64.len() > 16 * 1024 {
            return Err(bad());
        }
        let json = URL_SAFE_NO_PAD.decode(b64).map_err(|_| bad())?;
        serde_json::from_slice(&json).map_err(|_| bad())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn codes_are_well_formed_and_random() {
        let a = generate_code().unwrap();
        let b = generate_code().unwrap();
        assert_eq!(a.len(), 9);
        assert_eq!(a.as_bytes()[4], b'-');
        assert_ne!(*a, *b);
        assert_eq!(normalize_code(&a.to_lowercase()).unwrap().len(), 8);
        assert_eq!(&*normalize_code("abcd efg0").unwrap(), "ABCDEFGO");
        assert!(normalize_code("ABC").is_err());
        assert!(normalize_code("ABCDEFG9").is_err());
    }
}
