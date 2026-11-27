// SPDX-License-Identifier: GPL-3.0-or-later
//! Cryptographic primitives used by the vault.
//!
//! * **AEAD:** XChaCha20-Poly1305 with random 192-bit nonces (safe to generate randomly).
//! * **KDF:** Argon2id for password-derived key-encryption keys.
//! * **Blob format:** `version (1) || nonce (24) || ciphertext || tag (16)`.
//!
//! All key material is held in [`Zeroizing`] buffers.

use crate::error::VaultError;
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Current blob format version.
pub const BLOB_VERSION: u8 = 1;
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
/// Length of all symmetric keys.
pub const KEY_LEN: usize = 32;

/// Fills `buf` with cryptographically secure random bytes from the OS.
pub fn random_bytes(buf: &mut [u8]) -> Result<(), VaultError> {
    getrandom::fill(buf).map_err(|e| VaultError::Crypto(format!("os rng failure: {e}")))
}

/// A 256-bit symmetric key that is wiped from memory on drop.
#[derive(Clone)]
pub struct SymmetricKey(Zeroizing<[u8; KEY_LEN]>);

impl std::fmt::Debug for SymmetricKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SymmetricKey(<redacted>)")
    }
}

impl SymmetricKey {
    /// Generates a fresh random key.
    pub fn generate() -> Result<Self, VaultError> {
        let mut k = Zeroizing::new([0u8; KEY_LEN]);
        random_bytes(k.as_mut())?;
        Ok(Self(k))
    }

    /// Wraps existing key bytes.
    #[must_use]
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Builds a key from a slice, failing if the length is wrong.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, VaultError> {
        let arr: [u8; KEY_LEN] = bytes
            .try_into()
            .map_err(|_| VaultError::Crypto("invalid key length".into()))?;
        Ok(Self::from_bytes(arr))
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(self.0.as_ref().into())
    }

    /// Encrypts `plaintext`, authenticating `aad`.
    pub fn seal(&self, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, VaultError> {
        let mut nonce = [0u8; NONCE_LEN];
        random_bytes(&mut nonce)?;
        let ct = self
            .cipher()
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| VaultError::Crypto("encryption failed".into()))?;
        let mut out = Vec::with_capacity(1 + NONCE_LEN + ct.len());
        out.push(BLOB_VERSION);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        Ok(out)
    }

    /// Decrypts a blob produced by [`Self::seal`] with the same `aad`.
    pub fn open(&self, aad: &[u8], blob: &[u8]) -> Result<Zeroizing<Vec<u8>>, VaultError> {
        if blob.len() < 1 + NONCE_LEN + TAG_LEN {
            return Err(VaultError::Corrupt("ciphertext too short".into()));
        }
        if blob[0] != BLOB_VERSION {
            return Err(VaultError::Corrupt(format!(
                "unsupported blob version {}",
                blob[0]
            )));
        }
        let nonce = XNonce::from_slice(&blob[1..=NONCE_LEN]);
        self.cipher()
            .decrypt(
                nonce,
                Payload {
                    msg: &blob[1 + NONCE_LEN..],
                    aad,
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| VaultError::Authentication)
    }
}

/// Argon2id parameters, stored alongside password-wrapped keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KdfParams {
    /// Memory cost in KiB.
    pub memory_kib: u32,
    /// Iterations.
    pub iterations: u32,
    /// Parallelism (lanes).
    pub parallelism: u32,
    /// Random 16-byte salt, hex encoded.
    pub salt: String,
}

impl KdfParams {
    /// Recommended parameters for desktop devices (256 MiB, t=3).
    pub fn desktop() -> Result<Self, VaultError> {
        Self::with_cost(256 * 1024, 3, 1)
    }

    /// Recommended parameters for phones and tablets (64 MiB, t=3).
    pub fn mobile() -> Result<Self, VaultError> {
        Self::with_cost(64 * 1024, 3, 1)
    }

    /// Low-cost parameters for tests only.
    #[doc(hidden)]
    pub fn insecure_for_tests() -> Self {
        Self {
            memory_kib: 64,
            iterations: 1,
            parallelism: 1,
            salt: "00112233445566778899aabbccddeeff".into(),
        }
    }

