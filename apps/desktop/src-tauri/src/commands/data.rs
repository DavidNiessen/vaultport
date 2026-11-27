// SPDX-License-Identifier: GPL-3.0-or-later
//! Importing from other clients and cloud inventories.

use crate::commands::entities::{VAULT_CHANGED, VaultChanged};
use crate::error::{CmdError, CmdResult, ErrorCode};
use crate::state::AppState;
use core_model::{CloudIntegration, CloudProvider, Group, GroupId, Host, IntegrationId};
use importers::{Format, ImportBundle};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, State};
use zeroize::Zeroizing;

const MAX_FILE: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ImportFormat {
    OpenSshConfig,
    KnownHosts,
    Csv,
    PuttyReg,
    MobaXterm,
    SecureCrtXml,
}

impl From<ImportFormat> for Format {
    fn from(f: ImportFormat) -> Self {
        match f {
            ImportFormat::OpenSshConfig => Self::OpenSshConfig,
            ImportFormat::KnownHosts => Self::KnownHosts,
            ImportFormat::Csv => Self::Csv,
            ImportFormat::PuttyReg => Self::PuttyReg,
            ImportFormat::MobaXterm => Self::MobaXterm,
            ImportFormat::SecureCrtXml => Self::SecureCrtXml,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub hosts: u32,
    pub groups: u32,
    pub keys: u32,
    pub known_hosts: u32,
    pub forwards: u32,
    pub warnings: Vec<String>,
}

fn notify_all(app: &AppHandle) {
    for kind in ["host", "group", "key", "identity", "knownHost", "forward"] {
        let _ = app.emit(VAULT_CHANGED, VaultChanged { kind: kind.into() });
    }
}

pub(crate) fn read_limited(path: &Path) -> CmdResult<Vec<u8>> {
    let meta = std::fs::metadata(path)?;
    if !meta.is_file() {
        return Err(CmdError::invalid("not a file"));
    }
    if meta.len() > MAX_FILE {
        return Err(CmdError::invalid("the file is too large (max 16 MiB)"));
    }
    Ok(std::fs::read(path)?)
}

fn ssh_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|h| PathBuf::from(h).join(".ssh"))
}

/// Path of the user's OpenSSH client configuration, if it exists.
#[tauri::command]
#[specta::specta]
pub fn default_ssh_config_path() -> Option<String> {
    let p = ssh_dir()?.join("config");
    p.is_file().then(|| p.to_string_lossy().into_owned())
}

/// Stores an import bundle in the vault (secrets first, then entities).
fn apply_bundle(state: &AppState, bundle: ImportBundle) -> CmdResult<ImportSummary> {
    let mut summary = ImportSummary {
        warnings: bundle.warnings.iter().map(ToString::to_string).collect(),
        ..ImportSummary::default()
    };
    let key_files: Vec<_> = bundle.keys;
    state.with_vault(|v| {
        for s in &bundle.secrets {
            v.update_secret(s.slot, &s.value, false)?;
        }
        for g in &bundle.groups {
            if v.put(g)? {
                summary.groups += 1;
            }
        }
        for h in &bundle.hosts {
            match v.put(h) {
                Ok(true) => summary.hosts += 1,
                Ok(false) => {}
                Err(e) => summary.warnings.push(format!("{}: {e}", h.label)),
            }
        }
        for i in &bundle.identities {
            v.put(i)?;
        }
        for k in &bundle.known_hosts {
            if v.put(k)? {
                summary.known_hosts += 1;
            }
        }
        for f in &bundle.forwards {
            if v.put(f)? {
                summary.forwards += 1;
            }
        }
        Ok(())
    })?;
    for key in key_files {
        match crate::commands::keys::import_key_file(state, &key) {
            Ok(()) => summary.keys += 1,
            Err(e) => summary
                .warnings
                .push(format!("key {}: {}", key.label, e.message)),
        }
    }
    Ok(summary)
}

