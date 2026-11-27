// SPDX-License-Identifier: GPL-3.0-or-later
//! Typed, read-only views of the settings documents the web UI writes to the vault.
//!
//! The defaults here must match the `DEFAULT_*` objects in the TypeScript settings modules:
//! a missing document, a missing field or a locked vault all read as the default.

use crate::state::AppState;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tauri::Manager;

/// The `security` settings document (`features/settings/security.ts`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(mobile, allow(dead_code))]
pub struct SecurityPrefs {
    pub lock_on_sleep: bool,
}

impl Default for SecurityPrefs {
    fn default() -> Self {
        Self {
            lock_on_sleep: true,
        }
    }
}

/// The `terminal` settings document (`features/terminal/settings.ts`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TerminalPrefs {
    /// Program for local terminals; empty means the system default.
    pub local_shell: String,
    pub shell_integration: bool,
    pub import_history: bool,
    pub detect_os: bool,
}

impl Default for TerminalPrefs {
    fn default() -> Self {
        Self {
            local_shell: String::new(),
            shell_integration: false,
            import_history: false,
            detect_os: true,
        }
    }
}

/// The `logging` settings document (`features/settings/logging.ts`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LoggingPrefs {
    pub enabled: bool,
}

impl Default for LoggingPrefs {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[cfg_attr(mobile, allow(dead_code))]
pub const SECURITY: &str = "security";
pub const TERMINAL: &str = "terminal";
pub const LOGGING: &str = "logging";

/// Parses a settings document, falling back to the defaults for anything unreadable.
pub fn parse<T: DeserializeOwned + Default>(value: Option<serde_json::Value>) -> T {
    value
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// Reads the settings document `name` from the open vault.
pub fn load<T: DeserializeOwned + Default>(app: &tauri::AppHandle, name: &str) -> T {
    let state = app.state::<AppState>();
    parse(
        state
            .with_vault(|v| Ok(v.get_settings(name)?))
            .ok()
            .flatten(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_documents_use_defaults() {
        assert!(parse::<SecurityPrefs>(None).lock_on_sleep);
        assert!(parse::<LoggingPrefs>(None).enabled);
        let t = parse::<TerminalPrefs>(None);
        assert_eq!(t, TerminalPrefs::default());
        assert!(t.detect_os && !t.import_history && !t.shell_integration);
        assert!(t.local_shell.is_empty());
    }

    #[test]
    fn partial_documents_keep_other_defaults() {
        let t: TerminalPrefs = parse(Some(json!({
            "theme": "nord",
            "localShell": "/usr/bin/fish",
            "importHistory": true,
        })));
        assert_eq!(t.local_shell, "/usr/bin/fish");
        assert!(t.import_history);
        assert!(t.detect_os);
        assert!(!t.shell_integration);
        let s: SecurityPrefs = parse(Some(json!({ "lockOnSleep": false, "autoLockMinutes": 5 })));
        assert!(!s.lock_on_sleep);
        let l: LoggingPrefs = parse(Some(json!({ "enabled": false })));
        assert!(!l.enabled);
    }

    #[test]
    fn invalid_documents_use_defaults() {
        assert!(parse::<LoggingPrefs>(Some(json!("on"))).enabled);
        assert!(parse::<SecurityPrefs>(Some(json!({ "lockOnSleep": "no" }))).lock_on_sleep);
    }
}
