// SPDX-License-Identifier: GPL-3.0-or-later
//! Key handling.
//!
//! The vault does not expose its key directly. The sync crate obtains it through the public
//! pairing API (wrap with an ephemeral random transfer key, unwrap again) and never uses it
//! directly for its own ciphertexts: every container format gets its own HKDF-SHA256 subkey
//! (`info = "vaultport/sync/<purpose>/v1"`, `salt = vault id`), so segment, export and
//! pairing ciphertexts are domain-separated from the vault's own records and ops.

use crate::error::SyncError;
use hkdf::Hkdf;
use sha2::Sha256;
use uuid::Uuid;
use vault::{SymmetricKey, Vault, unwrap_exported_key};
use zeroize::Zeroizing;

/// Purposes of derived keys.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Purpose {
    Segment,
    Export,
}

impl Purpose {
    fn info(self) -> &'static [u8] {
        match self {
            Self::Segment => b"vaultport/sync/segment/v1",
            Self::Export => b"vaultport/sync/export/v1",
        }
    }
}

/// Returns the vault key of an unlocked vault.
pub(crate) fn vault_key(vault: &Vault) -> Result<SymmetricKey, SyncError> {
    let transfer = SymmetricKey::generate()?;
    let wrapped = vault.export_wrapped_key(&transfer)?;
    Ok(unwrap_exported_key(&transfer, &wrapped, vault.vault_id())?)
}

/// Derives a purpose-specific subkey from the vault key.
pub(crate) fn subkey(vk: &SymmetricKey, vault_id: Uuid, purpose: Purpose) -> SymmetricKey {
    let hk = Hkdf::<Sha256>::new(Some(vault_id.as_bytes()), vk.as_bytes());
    let mut out = Zeroizing::new([0u8; 32]);
    // 32 bytes is far below the HKDF-SHA256 output limit, so this cannot fail.
    hk.expand(purpose.info(), out.as_mut())
        .expect("32-byte HKDF output is always valid");
    SymmetricKey::from_bytes(*out)
}

/// Fills a buffer with OS randomness.
pub(crate) fn random<const N: usize>() -> Result<[u8; N], SyncError> {
    let mut b = [0u8; N];
    vault::crypto::random_bytes(&mut b)?;
    Ok(b)
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}
