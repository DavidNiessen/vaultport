// SPDX-License-Identifier: GPL-3.0-or-later
//! Keychain commands: generate, import and enroll SSH keys. Private key material is parsed and
//! stored only in the Rust core; the UI receives public metadata.

use crate::error::{CmdError, CmdResult, ErrorCode};
use crate::state::AppState;
use serde::Deserialize;
use specta::Type;
use tauri::State;

#[derive(Debug, Clone, Copy, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GenerateAlgorithm {
    Ed25519,
    EcdsaP256,
    EcdsaP384,
    Rsa,
}

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GenerateKeyRequest {
    pub label: String,
    pub algorithm: GenerateAlgorithm,
    pub bits: Option<u32>,
    pub passphrase: Option<String>,
    pub save_passphrase: bool,
}

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportKeyRequest {
    pub label: String,
    pub private_key: String,
    pub passphrase: Option<String>,
    pub certificate: Option<String>,
    pub save_passphrase: bool,
}

impl std::fmt::Debug for ImportKeyRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImportKeyRequest")
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

fn engine_missing() -> CmdError {
    CmdError::new(
        ErrorCode::Unsupported,
        "key management requires the SSH engine",
    )
}

/// Generates a new key pair in the vault.
#[tauri::command]
#[specta::specta]
pub fn key_generate(_state: State<'_, AppState>, request: GenerateKeyRequest) -> CmdResult<String> {
    let _ = (
        request.label,
        request.algorithm,
        request.bits,
        request.save_passphrase,
    );
    drop(request.passphrase.map(zeroize::Zeroizing::new));
    Err(engine_missing())
}

/// Imports a private key (OpenSSH, PEM or PuTTY).
#[tauri::command]
#[specta::specta]
pub fn key_import(_state: State<'_, AppState>, request: ImportKeyRequest) -> CmdResult<String> {
    let _ = zeroize::Zeroizing::new(request.private_key.clone());
    Err(engine_missing())
}

/// Reads a key file chosen by the user (max 64 KiB).
#[tauri::command]
#[specta::specta]
pub fn read_key_file(path: String) -> CmdResult<String> {
    let p = std::path::Path::new(&path);
    let meta = std::fs::metadata(p)?;
    if !meta.is_file() || meta.len() > 64 * 1024 {
        return Err(CmdError::invalid("not a key file"));
    }
    String::from_utf8(std::fs::read(p)?).map_err(|_| CmdError::invalid("the key file is not text"))
}

/// Enrolls a FIDO2 security key.
#[tauri::command]
#[specta::specta]
pub fn key_enroll_fido2(_state: State<'_, AppState>, label: String) -> CmdResult<String> {
    let _ = label;
    Err(CmdError::new(
        ErrorCode::Unsupported,
        "FIDO2 enrollment requires the hardware key module",
    ))
}

/// Creates a non-exportable key in the device's secure hardware.
#[tauri::command]
#[specta::specta]
pub fn key_create_hardware(_state: State<'_, AppState>, label: String) -> CmdResult<String> {
    let _ = label;
    Err(CmdError::new(
        ErrorCode::Unsupported,
        "secure hardware keys are not available on this device",
    ))
}

/// Imports a key referenced by another client's configuration.
pub fn import_key_file(_state: &AppState, _key: &importers::ImportedKey) -> CmdResult<()> {
    Err(engine_missing())
}
