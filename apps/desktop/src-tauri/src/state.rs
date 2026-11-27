// SPDX-License-Identifier: GPL-3.0-or-later
//! Application state shared by all commands.

use crate::error::{CmdError, CmdResult, ErrorCode};
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use vault::Vault;

/// Name of the active vault plus the open vault itself.
pub struct OpenVault {
    pub name: String,
    pub vault: Vault,
}

/// Global state managed by Tauri.
pub struct AppState {
    pub data_dir: PathBuf,
    pub vault: Arc<Mutex<Option<OpenVault>>>,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            vault: Arc::new(Mutex::new(None)),
        }
    }

    pub fn vaults_dir(&self) -> PathBuf {
        self.data_dir.join("vaults")
    }

    pub fn vault_path(&self, name: &str) -> CmdResult<PathBuf> {
        validate_vault_name(name)?;
        Ok(self.vaults_dir().join(format!("{name}.vault")))
    }

    /// Runs `f` with the unlocked vault.
    pub fn with_vault<T>(&self, f: impl FnOnce(&mut Vault) -> CmdResult<T>) -> CmdResult<T> {
        let mut guard = self.vault.lock();
        let open = guard
            .as_mut()
            .ok_or_else(|| CmdError::new(ErrorCode::NoVault, "no vault is open"))?;
        if open.vault.is_locked() {
            return Err(CmdError::new(ErrorCode::Locked, "the vault is locked"));
        }
        f(&mut open.vault)
    }
}

/// Vault names become file names: keep them strictly alphanumeric to rule out traversal.
pub fn validate_vault_name(name: &str) -> CmdResult<()> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(())
    } else {
        Err(CmdError::invalid(
            "vault names may only contain letters, digits, '-' and '_'",
        ))
    }
}

/// Lists vault names present in `dir`.
pub fn list_vault_files(dir: &Path) -> CmdResult<Vec<String>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("vault")
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            && validate_vault_name(stem).is_ok()
        {
            names.push(stem.to_owned());
        }
    }
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_names_are_safe() {
        assert!(validate_vault_name("personal").is_ok());
        assert!(validate_vault_name("work-2").is_ok());
        for bad in ["", "../x", "a/b", "a.b", "a b", &"x".repeat(65)] {
            assert!(validate_vault_name(bad).is_err(), "{bad}");
        }
    }
}
