// SPDX-License-Identifier: GPL-3.0-or-later
//! CRUD commands for vault entities, plus secret storage.
//!
//! Secrets flow **one way**: the UI can store a secret and receives an opaque [`SecretRef`],
//! but there is no command that returns secret material to the UI.

use crate::error::{CmdError, CmdResult, JsonValue};
use crate::state::AppState;
use core_model::{
    Group, GroupId, Host, HostId, Identity, KeyMeta, KnownHost, PortForwardRule, ResolvedConfig,
    Resolver, SecretRef, Snippet, SnippetPackage, WorkspaceTemplate,
};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter, State};
use zeroize::Zeroizing;

/// Event emitted whenever vault data changes so every window can refresh.
pub const VAULT_CHANGED: &str = "vault://changed";

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VaultChanged {
    pub kind: String,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn notify(app: &AppHandle, kind: &str) {
    let _ = app.emit(
        VAULT_CHANGED,
        VaultChanged {
            kind: kind.to_owned(),
        },
    );
}

macro_rules! crud {
    ($ty:ty, $list:ident, $save:ident, $delete:ident, $kind:literal, |$e:ident| $touch:block) => {
        #[tauri::command]
        #[specta::specta]
        pub fn $list(state: State<'_, AppState>) -> CmdResult<Vec<$ty>> {
            state.with_vault(|v| Ok(v.list::<$ty>()?))
        }

        #[tauri::command]
        #[specta::specta]
        pub fn $save(app: AppHandle, state: State<'_, AppState>, entity: $ty) -> CmdResult<$ty> {
            let mut $e = entity;
            $touch;
            let saved = $e.clone();
            let changed = state.with_vault(|v| Ok(v.put(&saved)?))?;
            if changed {
                notify(&app, $kind);
            }
            Ok(saved)
        }

        #[allow(dead_code, reason = "some kinds wrap deletion in a custom command")]
        #[tauri::command]
        #[specta::specta]
        pub fn $delete(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<bool> {
            let deleted = state.with_vault(|v| Ok(v.delete::<$ty>(&id)?))?;
            if deleted {
                notify(&app, $kind);
            }
            Ok(deleted)
        }
    };
}

crud!(
    Host,
    list_hosts,
    save_host,
    delete_host_record,
    "host",
    |e| {
        let now = now_ms();
        if e.created_at == 0 {
            e.created_at = now;
        }
        e.updated_at = now;
        e.tags.sort_unstable_by_key(|t| t.to_lowercase());
        e.tags.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    }
);
crud!(
    Group,
    list_groups,
    save_group_record,
    delete_group_record,
    "group",
    |e| {
        let now = now_ms();
        if e.created_at == 0 {
            e.created_at = now;
        }
        e.updated_at = now;
    }
);
crud!(
    Identity,
    list_identities,
    save_identity,
    delete_identity,
    "identity",
    |e| {
        let now = now_ms();
        if e.created_at == 0 {
            e.created_at = now;
        }
        e.updated_at = now;
    }
);
crud!(
    KeyMeta,
    list_keys,
    save_key_meta,
    delete_key_meta,
    "key",
    |e| {
        let now = now_ms();
        if e.created_at == 0 {
            e.created_at = now;
        }
        e.updated_at = now;
    }
);
crud!(
    Snippet,
    list_snippets,
    save_snippet,
    delete_snippet,
    "snippet",
    |e| {
        let now = now_ms();
        if e.created_at == 0 {
            e.created_at = now;
        }
        e.updated_at = now;
    }
);
crud!(
    SnippetPackage,
    list_packages,
    save_package,
    delete_package,
    "package",
    |e| {
        if e.created_at == 0 {
            e.created_at = now_ms();
        }
    }
);
crud!(
    PortForwardRule,
    list_forwards,
    save_forward,
    delete_forward,
    "forward",
    |e| {
        let now = now_ms();
        if e.created_at == 0 {
            e.created_at = now;
        }
        e.updated_at = now;
    }
);
crud!(
    KnownHost,
    list_known_hosts,
    save_known_host,
    delete_known_host,
    "knownHost",
    |e| {
        if e.added_at == 0 {
            e.added_at = now_ms();
        }
    }
);
crud!(
    WorkspaceTemplate,
    list_workspaces,
    save_workspace,
    delete_workspace,
    "workspace",
    |e| {
        let now = now_ms();
        if e.created_at == 0 {
            e.created_at = now;
        }
        e.updated_at = now;
    }
);

/// Saves a group, rejecting parent changes that would create a cycle.
#[tauri::command]
#[specta::specta]
pub fn save_group(app: AppHandle, state: State<'_, AppState>, entity: Group) -> CmdResult<Group> {
    state.with_vault(|v| {
        let hosts = v.list::<Host>()?;
        let mut groups = v.list::<Group>()?;
        groups.retain(|g| g.id != entity.id);
        groups.push(entity.clone());
        let r = Resolver::new(&hosts, &groups);
        if let Some(parent) = entity.parent
            && r.would_create_cycle(entity.id, Some(parent))
        {
            return Err(CmdError::invalid(
                "a group cannot be moved into its own subgroup",
            ));
        }
        Ok(())
    })?;
    save_group_record(app, state, entity)
}

/// Deletes a group. Child groups and hosts move up to the deleted group's parent.
#[tauri::command]
#[specta::specta]
pub fn delete_group(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<bool> {
    let gid: GroupId = id
        .parse()
        .map_err(|_| CmdError::invalid("invalid group id"))?;
    let deleted = state.with_vault(|v| {
        let Some(group) = v.get::<Group>(&id)? else {
            return Ok(false);
        };
        for mut h in v.list::<Host>()? {
            if h.group == Some(gid) {
                h.group = group.parent;
                h.updated_at = now_ms();
                v.put(&h)?;
            }
        }
        for mut g in v.list::<Group>()? {
            if g.parent == Some(gid) {
                g.parent = group.parent;
                g.updated_at = now_ms();
                v.put(&g)?;
            }
        }
        Ok(v.delete::<Group>(&id)?)
    })?;
    if deleted {
        notify(&app, "group");
        notify(&app, "host");
    }
    Ok(deleted)
}

/// Deletes a host together with its inline secrets and port-forwarding rules.
#[tauri::command]
#[specta::specta]
pub fn delete_host(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<bool> {
    let hid: HostId = id
        .parse()
        .map_err(|_| CmdError::invalid("invalid host id"))?;
    let deleted = state.with_vault(|v| {
        let Some(host) = v.get::<Host>(&id)? else {
            return Ok(false);
        };
        if let Some(core_model::Credential::Inline {
            password: Some(secret),
            ..
        }) = &host.config.ssh.credential
        {
            v.delete_secret(*secret)?;
        }
        for rule in v.list::<PortForwardRule>()? {
            if rule.host == hid {
                v.delete::<PortForwardRule>(&rule.id.to_string())?;
            }
        }
        Ok(v.delete::<Host>(&id)?)
    })?;
    if deleted {
        notify(&app, "host");
        notify(&app, "forward");
    }
    Ok(deleted)
}

/// Returns the effective configuration of a host after group inheritance.
#[tauri::command]
#[specta::specta]
pub fn resolve_host(state: State<'_, AppState>, id: String) -> CmdResult<ResolvedConfig> {
    let hid: HostId = id
        .parse()
        .map_err(|_| CmdError::invalid("invalid host id"))?;
    state.with_vault(|v| {
        let hosts = v.list::<Host>()?;
        let groups = v.list::<Group>()?;
        Ok(Resolver::new(&hosts, &groups).resolve(hid)?)
    })
}

/// Stores a secret (password, token, passphrase). Returns an opaque reference.
#[tauri::command]
#[specta::specta]
pub fn store_secret(
    state: State<'_, AppState>,
    value: String,
    existing: Option<SecretRef>,
    device_only: bool,
) -> CmdResult<SecretRef> {
    let value = Zeroizing::new(value);
    state.with_vault(|v| match existing {
        Some(id) => {
            v.update_secret(id, value.as_bytes(), device_only)?;
            Ok(id)
        }
        None => Ok(v.put_secret(value.as_bytes(), device_only)?),
    })
}

/// Deletes a stored secret.
#[tauri::command]
#[specta::specta]
pub fn delete_secret(state: State<'_, AppState>, id: SecretRef) -> CmdResult<bool> {
    state.with_vault(|v| Ok(v.delete_secret(id)?))
}

/// Reads a settings document (non-secret application preferences).
#[tauri::command]
#[specta::specta]
pub fn get_settings(state: State<'_, AppState>, name: String) -> CmdResult<Option<JsonValue>> {
    validate_settings_name(&name)?;
    state.with_vault(|v| Ok(v.get_settings(&name)?.map(JsonValue)))
}

/// Writes a settings document.
#[tauri::command]
#[specta::specta]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    value: JsonValue,
    device_only: bool,
) -> CmdResult<()> {
    validate_settings_name(&name)?;
    let changed = state.with_vault(|v| Ok(v.put_settings(&name, &value.0, device_only)?))?;
    if changed {
        notify(&app, "settings");
    }
    Ok(())
}

fn validate_settings_name(name: &str) -> CmdResult<()> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(CmdError::invalid("invalid settings name"));
    }
    Ok(())
}
