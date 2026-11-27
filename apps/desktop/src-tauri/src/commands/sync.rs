// SPDX-License-Identifier: GPL-3.0-or-later
//! Serverless sync: user-owned storage targets, encrypted export/import and device pairing.

use crate::error::{CmdError, CmdResult, ErrorCode};
use crate::state::{AppState, OpenVault};
use core_model::SecretRef;
use serde::{Deserialize, Serialize};
use specta::Type;
use sync::backends::{FolderBackend, S3Backend, S3Config, WebDavBackend, WebDavCredentials};
use sync::{SyncBackend, SyncEngine, SyncError, SyncState};
use tauri::{AppHandle, Emitter, State};
use vault::{CreateOptions, KdfParams, Vault};
use zeroize::Zeroizing;

const TARGETS: &str = "syncTargets";

/// Non-secret configuration of a sync target.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SyncTargetConfig {
    Folder {
        path: String,
    },
    Webdav {
        url: String,
        username: String,
    },
    #[serde(rename_all = "camelCase")]
    S3 {
        endpoint: Option<String>,
        bucket: String,
        region: String,
        prefix: String,
        access_key_id: String,
    },
    Git {
        remote: String,
        branch: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncTarget {
    pub id: String,
    pub label: String,
    pub config: SyncTargetConfig,
    pub secret: Option<SecretRef>,
    #[specta(type = Option<f64>)]
    pub last_sync_at: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TargetList {
    targets: Vec<SyncTarget>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub uploaded: u32,
    pub applied: u32,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PairingInfo {
    pub qr_svg: String,
    pub payload: String,
    pub code: String,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn map_sync(e: SyncError) -> CmdError {
    let code = match &e {
        SyncError::Authentication => ErrorCode::Authentication,
        SyncError::Auth(_) => ErrorCode::AuthFailed,
        SyncError::Backend(_) | SyncError::RateLimited { .. } => ErrorCode::Network,
        SyncError::NotFound(_) => ErrorCode::NotFound,
        _ => ErrorCode::Internal,
    };
    CmdError::new(code, e.to_string())
}

fn load_targets(v: &Vault) -> CmdResult<TargetList> {
    Ok(v.get_settings(TARGETS)?
        .and_then(|j| serde_json::from_value(j).ok())
        .unwrap_or_default())
}

fn save_targets(v: &mut Vault, list: &TargetList) -> CmdResult<()> {
    let value = serde_json::to_value(list).map_err(|e| CmdError::internal(e.to_string()))?;
    v.put_settings(TARGETS, &value, true)?;
    Ok(())
}

/// Lists configured sync targets.
#[tauri::command]
#[specta::specta]
pub fn sync_list_targets(state: State<'_, AppState>) -> CmdResult<Vec<SyncTarget>> {
    state.with_vault(|v| Ok(load_targets(v)?.targets))
}

/// Adds a sync target. `secret` is the WebDAV password or S3 secret key.
#[tauri::command]
#[specta::specta]
pub fn sync_add_target(
    state: State<'_, AppState>,
    label: String,
    config: SyncTargetConfig,
    secret: Option<String>,
) -> CmdResult<SyncTarget> {
    let secret = secret.map(Zeroizing::new);
    match &config {
        SyncTargetConfig::Folder { path } => {
            if !std::path::Path::new(path).is_dir() {
                return Err(CmdError::invalid("the folder does not exist"));
            }
        }
        SyncTargetConfig::Webdav { url, .. } => {
            if !url.starts_with("https://") {
                return Err(CmdError::invalid("WebDAV URLs must use https://"));
            }
        }
        SyncTargetConfig::S3 {
            endpoint, bucket, ..
        } => {
            if bucket.is_empty()
                || endpoint
                    .as_ref()
                    .is_some_and(|e| !e.starts_with("https://"))
            {
                return Err(CmdError::invalid("invalid S3 settings"));
            }
        }
        SyncTargetConfig::Git { remote, .. } => {
            if remote.is_empty() {
                return Err(CmdError::invalid("enter a repository"));
            }
        }
    }
    state.with_vault(|v| {
        let secret_ref = match &secret {
            Some(s) if !s.is_empty() => Some(v.put_secret(s.as_bytes(), true)?),
            _ => None,
        };
        let target = SyncTarget {
            id: uuid::Uuid::now_v7().to_string(),
            label,
            config,
            secret: secret_ref,
            last_sync_at: None,
            last_error: None,
        };
        let mut list = load_targets(v)?;
        list.targets.push(target.clone());
        save_targets(v, &list)?;
        Ok(target)
    })
}

/// Removes a sync target (data already uploaded is left untouched).
#[tauri::command]
#[specta::specta]
pub fn sync_remove_target(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.with_vault(|v| {
        let mut list = load_targets(v)?;
        if let Some(t) = list.targets.iter().find(|t| t.id == id)
            && let Some(s) = t.secret
        {
            v.delete_secret(s)?;
        }
        list.targets.retain(|t| t.id != id);
        save_targets(v, &list)
    })
}

fn build_backend(
    config: &SyncTargetConfig,
    secret: Option<Zeroizing<Vec<u8>>>,
) -> CmdResult<Box<dyn SyncBackend>> {
    let secret_str = || {
        secret
            .as_ref()
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .unwrap_or_default()
    };
    Ok(match config {
        SyncTargetConfig::Folder { path } => Box::new(FolderBackend::new(path.clone())),
        SyncTargetConfig::Webdav { url, username } => {
            let creds = WebDavCredentials {
                username: username.clone(),
                password: secret_str(),
            };
            Box::new(WebDavBackend::new(url, Some(creds)).map_err(map_sync)?)
        }
        SyncTargetConfig::S3 {
            endpoint,
            bucket,
            region,
            prefix,
            access_key_id,
        } => {
            let mut prefix = prefix.trim_matches('/').to_owned();
            if !prefix.is_empty() {
                prefix.push('/');
            }
            let cfg = S3Config {
                endpoint: endpoint
                    .clone()
                    .unwrap_or_else(|| format!("https://s3.{region}.amazonaws.com")),
                region: region.clone(),
                bucket: bucket.clone(),
                prefix,
                addressing: sync::backends::Addressing::default(),
                conditional_writes: true,
            };
            let creds = cloud_inventory::sigv4::Credentials {
                access_key_id: access_key_id.clone(),
                secret_access_key: secret_str(),
                session_token: None,
            };
            Box::new(S3Backend::new(cfg, creds).map_err(map_sync)?)
        }
        SyncTargetConfig::Git { .. } => {
            return Err(CmdError::new(
                ErrorCode::Unsupported,
                "Git sync requires the SSH engine transport",
            ));
        }
    })
}

/// Temporarily takes the open vault out of the shared slot so it can be used across awaits.
struct Borrowed<'a> {
    slot: &'a AppState,
    open: Option<OpenVault>,
}

impl<'a> Borrowed<'a> {
    fn take(state: &'a AppState) -> CmdResult<Self> {
        let open = state
            .vault
            .lock()
            .take()
            .ok_or_else(|| CmdError::new(ErrorCode::NoVault, "no vault is open"))?;
        if open.vault.is_locked() {
            *state.vault.lock() = Some(open);
            return Err(CmdError::new(ErrorCode::Locked, "the vault is locked"));
        }
        Ok(Self {
            slot: state,
            open: Some(open),
        })
    }

    fn vault(&mut self) -> &mut Vault {
        // `open` is only `None` after drop.
        &mut self.open.as_mut().unwrap_or_else(|| unreachable!()).vault
    }
}

impl Drop for Borrowed<'_> {
    fn drop(&mut self) {
        if let Some(open) = self.open.take() {
            *self.slot.vault.lock() = Some(open);
        }
    }
}

/// Runs a sync with one target.
#[tauri::command]
#[specta::specta]
pub async fn sync_now(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<SyncSummary> {
    let (target, secret) = state.with_vault(|v| {
        let list = load_targets(v)?;
        let t = list
            .targets
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| CmdError::not_found("sync target not found"))?;
        let s = match t.secret {
            Some(r) => Some(v.get_secret(r)?),
            None => None,
        };
        Ok((t, s))
    })?;
    let backend = build_backend(&target.config, secret)?;
    // `Vault` is not `Sync`, so the engine runs on a dedicated current-thread runtime inside
    // a blocking task, with the vault temporarily moved out of the shared slot.
    let slot = state.vault.clone();
    let id2 = id.clone();
    let summary = tauri::async_runtime::spawn_blocking(move || -> CmdResult<SyncSummary> {
        let state_ref = AppState {
            data_dir: std::path::PathBuf::new(),
            vault: slot,
        };
        let mut borrowed = Borrowed::take(&state_ref)?;
        let vault = borrowed.vault();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| CmdError::internal(e.to_string()))?;
        let mut sync_state = SyncState::load(vault, &id2).map_err(map_sync)?;
        let result =
            rt.block_on(SyncEngine::default().sync(vault, backend.as_ref(), &mut sync_state));
        let _ = sync_state.save(vault, &id2);
        let mut list = load_targets(vault)?;
        let entry = list.targets.iter_mut().find(|t| t.id == id2);
        match result {
            Ok(report) => {
                if let Some(e) = entry {
                    e.last_sync_at = Some(now_ms());
                    e.last_error = report
                        .issues
                        .first()
                        .map(|i| format!("integrity issue: {i:?}"));
                }
                save_targets(vault, &list)?;
                Ok(SyncSummary {
                    uploaded: report
                        .uploaded
                        .map_or(0, |u| u32::try_from(u.ops).unwrap_or(u32::MAX)),
                    applied: u32::try_from(report.changes.len()).unwrap_or(u32::MAX),
                    issues: report.issues.iter().map(|i| format!("{i:?}")).collect(),
                })
            }
            Err(e) => {
                if let Some(t) = entry {
                    t.last_error = Some(e.to_string());
                }
                save_targets(vault, &list)?;
                Err(map_sync(e))
            }
        }
    })
    .await
    .map_err(|e| CmdError::internal(e.to_string()))??;
    for kind in [
        "host",
        "group",
        "key",
        "identity",
        "snippet",
        "forward",
        "knownHost",
    ] {
        let _ = app.emit(
            crate::commands::entities::VAULT_CHANGED,
            crate::commands::entities::VaultChanged { kind: kind.into() },
        );
    }
    Ok(summary)
}

/// Exports the vault to an encrypted `.sshvault` file.
#[tauri::command]
#[specta::specta]
pub async fn sync_export_file(
    state: State<'_, AppState>,
    path: String,
    passphrase: String,
) -> CmdResult<()> {
    let passphrase = Zeroizing::new(passphrase);
    if passphrase.chars().count() < 8 {
        return Err(CmdError::invalid(
            "the passphrase must have at least 8 characters",
        ));
    }
    let slot = state.vault.clone();
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<()> {
        let guard = slot.lock();
        let open = guard
            .as_ref()
            .ok_or_else(|| CmdError::new(ErrorCode::NoVault, "no vault is open"))?;
        let bytes = sync::export_file(&open.vault, passphrase.as_bytes()).map_err(map_sync)?;
        std::fs::write(&path, bytes)?;
        Ok(())
    })
    .await
    .map_err(|e| CmdError::internal(e.to_string()))?
}

