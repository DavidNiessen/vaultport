// SPDX-License-Identifier: GPL-3.0-or-later
//! Vault lifecycle commands: create, unlock, lock, manage unlock methods.

use crate::error::{CmdError, CmdResult, ErrorCode};
use crate::keystore;
use crate::state::{AppState, OpenVault, list_vault_files};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;
use vault::{CreateOptions, KdfParams, SymmetricKey, Vault};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    /// Names of all vaults on this device.
    pub vaults: Vec<String>,
    /// Name of the open vault, if any.
    pub active: Option<String>,
    pub locked: bool,
    pub has_password: bool,
    pub has_device_key: bool,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateVaultRequest {
    pub name: String,
    /// Master password. Optional, but strongly recommended.
    pub password: Option<String>,
    /// Also allow unlocking with a key stored in the system key store.
    pub remember_on_device: bool,
}

fn kdf() -> CmdResult<KdfParams> {
    let p = if cfg!(any(target_os = "android", target_os = "ios")) {
        KdfParams::mobile()
    } else {
        KdfParams::desktop()
    };
    Ok(p?)
}

fn status(state: &AppState) -> CmdResult<VaultStatus> {
    let vaults = list_vault_files(&state.vaults_dir())?;
    let guard = state.vault.lock();
    Ok(match guard.as_ref() {
        Some(open) => {
            let m = open.vault.unlock_methods();
            VaultStatus {
                vaults,
                active: Some(open.name.clone()),
                locked: open.vault.is_locked(),
                has_password: m.password,
                has_device_key: m.device_key,
            }
        }
        None => VaultStatus {
            vaults,
            active: None,
            locked: true,
            has_password: false,
            has_device_key: false,
        },
    })
}

/// Returns the state of all vaults on this device.
#[tauri::command]
#[specta::specta]
pub fn vault_status(state: State<'_, AppState>) -> CmdResult<VaultStatus> {
    status(&state)
}

/// Creates a new vault and opens it.
#[tauri::command]
#[specta::specta]
pub async fn vault_create(
    state: State<'_, AppState>,
    request: CreateVaultRequest,
) -> CmdResult<VaultStatus> {
    let path = state.vault_path(&request.name)?;
    std::fs::create_dir_all(state.vaults_dir())?;
    let password = request.password.map(Zeroizing::new);
    if password.as_ref().is_some_and(|p| p.chars().count() < 8) {
        return Err(CmdError::invalid(
            "the master password must have at least 8 characters",
        ));
    }
    if password.is_none() && !request.remember_on_device {
        return Err(CmdError::invalid(
            "choose a master password or allow unlocking on this device",
        ));
    }
    let device_key = if request.remember_on_device {
        Some(SymmetricKey::generate()?)
    } else {
        None
    };
    let kdf = kdf()?;
    let dk = device_key.clone();
    let vault = tauri::async_runtime::spawn_blocking(move || {
        Vault::create(
            &path,
            &CreateOptions {
                password: password.as_ref().map(|p| p.as_bytes()),
                device_key: dk.as_ref(),
                kdf,
            },
        )
    })
    .await
    .map_err(|e| CmdError::internal(e.to_string()))??;
    if let Some(dk) = &device_key
        && let Err(e) = keystore::store_device_key(&vault.vault_id(), dk)
    {
        // Roll back: a vault whose only unlock method is lost would be unusable.
        let path = vault.path().to_path_buf();
        drop(vault);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("vault-wal"));
        let _ = std::fs::remove_file(path.with_extension("vault-shm"));
        return Err(e);
    }
    *state.vault.lock() = Some(OpenVault {
        name: request.name,
        vault,
    });
    status(&state)
}

