// SPDX-License-Identifier: GPL-3.0-or-later
//! Self-contained encrypted vault files (`.sshvault`).
//!
//! # Format (version 1)
//!
//! ```text
//! "SSHVAULT"                  8 bytes magic
//! version                     u16, big endian (= 1)
//! header length               u32, big endian
//! header                      JSON: { vaultId, kdf: { memoryKib, iterations, parallelism,
//!                             salt }, createdAt, opCount, wrappedKey }
//! payload                     XChaCha20-Poly1305(export key, aad, json([SealedOp]))
//! ```
//!
//! * `wrappedKey` is the vault key wrapped (via [`Vault::export_wrapped_key`]) with
//!   `Argon2id(passphrase, kdf)`; the KDF parameters and salt travel in the header.
//! * The payload key is an HKDF subkey of the vault key; its AAD is
//!   `"ssh-client/export/v1|" || SHA-256(header)`, so any change to the header (or the
//!   payload) is detected.
//! * The ops inside are the vault's own sealed ops, so importing yields exactly the same
//!   CRDT state, and the imported replica can keep syncing with the original devices.

use crate::error::SyncError;
use crate::keys::{Purpose, now_ms, subkey, vault_key};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use uuid::Uuid;
use vault::{CreateOptions, KdfParams, SealedOp, Vault, unwrap_exported_key};

const MAGIC: &[u8; 8] = b"SSHVAULT";
/// Current file format version.
pub const EXPORT_VERSION: u16 = 1;
const MAX_HEADER: usize = 64 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    vault_id: Uuid,
    kdf: KdfParams,
    created_at: i64,
    op_count: usize,
    wrapped_key: String,
}

fn payload_aad(header: &[u8]) -> Vec<u8> {
    let mut aad = b"ssh-client/export/v1|".to_vec();
    aad.extend_from_slice(&Sha256::digest(header));
    aad
}

/// Exports all syncable data of an unlocked vault, protected by `passphrase`, using Argon2id
/// parameters suitable for phones (64 MiB, t=3) so the file can be imported anywhere.
pub fn export_file(vault: &Vault, passphrase: &[u8]) -> Result<Vec<u8>, SyncError> {
    export_file_with(vault, passphrase, KdfParams::mobile()?)
}

/// Like [`export_file`] with explicit KDF parameters (a fresh salt is recommended:
/// [`KdfParams::with_cost`]).
pub fn export_file_with(
    vault: &Vault,
    passphrase: &[u8],
    kdf: KdfParams,
) -> Result<Vec<u8>, SyncError> {
    if passphrase.is_empty() {
        return Err(SyncError::Format("the passphrase must not be empty".into()));
    }
    let vault_id = vault.vault_id();
    let ops = vault.all_syncable_ops()?;
    let kek = kdf.derive(passphrase)?;
    let header = Header {
        vault_id,
        kdf,
        created_at: now_ms(),
        op_count: ops.len(),
        wrapped_key: vault.export_wrapped_key(&kek)?,
    };
    let header = serde_json::to_vec(&header).map_err(|e| SyncError::Format(e.to_string()))?;
    let vk = vault_key(vault)?;
    let ek = subkey(&vk, vault_id, Purpose::Export);
    let payload = zeroize::Zeroizing::new(
        serde_json::to_vec(&ops).map_err(|e| SyncError::Format(e.to_string()))?,
    );
    let sealed = ek.seal(&payload_aad(&header), &payload)?;

    let header_len =
        u32::try_from(header.len()).map_err(|_| SyncError::Format("header too large".into()))?;
    let mut out = Vec::with_capacity(14 + header.len() + sealed.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&EXPORT_VERSION.to_be_bytes());
    out.extend_from_slice(&header_len.to_be_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// Summary of an export file readable without the passphrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportInfo {
    pub vault_id: Uuid,
    pub created_at: i64,
    pub op_count: usize,
}

fn parse(bytes: &[u8]) -> Result<(Header, &[u8], &[u8]), SyncError> {
    let bad = || SyncError::Format("not an ssh-client vault file".into());
    let rest = bytes.strip_prefix(MAGIC.as_slice()).ok_or_else(bad)?;
    let (version, rest) = rest.split_at_checked(2).ok_or_else(bad)?;
    let version = u16::from_be_bytes([version[0], version[1]]);
    if version != EXPORT_VERSION {
        return Err(SyncError::Format(format!(
            "unsupported vault file version {version}"
        )));
    }
    let (len, rest) = rest.split_at_checked(4).ok_or_else(bad)?;
    let len = u32::from_be_bytes([len[0], len[1], len[2], len[3]]) as usize;
    if len > MAX_HEADER {
        return Err(bad());
    }
    let (header_bytes, payload) = rest.split_at_checked(len).ok_or_else(bad)?;
    let header: Header =
        serde_json::from_slice(header_bytes).map_err(|e| SyncError::Format(e.to_string()))?;
    Ok((header, header_bytes, payload))
}

/// Reads the unencrypted header of an export file.
pub fn inspect_file(bytes: &[u8]) -> Result<ExportInfo, SyncError> {
    let (h, _, _) = parse(bytes)?;
    Ok(ExportInfo {
        vault_id: h.vault_id,
        created_at: h.created_at,
        op_count: h.op_count,
    })
}

/// Imports an export file into a **new** local replica at `dest` (which must not exist),
/// protected by the local unlock methods in `local`. The replica gets its own device id.
/// Fails with [`SyncError::Authentication`] for a wrong passphrase or a modified file; in
/// that case nothing is created.
pub fn import_file(
    bytes: &[u8],
    passphrase: &[u8],
    dest: &Path,
    local: &CreateOptions<'_>,
) -> Result<Vault, SyncError> {
    let (header, header_bytes, payload) = parse(bytes)?;
    let kek = header.kdf.derive(passphrase)?;
    let vk = unwrap_exported_key(&kek, &header.wrapped_key, header.vault_id)?;
    let ek = subkey(&vk, header.vault_id, Purpose::Export);
    let plain = ek.open(&payload_aad(header_bytes), payload)?;
    let ops: Vec<SealedOp> =
        serde_json::from_slice(&plain).map_err(|e| SyncError::Format(e.to_string()))?;
    drop(plain);

    let mut vault = Vault::create_replica(dest, header.vault_id, &vk, local)?;
    if let Err(e) = vault.apply_remote(&ops) {
        drop(vault);
        for suffix in ["", "-wal", "-shm"] {
            let mut p = dest.as_os_str().to_owned();
            p.push(suffix);
            let _ = std::fs::remove_file(p);
        }
        return Err(e.into());
    }
    Ok(vault)
}
