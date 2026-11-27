// SPDX-License-Identifier: GPL-3.0-or-later
//! Bridge to the Kotlin and Swift implementations.

use crate::{Error, HardwareKey, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tauri::plugin::{PluginApi, PluginHandle};
use tauri::{AppHandle, Runtime};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_vaultport_mobile);

pub struct Mobile<R: Runtime>(PluginHandle<R>);

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> std::result::Result<Mobile<R>, Box<dyn std::error::Error>> {
    #[cfg(target_os = "android")]
    let handle = api.register_android_plugin("dev.niessen.vaultport.mobile", "MobilePlugin")?;
    #[cfg(target_os = "ios")]
    let handle = api.register_ios_plugin(init_plugin_vaultport_mobile)?;
    Ok(Mobile(handle))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KeyArgs<'a> {
    key: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<&'a str>,
}

#[derive(Deserialize)]
struct ValueResponse {
    value: Option<String>,
}

#[derive(Deserialize)]
struct Empty {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KeepAliveArgs {
    sessions: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SecureArgs {
    secure: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateArgs<'a> {
    label: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SignArgs<'a> {
    alias: &'a str,
    data: &'a str,
    reason: &'a str,
}

#[derive(Deserialize)]
struct SignResponse {
    signature: String,
}

impl<R: Runtime> Mobile<R> {
    fn call<A: Serialize, T: DeserializeOwned>(&self, cmd: &str, args: A) -> Result<T> {
        self.0
            .run_mobile_plugin(cmd, args)
            .map_err(|e| Error::Native(e.to_string()))
    }

    pub fn secure_set(&self, key: &str, value: &str) -> Result<()> {
        self.call::<_, Empty>(
            "secureSet",
            KeyArgs {
                key,
                value: Some(value),
            },
        )
        .map(|_| ())
    }

    pub fn secure_get(&self, key: &str) -> Result<Option<String>> {
        self.call::<_, ValueResponse>("secureGet", KeyArgs { key, value: None })
            .map(|r| r.value)
    }

    pub fn secure_delete(&self, key: &str) -> Result<()> {
        self.call::<_, Empty>("secureDelete", KeyArgs { key, value: None })
            .map(|_| ())
    }

    pub fn keep_alive(&self, sessions: u32) -> Result<()> {
        self.call::<_, Empty>("keepAlive", KeepAliveArgs { sessions })
            .map(|_| ())
    }

    pub fn set_secure_screen(&self, secure: bool) -> Result<()> {
        self.call::<_, Empty>("setSecureScreen", SecureArgs { secure })
            .map(|_| ())
    }

    pub fn hardware_key_create(&self, label: &str) -> Result<HardwareKey> {
        self.call("hardwareKeyCreate", CreateArgs { label })
    }

    pub fn hardware_key_sign(&self, alias: &str, data: &str, reason: &str) -> Result<String> {
        self.call::<_, SignResponse>(
            "hardwareKeySign",
            SignArgs {
                alias,
                data,
                reason,
            },
        )
        .map(|r| r.signature)
    }

    pub fn hardware_key_delete(&self, alias: &str) -> Result<()> {
        self.call::<_, Empty>("hardwareKeyDelete", CreateArgs { label: alias })
            .map(|_| ())
    }
}