/// Restores an encrypted `.sshvault` file as a new local vault named `name`, protected by
/// the same passphrase as the file (it can be changed afterwards).
#[tauri::command]
#[specta::specta]
pub async fn sync_import_file(
    state: State<'_, AppState>,
    path: String,
    passphrase: String,
    name: String,
) -> CmdResult<u32> {
    let passphrase = Zeroizing::new(passphrase);
    let dest = state.vault_path(&name)?;
    if dest.exists() {
        return Err(CmdError::invalid("a vault with this name already exists"));
    }
    std::fs::create_dir_all(state.vaults_dir())?;
    let bytes = crate::commands::data::read_limited(std::path::Path::new(&path))?;
    let kdf = if cfg!(mobile) {
        KdfParams::mobile()
    } else {
        KdfParams::desktop()
    }?;
    let slot = state.vault.clone();
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<u32> {
        let vault = sync::import_file(
            &bytes,
            passphrase.as_bytes(),
            &dest,
            &CreateOptions {
                password: Some(passphrase.as_bytes()),
                device_key: None,
                kdf,
            },
        )
        .map_err(map_sync)?;
        let n = vault.all_syncable_ops().map_or(0, |o| o.len());
        *slot.lock() = Some(OpenVault { name, vault });
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    })
    .await
    .map_err(|e| CmdError::internal(e.to_string()))?
}