/// Opens (if needed) and unlocks a vault with the master password or, when `password` is
/// `None`, with the device key from the system key store.
#[tauri::command]
#[specta::specta]
pub async fn vault_unlock(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    name: String,
    password: Option<String>,
) -> CmdResult<VaultStatus> {
    let path = state.vault_path(&name)?;
    let password = password.map(Zeroizing::new);
    let slot = state.vault.clone();
    let name2 = name.clone();
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<()> {
        let mut guard = slot.lock();
        let reuse = guard.as_ref().is_some_and(|o| o.name == name2);
        if !reuse {
            *guard = Some(OpenVault {
                name: name2,
                vault: Vault::open(&path)?,
            });
        }
        let open = guard
            .as_mut()
            .ok_or_else(|| CmdError::internal("vault slot empty"))?;
        if !open.vault.is_locked() {
            return Ok(());
        }
        if let Some(pw) = &password {
            open.vault.unlock_with_password(pw.as_bytes())?;
        } else {
            let key = keystore::load_device_key(&open.vault.vault_id())?.ok_or_else(|| {
                CmdError::new(
                    ErrorCode::Authentication,
                    "no device key is stored for this vault; enter the master password",
                )
            })?;
            open.vault.unlock_with_device_key(&key)?;
        }
        Ok(())
    })
    .await
    .map_err(|e| CmdError::internal(e.to_string()))??;
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        crate::commands::forwarding::autostart(&app2).await;
    });
    status(&state)
}

/// Emitted after the core locks the vault on its own (e.g. when the computer sleeps).
pub const VAULT_LOCKED: &str = "vault://locked";

/// Closes sessions, forwards and connections, then locks the open vault and wipes decrypted
/// data from memory.
pub async fn lock_vault(app: &tauri::AppHandle) {
    use tauri::{Emitter as _, Manager};
    app.state::<crate::sessions::SessionManager>().close_all();
    app.state::<crate::commands::forwarding::ForwardManager>()
        .stop_all()
        .await;
    app.state::<crate::net::ConnectionPool>().close_all().await;
    app.state::<std::sync::Arc<crate::agent::AgentState>>()
        .clear_grants();
    app.state::<crate::commands::files::FilesState>()
        .reset()
        .await;
    if let Some(open) = app.state::<AppState>().vault.lock().as_mut() {
        open.vault.lock();
    }
    let _ = app.emit(VAULT_LOCKED, ());
}

/// Locks the open vault and wipes decrypted data from memory.
#[tauri::command]
#[specta::specta]
pub async fn vault_lock(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<VaultStatus> {
    lock_vault(&app).await;
    status(&state)
}

/// Sets or changes the master password of the open vault.
#[tauri::command]
#[specta::specta]
pub async fn vault_set_password(
    state: State<'_, AppState>,
    password: String,
) -> CmdResult<VaultStatus> {
    let password = Zeroizing::new(password);
    if password.chars().count() < 8 {
        return Err(CmdError::invalid(
            "the master password must have at least 8 characters",
        ));
    }
    let kdf = kdf()?;
    let slot = state.vault.clone();
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<()> {
        let mut guard = slot.lock();
        let open = guard
            .as_mut()
            .ok_or_else(|| CmdError::new(ErrorCode::NoVault, "no vault is open"))?;
        open.vault.set_password(password.as_bytes(), kdf)?;
        Ok(())
    })
    .await
    .map_err(|e| CmdError::internal(e.to_string()))??;
    status(&state)
}

/// Removes the master password (only allowed when device unlock is enabled).
#[tauri::command]
#[specta::specta]
pub fn vault_remove_password(state: State<'_, AppState>) -> CmdResult<VaultStatus> {
    state.with_vault(|v| Ok(v.remove_password()?))?;
    status(&state)
}

/// Enables or disables unlocking with the system key store on this device.
#[tauri::command]
#[specta::specta]
pub fn vault_set_device_unlock(
    state: State<'_, AppState>,
    enabled: bool,
) -> CmdResult<VaultStatus> {
    state.with_vault(|v| {
        if enabled {
            let key = SymmetricKey::generate()?;
            keystore::store_device_key(&v.vault_id(), &key)?;
            v.set_device_key(&key)?;
        } else {
            v.remove_device_key()?;
            keystore::delete_device_key(&v.vault_id())?;
        }
        Ok(())
    })?;
    status(&state)
}
