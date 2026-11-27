// SPDX-License-Identifier: GPL-3.0-or-later
//! Keychain commands: generate, import and enroll SSH keys. Private key material is parsed and
//! stored only in the Rust core; the UI receives public metadata.

use crate::commands::entities::{VAULT_CHANGED, VaultChanged};
use crate::error::{CmdError, CmdResult, ErrorCode};
use crate::net;
use crate::state::AppState;
use core_model::{KeyAlgorithm, KeyId, KeyMeta, KeyStorage};
use serde::Deserialize;
use specta::Type;
use ssh_engine::keys::{self as k, GenerateOptions, KeyError};
use tauri::{AppHandle, Emitter, State};
use zeroize::Zeroizing;

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

impl std::fmt::Debug for GenerateKeyRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GenerateKeyRequest")
            .field("label", &self.label)
            .field("algorithm", &self.algorithm)
            .finish_non_exhaustive()
    }
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

fn map_key(e: KeyError) -> CmdError {
    match e {
        KeyError::NeedsPassphrase => CmdError::new(
            ErrorCode::Authentication,
            "this key is encrypted: enter its passphrase",
        ),
        KeyError::WrongPassphrase => {
            CmdError::new(ErrorCode::Authentication, "wrong passphrase for this key")
        }
        other => CmdError::invalid(other.to_string()),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn notify(app: &AppHandle) {
    let _ = app.emit(VAULT_CHANGED, VaultChanged { kind: "key".into() });
}

/// Generates a new key pair in the vault.
#[tauri::command]
#[specta::specta]
pub async fn key_generate(
    app: AppHandle,
    state: State<'_, AppState>,
    request: GenerateKeyRequest,
) -> CmdResult<String> {
    let passphrase = request
        .passphrase
        .map(Zeroizing::new)
        .filter(|p| !p.is_empty());
    let algorithm = match request.algorithm {
        GenerateAlgorithm::Ed25519 => KeyAlgorithm::Ed25519,
        GenerateAlgorithm::EcdsaP256 => KeyAlgorithm::EcdsaP256,
        GenerateAlgorithm::EcdsaP384 => KeyAlgorithm::EcdsaP384,
        GenerateAlgorithm::Rsa => KeyAlgorithm::Rsa,
    };
    let opts = GenerateOptions {
        algorithm,
        rsa_bits: request.bits.unwrap_or(4096).clamp(3072, 8192),
        comment: request.label.clone(),
        passphrase: passphrase.clone(),
    };
    // RSA generation can take a while: keep it off the async executor.
    let generated = tauri::async_runtime::spawn_blocking(move || k::generate(&opts))
        .await
        .map_err(|e| CmdError::internal(e.to_string()))?
        .map_err(map_key)?;
    let id = state.with_vault(|v| {
        let secret = v.put_secret(generated.private_key_pem.as_bytes(), false)?;
        let pass_ref = match (&passphrase, request.save_passphrase) {
            (Some(p), true) => Some(v.put_secret(p.as_bytes(), false)?),
            _ => None,
        };
        let meta = KeyMeta {
            id: KeyId::new(),
            label: request.label.clone(),
            algorithm: generated.algorithm,
            bits: (generated.algorithm == KeyAlgorithm::Rsa).then_some(generated.bits),
            public_key: generated.public_key.clone(),
            fingerprint: generated.fingerprint.clone(),
            storage: KeyStorage::Vault { secret },
            encrypted: passphrase.is_some(),
            passphrase: pass_ref,
            certificate: None,
            created_at: now_ms(),
            updated_at: now_ms(),
        };
        v.put(&meta)?;
        Ok(meta.id.to_string())
    })?;
    notify(&app);
    Ok(id)
}

fn store_imported(
    state: &AppState,
    label: &str,
    pem: &str,
    passphrase: Option<&Zeroizing<String>>,
    save_passphrase: bool,
    certificate: Option<String>,
    id: Option<KeyId>,
) -> CmdResult<String> {
    let info = k::inspect_private_key(pem, passphrase.map(|p| p.as_str())).map_err(map_key)?;
    // Make sure the key actually decrypts before storing it.
    k::decode_private_key(pem, passphrase.map(|p| p.as_str())).map_err(map_key)?;
    let public_key = info
        .public_key
        .ok_or_else(|| CmdError::invalid("could not derive the public key"))?;
    let algorithm = info
        .algorithm
        .ok_or_else(|| CmdError::invalid("unsupported key algorithm"))?;
    if let Some(cert) = &certificate {
        k::parse_certificate(cert).map_err(map_key)?;
    }
    state.with_vault(|v| {
        let secret = v.put_secret(pem.as_bytes(), false)?;
        let pass_ref = match (passphrase, save_passphrase) {
            (Some(p), true) => Some(v.put_secret(p.as_bytes(), false)?),
            _ => None,
        };
        let meta = KeyMeta {
            id: id.unwrap_or_default(),
            label: if label.trim().is_empty() {
                info.comment
                    .clone()
                    .unwrap_or_else(|| "Imported key".into())
            } else {
                label.to_owned()
            },
            algorithm,
            bits: info.bits,
            public_key,
            fingerprint: info.fingerprint.clone().unwrap_or_default(),
            storage: KeyStorage::Vault { secret },
            encrypted: info.encrypted,
            passphrase: pass_ref,
            certificate: certificate.filter(|c| !c.trim().is_empty()),
            created_at: now_ms(),
            updated_at: now_ms(),
        };
        v.put(&meta)?;
        Ok(meta.id.to_string())
    })
}

/// Imports a private key (OpenSSH, PEM or PuTTY).
#[tauri::command]
#[specta::specta]
pub fn key_import(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ImportKeyRequest,
) -> CmdResult<String> {
    let pem = Zeroizing::new(request.private_key);
    let passphrase = request
        .passphrase
        .map(Zeroizing::new)
        .filter(|p| !p.is_empty());
    let id = store_imported(
        &state,
        &request.label,
        &pem,
        passphrase.as_ref(),
        request.save_passphrase,
        request.certificate,
        None,
    )?;
    notify(&app);
    Ok(id)
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

/// Enrolls a FIDO2 security key (touch and, if configured, PIN on the device).
#[tauri::command]
#[specta::specta]
pub async fn key_enroll_fido2(
    app: AppHandle,
    state: State<'_, AppState>,
    label: String,
) -> CmdResult<String> {
    #[cfg(desktop)]
    {
        let pin = crate::net::ui_pin(crate::net::Asker::Global(app.clone()));
        let options = hw_keys::fido2::EnrollOptions {
            user_name: "vaultport".into(),
            ..hw_keys::fido2::EnrollOptions::default()
        };
        let enrollment = hw_keys::fido2::enroll(&options, pin.as_ref())
            .await
            .map_err(|e| CmdError::new(ErrorCode::Unsupported, e.to_string()))?;
        let key = enrollment.key;
        let id = state.with_vault(|v| {
            let secret = v.put_secret(key.key_handle(), false)?;
            let meta = KeyMeta {
                id: KeyId::new(),
                label: label.clone(),
                algorithm: match key.algorithm() {
                    hw_keys::sk::SkAlgorithm::Ed25519 => KeyAlgorithm::SkEd25519,
                    _ => KeyAlgorithm::SkEcdsaP256,
                },
                bits: None,
                public_key: key.public_key_openssh(&label),
                fingerprint: key.fingerprint(),
                storage: key.key_storage(secret),
                encrypted: false,
                passphrase: None,
                certificate: None,
                created_at: now_ms(),
                updated_at: now_ms(),
            };
            v.put(&meta)?;
            Ok(meta.id.to_string())
        })?;
        notify(&app);
        Ok(id)
    }
    #[cfg(mobile)]
    {
        let _ = (app, state, label);
        Err(CmdError::new(
            ErrorCode::Unsupported,
            "security keys are enrolled through the system on this device",
        ))
    }
}

/// Creates a non-exportable key in the device's secure hardware (Secure Enclave, Windows
/// Hello / TPM, Android Keystore).
#[tauri::command]
#[specta::specta]
pub async fn key_create_hardware(
    app: AppHandle,
    state: State<'_, AppState>,
    label: String,
) -> CmdResult<String> {
    let backend = crate::mobile::hardware_backend();
    if !backend.is_available().await {
        return Err(CmdError::new(
            ErrorCode::Unsupported,
            "this device has no supported secure hardware for SSH keys",
        ));
    }
    let signer = hw_keys::platform::PlatformSigner::create(
        backend,
        &label,
        hw_keys::platform::AccessPolicy::default(),
    )
    .await
    .map_err(|e| CmdError::new(ErrorCode::Unsupported, e.to_string()))?;
    use hw_keys::HardwareSigner as _;
    let id = state.with_vault(|v| {
        let meta = KeyMeta {
            id: KeyId::new(),
            label: label.clone(),
            algorithm: signer.key_algorithm(),
            bits: None,
            public_key: signer.public_key_openssh(),
            fingerprint: signer.fingerprint(),
            storage: signer.key_storage(),
            encrypted: false,
            passphrase: None,
            certificate: None,
            created_at: now_ms(),
            updated_at: now_ms(),
        };
        v.put(&meta)?;
        Ok(meta.id.to_string())
    })?;
    notify(&app);
    Ok(id)
}

/// Appends a key's public half to `~/.ssh/authorized_keys` on a saved host.
#[tauri::command]
#[specta::specta]
pub async fn key_export_to_host(
    app: AppHandle,
    state: State<'_, AppState>,
    key_id: String,
    host_id: String,
) -> CmdResult<bool> {
    let public = state.with_vault(|v| {
        Ok(v.get::<KeyMeta>(&key_id)?
            .ok_or_else(|| CmdError::not_found("key not found"))?
            .public_key)
    })?;
    let pool = tauri::Manager::state::<net::ConnectionPool>(&app);
    let conn = pool.get(&app, &host_id).await?;
    let outcome = k::export_to_host(&conn, &public, None)
        .await
        .map_err(net::map_ssh)?;
    Ok(matches!(outcome, k::ExportOutcome::Added))
}

/// Imports a key referenced by another client's configuration (e.g. `IdentityFile`).
pub fn import_key_file(state: &AppState, key: &importers::ImportedKey) -> CmdResult<()> {
    let pem = match &key.material {
        importers::KeyMaterial::Private { pem } => pem.clone(),
        importers::KeyMaterial::Unloaded => {
            let path = key
                .source_path
                .as_deref()
                .ok_or_else(|| CmdError::invalid("no key file path"))?;
            let expanded = path.strip_prefix("~/").map_or_else(
                || std::path::PathBuf::from(path),
                |rest| {
                    std::env::var_os("HOME")
                        .or_else(|| std::env::var_os("USERPROFILE"))
                        .map(|h| std::path::PathBuf::from(h).join(rest))
                        .unwrap_or_default()
                },
            );
            Zeroizing::new(read_key_file(expanded.to_string_lossy().into_owned())?)
        }
        _ => return Err(CmdError::invalid("only private keys can be imported")),
    };
    // Encrypted keys are stored as-is; the passphrase is asked for on first use.
    match store_imported(state, &key.label, &pem, None, false, None, Some(key.id)) {
        Err(e) if e.code == ErrorCode::Authentication => {
            let info = k::inspect_private_key(&pem, None).map_err(map_key)?;
            state.with_vault(|v| {
                let secret = v.put_secret(pem.as_bytes(), false)?;
                v.put(&KeyMeta {
                    id: key.id,
                    label: key.label.clone(),
                    algorithm: info.algorithm.unwrap_or(KeyAlgorithm::Ed25519),
                    bits: info.bits,
                    public_key: info.public_key.clone().unwrap_or_default(),
                    fingerprint: info.fingerprint.clone().unwrap_or_default(),
                    storage: KeyStorage::Vault { secret },
                    encrypted: true,
                    passphrase: None,
                    certificate: None,
                    created_at: now_ms(),
                    updated_at: now_ms(),
                })?;
                Ok(())
            })
        }
        other => other.map(|_| ()),
    }
}