/// Creates a pairing offer for another device: a QR code plus a short code shown separately.
#[tauri::command]
#[specta::specta]
pub fn sync_start_pairing(state: State<'_, AppState>) -> CmdResult<PairingInfo> {
    state.with_vault(|v| {
        let targets = load_targets(v)?;
        let backend = targets
            .targets
            .first()
            .map_or(serde_json::Value::Null, |t| {
                serde_json::to_value(&t.config).unwrap_or_default()
            });
        let secret = match targets.targets.first().and_then(|t| t.secret) {
            Some(r) => Some(v.get_secret(r)?),
            None => None,
        };
        let offer = sync::pairing::create_offer(
            v,
            backend,
            secret.as_deref().map(|s| &s[..]),
            sync::pairing::PairingKdf::default(),
        )
        .map_err(map_sync)?;
        let payload = offer.payload.to_qr_string().map_err(map_sync)?;
        let qr =
            qrcode::QrCode::with_error_correction_level(payload.as_bytes(), qrcode::EcLevel::M)
                .map_err(|e| CmdError::internal(e.to_string()))?;
        let svg = qr
            .render::<qrcode::render::svg::Color<'_>>()
            .min_dimensions(256, 256)
            .quiet_zone(true)
            .build();
        Ok(PairingInfo {
            qr_svg: svg,
            payload,
            code: offer.code().to_owned(),
        })
    })
}

