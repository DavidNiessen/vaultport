// SPDX-License-Identifier: GPL-3.0-or-later
//! Access to the operating system's secret store.
//!
//! The device key that wraps a vault key is stored in:
//! * macOS / iOS: Keychain (`kSecClassGenericPassword`, this-device-only on iOS)
//! * Windows: Credential Manager (DPAPI-protected)
//! * Linux: Secret Service (GNOME Keyring, KWallet) over D-Bus
//! * Android: Android Keystore through the native secure-store plugin

use crate::error::{CmdError, CmdResult, ErrorCode};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use vault::SymmetricKey;
use zeroize::Zeroizing;

const SERVICE: &str = "dev.niessen.sshclient";

fn account(vault_id: &uuid::Uuid) -> String {
    format!("vault-device-key:{vault_id}")
}

/// Stores a device key for a vault.
#[cfg(not(target_os = "android"))]
pub fn store_device_key(vault_id: &uuid::Uuid, key: &SymmetricKey) -> CmdResult<()> {
    let entry = keyring::Entry::new(SERVICE, &account(vault_id)).map_err(map_err)?;
    let encoded = Zeroizing::new(STANDARD.encode(key.as_bytes()));
    entry.set_password(&encoded).map_err(map_err)
}

/// Loads the device key for a vault, if one was stored.
#[cfg(not(target_os = "android"))]
pub fn load_device_key(vault_id: &uuid::Uuid) -> CmdResult<Option<SymmetricKey>> {
    let entry = keyring::Entry::new(SERVICE, &account(vault_id)).map_err(map_err)?;
    match entry.get_password() {
        Ok(s) => {
            let s = Zeroizing::new(s);
            let bytes = Zeroizing::new(
                STANDARD
                    .decode(s.as_bytes())
                    .map_err(|_| CmdError::internal("corrupt device key in key store"))?,
            );
            Ok(Some(SymmetricKey::from_slice(&bytes)?))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(map_err(e)),
    }
}

/// Removes the device key for a vault.
#[cfg(not(target_os = "android"))]
pub fn delete_device_key(vault_id: &uuid::Uuid) -> CmdResult<()> {
    let entry = keyring::Entry::new(SERVICE, &account(vault_id)).map_err(map_err)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(map_err(e)),
    }
}

#[cfg(not(target_os = "android"))]
fn map_err(e: keyring::Error) -> CmdError {
    CmdError::new(
        ErrorCode::Unsupported,
        format!("the system key store is unavailable: {e}"),
    )
}

// Android: the device key is stored by the `secure-store` mobile plugin; until it is wired the
// app falls back to password-only vaults.
#[cfg(target_os = "android")]
pub fn store_device_key(_vault_id: &uuid::Uuid, _key: &SymmetricKey) -> CmdResult<()> {
    Err(CmdError::new(
        ErrorCode::Unsupported,
        "device key storage is provided by the secure-store plugin",
    ))
}

#[cfg(target_os = "android")]
pub fn load_device_key(_vault_id: &uuid::Uuid) -> CmdResult<Option<SymmetricKey>> {
    Ok(None)
}

#[cfg(target_os = "android")]
pub fn delete_device_key(_vault_id: &uuid::Uuid) -> CmdResult<()> {
    Ok(())
}
