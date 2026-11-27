// SPDX-License-Identifier: GPL-3.0-or-later
//! Glue between the core and the native mobile plugin (`tauri-plugin-ssh-client-mobile`).
//!
//! On desktop the plugin's functions are no-ops, so callers do not need `cfg` guards.

use std::sync::Arc;
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};
use tauri_plugin_ssh_client_mobile::MobileServicesExt as _;

static APP: OnceLock<AppHandle> = OnceLock::new();

/// Remembers the app handle for code without one (the key store) and turns on screen privacy.
pub fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
    if let Err(e) = app.mobile_services().set_secure_screen(true) {
        log::warn!("could not enable screen privacy: {e}");
    }
}

/// The app handle, once [`init`] ran.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn app() -> Option<&'static AppHandle> {
    APP.get()
}

/// Updates the keep-alive service with the number of open sessions and tunnels.
pub fn update_keep_alive(app: &AppHandle) {
    let sessions = app
        .try_state::<crate::sessions::SessionManager>()
        .map_or(0, |m| m.count());
    let tunnels = app
        .try_state::<crate::commands::forwarding::ForwardManager>()
        .map_or(0, |m| m.count());
    let total = u32::try_from(sessions + tunnels).unwrap_or(u32::MAX);
    if let Err(e) = app.mobile_services().keep_alive(total) {
        log::warn!("keep-alive update failed: {e}");
    }
}

/// The backend for hardware-bound SSH keys on this platform.
pub fn hardware_backend() -> Arc<dyn hw_keys::platform::PlatformKeyBackend> {
    #[cfg(target_os = "android")]
    if let Some(app) = app() {
        return Arc::new(android_backend(app.clone()));
    }
    hw_keys::platform::default_backend()
}

#[cfg(target_os = "android")]
fn android_backend(app: AppHandle) -> hw_keys::platform::BridgeBackend {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use hw_keys::platform::{BridgeBackend, BridgeKey, BridgeSignatureEncoding};

    fn native(e: impl std::fmt::Display) -> hw_keys::Error {
        let msg = e.to_string();
        if msg.to_ascii_lowercase().contains("cancel") {
            hw_keys::Error::Cancelled
        } else {
            hw_keys::Error::Platform(msg)
        }
    }

    async fn blocking<T: Send + 'static>(
        f: impl FnOnce() -> hw_keys::Result<T> + Send + 'static,
    ) -> hw_keys::Result<T> {
        tauri::async_runtime::spawn_blocking(f)
            .await
            .map_err(|e| hw_keys::Error::Platform(e.to_string()))?
    }

    let sign_app = app.clone();
    let create_app = app.clone();
    BridgeBackend::new("Android Keystore", move |alias, data| {
        let app = sign_app.clone();
        blocking(move || {
            let sig = app
                .mobile_services()
                .hardware_key_sign(&alias, &STANDARD.encode(&data), "Sign in with your SSH key")
                .map_err(native)?;
            STANDARD.decode(sig).map_err(native)
        })
    })
    .with_signature_encoding(BridgeSignatureEncoding::Der)
    .with_create(move |label, _policy| {
        let app = create_app.clone();
        blocking(move || {
            let key = app
                .mobile_services()
                .hardware_key_create(&label)
                .map_err(native)?;
            Ok(BridgeKey {
                platform_ref: key.alias,
                public_key: STANDARD.decode(key.public_key).map_err(native)?,
            })
        })
    })
    .with_delete(move |alias| {
        let app = app.clone();
        blocking(move || {
            app.mobile_services()
                .hardware_key_delete(&alias)
                .map_err(native)
        })
    })
}