    /// Parameters with a fresh random salt.
    pub fn with_cost(
        memory_kib: u32,
        iterations: u32,
        parallelism: u32,
    ) -> Result<Self, VaultError> {
        let mut salt = [0u8; 16];
        random_bytes(&mut salt)?;
        Ok(Self {
            memory_kib,
            iterations,
            parallelism,
            salt: hex::encode(salt),
        })
    }

    /// Rejects parameters that are too weak to be accepted from a (possibly attacker-modified)
    /// header. Test parameters are allowed only in debug builds.
    pub fn check_strength(&self) -> Result<(), VaultError> {
        let min_mem = if cfg!(debug_assertions) { 8 } else { 19 * 1024 };
        if self.memory_kib < min_mem || self.iterations < 1 || self.parallelism < 1 {
            return Err(VaultError::Corrupt("KDF parameters below minimum".into()));
        }
        if self.memory_kib > 4 * 1024 * 1024 || self.iterations > 64 || self.parallelism > 16 {
            return Err(VaultError::Corrupt("KDF parameters above maximum".into()));
        }
        Ok(())
    }

    /// Derives a 256-bit key-encryption key from `password`.
    pub fn derive(&self, password: &[u8]) -> Result<SymmetricKey, VaultError> {
        self.check_strength()?;
        let salt = hex::decode(&self.salt).map_err(|_| VaultError::Corrupt("bad salt".into()))?;
        if salt.len() < 16 {
            return Err(VaultError::Corrupt("salt too short".into()));
        }
        let params = Params::new(
            self.memory_kib,
            self.iterations,
            self.parallelism,
            Some(KEY_LEN),
        )
        .map_err(|e| VaultError::Crypto(format!("argon2 params: {e}")))?;
        let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut out = Zeroizing::new([0u8; KEY_LEN]);
        argon
            .hash_password_into(password, &salt, out.as_mut())
            .map_err(|e| VaultError::Crypto(format!("argon2: {e}")))?;
        Ok(SymmetricKey(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip() -> Result<(), VaultError> {
        let k = SymmetricKey::generate()?;
        let blob = k.seal(b"aad", b"hello")?;
        assert_eq!(&*k.open(b"aad", &blob)?, b"hello");
        Ok(())
    }

    #[test]
    fn tampering_is_detected() -> Result<(), VaultError> {
        let k = SymmetricKey::generate()?;
        let mut blob = k.seal(b"aad", b"hello")?;
        assert!(matches!(
            k.open(b"other", &blob),
            Err(VaultError::Authentication)
        ));
        let last = blob.len() - 1;
        blob[last] ^= 1;
        assert!(matches!(
            k.open(b"aad", &blob),
            Err(VaultError::Authentication)
        ));
        let other = SymmetricKey::generate()?;
        let blob = k.seal(b"aad", b"hello")?;
        assert!(matches!(
            other.open(b"aad", &blob),
            Err(VaultError::Authentication)
        ));
        assert!(matches!(
            k.open(b"aad", &blob[..10]),
            Err(VaultError::Corrupt(_))
        ));
        Ok(())
    }

    #[test]
    fn nonces_are_unique() -> Result<(), VaultError> {
        let k = SymmetricKey::generate()?;
        let a = k.seal(b"", b"same")?;
        let b = k.seal(b"", b"same")?;
        assert_ne!(a, b);
        Ok(())
    }

    #[test]
    fn kdf_is_deterministic_per_salt() -> Result<(), VaultError> {
        let p = KdfParams::insecure_for_tests();
        let a = p.derive(b"correct horse")?;
        let b = p.derive(b"correct horse")?;
        let c = p.derive(b"battery staple")?;
        assert_eq!(a.as_bytes(), b.as_bytes());
        assert_ne!(a.as_bytes(), c.as_bytes());
        let mut weak = p.clone();
        weak.memory_kib = 1;
        assert!(weak.derive(b"x").is_err());
        Ok(())
    }
}
