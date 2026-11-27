// SPDX-License-Identifier: GPL-3.0-or-later
//! Platform information used by the UI to render the native-looking title bar and layout.

use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum PlatformKind {
    Macos,
    Windows,
    Linux,
    Ios,
    Android,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub kind: PlatformKind,
    pub mobile: bool,
    pub arch: String,
    pub version: String,
    /// Linux: button layout from `gtk-decoration-layout` (e.g. `"appmenu:minimize,maximize,close"`).
    pub decoration_layout: Option<String>,
    /// Whether the window uses native decorations (no custom title bar controls needed).
    pub native_decorations: bool,
    /// Whether this build can open local terminals (not available in sandboxed builds / mobile).
    pub local_terminal: bool,
}

const fn kind() -> PlatformKind {
    if cfg!(target_os = "macos") {
        PlatformKind::Macos
    } else if cfg!(target_os = "windows") {
        PlatformKind::Windows
    } else if cfg!(target_os = "ios") {
        PlatformKind::Ios
    } else if cfg!(target_os = "android") {
        PlatformKind::Android
    } else {
        PlatformKind::Linux
    }
}

#[cfg(target_os = "linux")]
fn decoration_layout() -> Option<String> {
    // Prefer the GNOME setting; fall back to the classic right-aligned layout.
    std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.wm.preferences", "button-layout"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().trim_matches('\'').to_owned())
        .filter(|s| !s.is_empty())
}

#[cfg(not(target_os = "linux"))]
const fn decoration_layout() -> Option<String> {
    None
}

/// Returns information about the platform the app is running on.
#[tauri::command]
#[specta::specta]
pub fn platform_info() -> PlatformInfo {
    let k = kind();
    let mobile = matches!(k, PlatformKind::Ios | PlatformKind::Android);
    PlatformInfo {
        kind: k,
        mobile,
        arch: std::env::consts::ARCH.to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        decoration_layout: decoration_layout(),
        native_decorations: std::env::var_os("SSH_CLIENT_NATIVE_DECORATIONS").is_some() || mobile,
        local_terminal: !mobile,
    }
}
