// SPDX-License-Identifier: GPL-3.0-or-later
//! Native mobile services for Vaultport.
//!
//! The plugin exposes **no commands to the web UI**: it is used only from the Rust core.
//!
//! * **Secure store**: small secrets (the vault's device key) in the Android Keystore
//!   (AES-256-GCM key that never leaves secure hardware, ciphertext in private preferences) or
//!   the iOS Keychain (`kSecAttrAccessibleWhenUnlockedThisDeviceOnly`).
//! * **Keep-alive**: an Android foreground service with a persistent notification while
//!   sessions or tunnels are open; on iOS a background task extends the grace period.
//! * **Screen privacy**: `FLAG_SECURE` on Android (no screenshots, blank app-switcher preview).
//! * **Hardware keys**: non-exportable P-256 keys in the Android Keystore (StrongBox when
//!   available) gated by `BiometricPrompt`, signing `SHA256withECDSA` for SSH.
//!
//! On desktop every function is a no-op or returns [`Error::Unsupported`].

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

#[cfg(mobile)]
mod mobile;

/// Errors returned by the native side.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not supported on this platform")]
    Unsupported,
    #[error("{0}")]
    Native(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// A hardware key created in the platform key store.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareKey {
    /// Key store alias.
    pub alias: String,
    /// DER `SubjectPublicKeyInfo`, base64.
    pub public_key: String,
}

/// Handle to the native services (stored in the app state).
pub struct MobileServices<R: Runtime> {
    #[cfg(mobile)]
    inner: mobile::Mobile<R>,
    #[cfg(not(mobile))]
    _marker: std::marker::PhantomData<fn() -> R>,
}

impl<R: Runtime> std::fmt::Debug for MobileServices<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MobileServices").finish_non_exhaustive()
    }
}

// The desktop stubs ignore `self` and never fail; the signatures match the mobile build.
#[cfg_attr(not(mobile), allow(clippy::unused_self, clippy::unnecessary_wraps))]
impl<R: Runtime> MobileServices<R> {
    /// Stores a secret value under `key`.
    pub fn secure_set(&self, key: &str, value: &str) -> Result<()> {
        #[cfg(mobile)]
        return self.inner.secure_set(key, value);
        #[cfg(not(mobile))]
        {
            let _ = (key, value);
            Err(Error::Unsupported)
        }
    }

    /// Reads a secret value.
    pub fn secure_get(&self, key: &str) -> Result<Option<String>> {
        #[cfg(mobile)]
        return self.inner.secure_get(key);
        #[cfg(not(mobile))]
        {
            let _ = key;
            Err(Error::Unsupported)
        }
    }

    /// Deletes a secret value.
    pub fn secure_delete(&self, key: &str) -> Result<()> {
        #[cfg(mobile)]
        return self.inner.secure_delete(key);
        #[cfg(not(mobile))]
        {
            let _ = key;
            Err(Error::Unsupported)
        }
    }

    /// Starts (or updates) the keep-alive service with the number of open sessions; `0` stops it.
    pub fn keep_alive(&self, sessions: u32) -> Result<()> {
        #[cfg(mobile)]
        return self.inner.keep_alive(sessions);
        #[cfg(not(mobile))]
        {
            let _ = sessions;
            Ok(())
        }
    }

    /// Prevents screenshots and hides the app-switcher preview.
    pub fn set_secure_screen(&self, secure: bool) -> Result<()> {
        #[cfg(mobile)]
        return self.inner.set_secure_screen(secure);
        #[cfg(not(mobile))]
        {
            let _ = secure;
            Ok(())
        }
    }

    /// Creates a biometric-gated P-256 key in secure hardware.
    pub fn hardware_key_create(&self, label: &str) -> Result<HardwareKey> {
        #[cfg(mobile)]
        return self.inner.hardware_key_create(label);
        #[cfg(not(mobile))]
        {
            let _ = label;
            Err(Error::Unsupported)
        }
    }

    /// Signs `data` (base64) with a hardware key after biometric confirmation. Returns the DER
    /// ECDSA signature (base64).
    pub fn hardware_key_sign(&self, alias: &str, data_b64: &str, reason: &str) -> Result<String> {
        #[cfg(mobile)]
        return self.inner.hardware_key_sign(alias, data_b64, reason);
        #[cfg(not(mobile))]
        {
            let _ = (alias, data_b64, reason);
            Err(Error::Unsupported)
        }
    }

    /// Deletes a hardware key.
    pub fn hardware_key_delete(&self, alias: &str) -> Result<()> {
        #[cfg(mobile)]
        return self.inner.hardware_key_delete(alias);
        #[cfg(not(mobile))]
        {
            let _ = alias;
            Err(Error::Unsupported)
        }
    }
}

/// Access to the services from an app handle.
pub trait MobileServicesExt<R: Runtime> {
    fn mobile_services(&self) -> &MobileServices<R>;
}

impl<R: Runtime, T: Manager<R>> MobileServicesExt<R> for T {
    fn mobile_services(&self) -> &MobileServices<R> {
        self.state::<MobileServices<R>>().inner()
    }
}

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("vaultport-mobile")
        .setup(|app, api| {
            #[cfg(mobile)]
            let services = MobileServices {
                inner: mobile::init(app, api)?,
            };
            #[cfg(not(mobile))]
            let services = {
                let _ = api;
                MobileServices::<R> {
                    _marker: std::marker::PhantomData,
                }
            };
            app.manage(services);
            Ok(())
        })
        .build()
}
