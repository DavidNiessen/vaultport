// SPDX-License-Identifier: GPL-3.0-or-later
//! Signed self-updates from GitHub Releases (desktop only; can be disabled).

use crate::error::{CmdError, CmdResult};
use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
}

/// Checks GitHub Releases for a newer, signed version.
#[tauri::command]
#[specta::specta]
pub async fn update_check(app: tauri::AppHandle) -> CmdResult<Option<UpdateInfo>> {
    #[cfg(desktop)]
    {
        use tauri_plugin_updater::UpdaterExt;
        let updater = app
            .updater()
            .map_err(|e| CmdError::internal(e.to_string()))?;
        let update = updater
            .check()
            .await
            .map_err(|e| CmdError::internal(e.to_string()))?;
        Ok(update.map(|u| UpdateInfo {
            version: u.version,
            notes: u.body,
        }))
    }
    #[cfg(mobile)]
    {
        let _ = app;
        Ok(None)
    }
}

/// Downloads, verifies (minisign) and installs the update, then restarts.
#[tauri::command]
#[specta::specta]
pub async fn update_install(app: tauri::AppHandle) -> CmdResult<()> {
    #[cfg(desktop)]
    {
        use tauri_plugin_updater::UpdaterExt;
        let updater = app
            .updater()
            .map_err(|e| CmdError::internal(e.to_string()))?;
        let Some(update) = updater
            .check()
            .await
            .map_err(|e| CmdError::internal(e.to_string()))?
        else {
            return Ok(());
        };
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|e| CmdError::internal(e.to_string()))?;
        app.restart();
    }
    #[cfg(mobile)]
    {
        let _ = app;
        Err(CmdError::invalid(
            "updates are delivered through the app store",
        ))
    }
}
