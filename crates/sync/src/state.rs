// SPDX-License-Identifier: GPL-3.0-or-later
//! Local, per-backend sync state.
//!
//! The state is small JSON that the app keeps in the vault as a **device-local** settings
//! document ([`SyncState::load`] / [`SyncState::save`]), one per configured backend. It is
//! what makes rollback and truncation detectable: it remembers the newest segment (counter
//! and hash) seen from every device.

use crate::error::SyncError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;
use vault::Vault;
use vault::hlc::DeviceId;

/// The newest verified segment of a device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Head {
    pub counter: u64,
    /// SHA-256 (hex) of the segment file.
    pub hash: String,
    /// Object name of the segment (lets readers notice a different file at this position).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Sync state for one vault on one backend.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncState {
    /// The vault this state belongs to (set on first sync).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_id: Option<Uuid>,
    /// Local op-log cursor: ops with a higher sequence number have not been uploaded yet.
    #[serde(default)]
    pub cursor: i64,
    /// Newest verified segment per device (hex device id), including this device.
    #[serde(default)]
    pub heads: BTreeMap<String, Head>,
    /// For snapshots that have been applied: object name → device → covered counter. Used to
    /// tell legitimately pruned segments from a rollback.
    #[serde(default)]
    pub coverage: BTreeMap<String, BTreeMap<String, u64>>,
    /// Unix ms of the last successful sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_at: Option<i64>,
}

/// Settings document name prefix used by [`SyncState::load`] / [`SyncState::save`].
pub const STATE_SETTINGS_PREFIX: &str = "sync-state-";

impl SyncState {
    /// A fresh state for `vault`.
    #[must_use]
    pub fn new(vault_id: Uuid) -> Self {
        Self {
            vault_id: Some(vault_id),
            ..Self::default()
        }
    }

    pub(crate) fn check_vault(&mut self, vault_id: Uuid) -> Result<(), SyncError> {
        match self.vault_id {
            Some(v) if v != vault_id => Err(SyncError::StateMismatch {
                state: v,
                vault: vault_id,
            }),
            Some(_) => Ok(()),
            None => {
                self.vault_id = Some(vault_id);
                Ok(())
            }
        }
    }

    #[must_use]
    pub fn head(&self, device: DeviceId) -> Option<&Head> {
        self.heads.get(&device.to_string())
    }

    pub(crate) fn set_head(&mut self, device: DeviceId, head: Head) {
        self.heads.insert(device.to_string(), head);
    }

    /// Loads the state for `backend_id` from the vault's device-local settings.
    pub fn load(vault: &Vault, backend_id: &str) -> Result<Self, SyncError> {
        let name = format!("{STATE_SETTINGS_PREFIX}{backend_id}");
        match vault.get_settings(&name)? {
            Some(v) => serde_json::from_value(v).map_err(|e| SyncError::Format(e.to_string())),
            None => Ok(Self::new(vault.vault_id())),
        }
    }

    /// Saves the state for `backend_id` as a device-local (never replicated) settings
    /// document.
    pub fn save(&self, vault: &mut Vault, backend_id: &str) -> Result<(), SyncError> {
        let name = format!("{STATE_SETTINGS_PREFIX}{backend_id}");
        let v = serde_json::to_value(self).map_err(|e| SyncError::Format(e.to_string()))?;
        vault.put_settings(&name, &v, true)?;
        Ok(())
    }
}