/// Imports a file from another client. `format` is detected when `None`.
#[tauri::command]
#[specta::specta]
pub fn import_file(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    format: Option<ImportFormat>,
) -> CmdResult<ImportSummary> {
    let path = PathBuf::from(path);
    let bytes = read_limited(&path)?;
    let format: Format = match format {
        Some(f) => f.into(),
        None => importers::detect_format(&bytes)
            .ok_or_else(|| CmdError::invalid("the file format was not recognised"))?,
    };
    let bundle = if matches!(format, Format::OpenSshConfig) {
        let main = String::from_utf8_lossy(&bytes).into_owned();
        let base = path.parent().map(Path::to_path_buf).or_else(ssh_dir);
        importers::ssh_config::parse_with_includes(&main, |inc| {
            let expanded = inc.strip_prefix("~/").map_or_else(
                || PathBuf::from(inc),
                |rest| {
                    ssh_dir()
                        .and_then(|d| d.parent().map(|h| h.join(rest)))
                        .unwrap_or_default()
                },
            );
            let full = if expanded.is_absolute() {
                expanded
            } else {
                base.clone()?.join(expanded)
            };
            read_limited(&full)
                .ok()
                .map(|b| String::from_utf8_lossy(&b).into_owned())
        })
    } else {
        importers::import(format, &bytes)
    }
    .map_err(|e| CmdError::invalid(e.to_string()))?;
    let summary = apply_bundle(&state, bundle)?;
    notify_all(&app);
    Ok(summary)
}

/// Settings key under which cloud reconciliation state is kept.
const MANAGED_STATE: &str = "managedState";

fn provider_from(s: &str) -> CmdResult<CloudProvider> {
    match s {
        "aws" => Ok(CloudProvider::Aws),
        "digitalocean" => Ok(CloudProvider::Digitalocean),
        "azure" => Ok(CloudProvider::Azure),
        _ => Err(CmdError::invalid("unknown provider")),
    }
}

/// Adds a cloud inventory integration (credentials stay on this device) and syncs it once.
/// Returns the number of hosts in the group after syncing.
#[tauri::command]
#[specta::specta]
pub async fn cloud_add_integration(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: String,
    label: String,
    credentials: String,
    settings: BTreeMap<String, String>,
) -> CmdResult<u32> {
    let provider = provider_from(&provider)?;
    let credentials = Zeroizing::new(credentials);
    let label = if label.trim().is_empty() {
        format!("{provider:?}")
    } else {
        label
    };
    let id = state.with_vault(|v| {
        let secret = v.put_secret(credentials.as_bytes(), true)?;
        let mut group = Group::new(label.clone());
        let integration = CloudIntegration {
            id: IntegrationId::new(),
            label: label.clone(),
            provider,
            group: group.id,
            settings: settings
                .into_iter()
                .map(|(k, val)| (k, serde_json::Value::String(val)))
                .collect(),
            credentials: secret,
            sync_credentials: false,
            last_sync_at: None,
            last_error: None,
        };
        group.integration = Some(integration.id);
        v.put(&group)?;
        v.put(&integration)?;
        Ok(integration.id)
    })?;
    cloud_sync(app, state, id.to_string()).await
}

/// Re-syncs a cloud integration. Returns the number of hosts in its group.
#[tauri::command]
#[specta::specta]
pub async fn cloud_sync(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<u32> {
    let (integration, secret) = state.with_vault(|v| {
        let i = v
            .get::<CloudIntegration>(&id)?
            .ok_or_else(|| CmdError::not_found("integration not found"))?;
        let s = v.get_secret(i.credentials)?;
        Ok((i, s))
    })?;
    let result =
        cloud_inventory::fetch_inventory(integration.provider, &integration.settings, &secret)
            .await;
    drop(secret);
    let inventory = match result {
        Ok(inv) => inv,
        Err(e) => {
            let msg = e.to_string();
            state.with_vault(|v| {
                let mut i = integration.clone();
                i.last_error = Some(msg.clone());
                v.put(&i)?;
                Ok(())
            })?;
            return Err(CmdError::new(ErrorCode::Network, msg));
        }
    };
    let count = state.with_vault(|v| {
        let group: GroupId = integration.group;
        let existing: Vec<Host> = v
            .list::<Host>()?
            .into_iter()
            .filter(|h| h.group == Some(group))
            .collect();
        let previous = integration
            .settings
            .get(MANAGED_STATE)
            .and_then(|s| serde_json::from_value(s.clone()).ok())
            .unwrap_or_default();
        let diff = cloud_inventory::sync_group(&inventory, &existing, group, &previous);
        for h in diff.add.iter().chain(diff.update.iter()) {
            v.put(h)?;
        }
        for h in &diff.remove {
            v.delete::<Host>(&h.id.to_string())?;
        }
        let mut i = integration.clone();
        i.settings.insert(
            MANAGED_STATE.into(),
            serde_json::to_value(&diff.state).map_err(|e| CmdError::internal(e.to_string()))?,
        );
        i.last_sync_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as i64),
        );
        i.last_error = None;
        v.put(&i)?;
        let n = existing.len() + diff.add.len() - diff.remove.len();
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    })?;
    notify_all(&app);
    Ok(count)
}
