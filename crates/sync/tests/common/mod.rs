// SPDX-License-Identifier: GPL-3.0-or-later
//! Test helpers shared by the integration tests.

#![allow(clippy::unwrap_used, dead_code)]

use std::path::Path;
use vault::{CreateOptions, KdfParams, SymmetricKey, Vault, unwrap_exported_key};

pub fn opts() -> CreateOptions<'static> {
    CreateOptions {
        password: Some(b"pw"),
        device_key: None,
        kdf: KdfParams::insecure_for_tests(),
    }
}

pub fn new_vault(dir: &Path, name: &str) -> Vault {
    Vault::create(dir.join(name), &opts()).unwrap()
}

/// A second device of the same vault (as after pairing).
pub fn replica(of: &Vault, dir: &Path, name: &str) -> Vault {
    let t = SymmetricKey::generate().unwrap();
    let w = of.export_wrapped_key(&t).unwrap();
    let vk = unwrap_exported_key(&t, &w, of.vault_id()).unwrap();
    Vault::create_replica(dir.join(name), of.vault_id(), &vk, &opts()).unwrap()
}

pub fn add_host(v: &mut Vault, label: &str) -> core_model::Host {
    let h = core_model::Host::new(label, "10.0.0.1");
    v.put(&h).unwrap();
    h
}

pub fn labels(v: &Vault) -> Vec<String> {
    let mut l: Vec<String> = v
        .list::<core_model::Host>()
        .unwrap()
        .into_iter()
        .map(|h| h.label)
        .collect();
    l.sort();
    l
}