/// Accepts a pairing offer on a new device and creates the local replica `name`.
#[tauri::command]
#[specta::specta]
pub async fn sync_accept_pairing(
    state: State<'_, AppState>,
    payload: String,
    code: String,
    name: String,
    password: String,
) -> CmdResult<()> {
    let password = Zeroizing::new(password);
    if password.chars().count() < 8 {
        return Err(CmdError::invalid(
            "the master password must have at least 8 characters",
        ));
    }
    let dest = state.vault_path(&name)?;
    if dest.exists() {
        return Err(CmdError::invalid("a vault with this name already exists"));
    }
    std::fs::create_dir_all(state.vaults_dir())?;
    let kdf = if cfg!(mobile) {
        KdfParams::mobile()
    } else {
        KdfParams::desktop()
    }?;
    let slot = state.vault.clone();
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<()> {
        let payload =
            sync::pairing::PairingPayload::from_qr_string(payload.trim()).map_err(map_sync)?;
        let accepted = sync::pairing::accept_offer(&payload, code.trim()).map_err(map_sync)?;
        let mut vault = Vault::create_replica(
            &dest,
            accepted.vault_id,
            &accepted.vault_key,
            &CreateOptions {
                password: Some(password.as_bytes()),
                device_key: None,
                kdf,
            },
        )?;
        if let Ok(config) = serde_json::from_value::<SyncTargetConfig>(accepted.backend.clone()) {
            let secret = match &accepted.backend_secret {
                Some(s) => Some(vault.put_secret(s, true)?),
                None => None,
            };
            save_targets(
                &mut vault,
                &TargetList {
                    targets: vec![SyncTarget {
                        id: uuid::Uuid::now_v7().to_string(),
                        label: "Paired storage".into(),
                        config,
                        secret,
                        last_sync_at: None,
                        last_error: None,
                    }],
                },
            )?;
        }
        *slot.lock() = Some(OpenVault { name, vault });
        Ok(())
    })
    .await
    .map_err(|e| CmdError::internal(e.to_string()))?
}
