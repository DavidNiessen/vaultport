// SPDX-License-Identifier: GPL-3.0-or-later
//! The encrypted vault store.

use crate::crdt::{Change, Doc, Op, diff};
use crate::crypto::{KdfParams, SymmetricKey, random_bytes};
use crate::entity::{Entity, SECRET_KIND, SETTINGS_KIND};
use crate::error::VaultError;
use crate::hlc::{Clock, DeviceId, Hlc};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use core_model::SecretRef;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zeroize::Zeroizing;

/// On-disk format version.
pub const FORMAT_VERSION: u32 = 1;

const SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS ops (
    seq    INTEGER PRIMARY KEY AUTOINCREMENT,
    id     TEXT    NOT NULL UNIQUE,
    local  INTEGER NOT NULL,
    remote INTEGER NOT NULL,
    blob   BLOB    NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS docs (
    record TEXT PRIMARY KEY NOT NULL,
    blob   BLOB NOT NULL
) STRICT;
";

/// A key-encryption-key wrapped copy of the vault key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Wrapped {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kdf: Option<KdfParams>,
    blob: String,
}

/// Plaintext vault header. It contains no secrets: wrapped keys are AEAD ciphertexts bound to
/// the vault id and the unlock method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    version: u32,
    vault_id: Uuid,
    created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    password: Option<Wrapped>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    device: Option<Wrapped>,
}

/// How a vault can be unlocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockMethods {
    pub password: bool,
    pub device_key: bool,
}

/// Options for creating a vault.
#[derive(Debug)]
pub struct CreateOptions<'a> {
    pub password: Option<&'a [u8]>,
    pub device_key: Option<&'a SymmetricKey>,
    pub kdf: KdfParams,
}

/// Summary of a change applied to the vault, used to notify the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEvent {
    pub kind: String,
    pub record: String,
    pub deleted: bool,
}

/// A serialized op ready for replication. The blob is encrypted with the vault key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SealedOp {
    pub id: Uuid,
    #[serde(with = "b64_bytes")]
    pub blob: Vec<u8>,
}

mod b64_bytes {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(v))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        STANDARD.decode(s).map_err(serde::de::Error::custom)
    }
}

struct Unlocked {
    key: SymmetricKey,
    docs: HashMap<String, Doc>,
}

/// An encrypted, CRDT-backed vault stored in a single SQLite file.
pub struct Vault {
    path: PathBuf,
    conn: Connection,
    header: Header,
    clock: Clock,
    state: Option<Unlocked>,
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault")
            .field("path", &self.path)
            .field("vault_id", &self.header.vault_id)
            .field("locked", &self.state.is_none())
            .finish_non_exhaustive()
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn open_connection(path: &Path) -> Result<Connection, VaultError> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA secure_delete = ON;
         PRAGMA foreign_keys = ON;
         PRAGMA trusted_schema = OFF;",
    )?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<(), VaultError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<(), VaultError> {
    Ok(())
}

impl Vault {
    /// Creates a new vault at `path`. Fails if a vault already exists there.
    pub fn create(path: impl AsRef<Path>, opts: &CreateOptions<'_>) -> Result<Self, VaultError> {
        let path = path.as_ref().to_path_buf();
        if path.exists() {
            return Err(VaultError::Invalid(format!(
                "{} already exists",
                path.display()
            )));
        }
        if opts.password.is_none() && opts.device_key.is_none() {
            return Err(VaultError::LastUnlockMethod);
        }
        let conn = open_connection(&path)?;
        restrict_permissions(&path)?;
        let vault_id = Uuid::new_v4();
        let vk = SymmetricKey::generate()?;
        let mut header = Header {
            version: FORMAT_VERSION,
            vault_id,
            created_at: now_ms(),
            password: None,
            device: None,
        };
        if let Some(pw) = opts.password {
            header.password = Some(wrap_password(&vk, pw, opts.kdf.clone(), vault_id)?);
        }
        if let Some(dk) = opts.device_key {
            header.device = Some(wrap_device(&vk, dk, vault_id)?);
        }
        let mut dev = [0u8; 8];
        random_bytes(&mut dev)?;
        let device = DeviceId(u64::from_le_bytes(dev));
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('header', ?1), ('device', ?2)",
            params![serde_json::to_string(&header)?, device.to_string()],
        )?;
        Ok(Self {
            path,
            conn,
            header,
            clock: Clock::new(device),
            state: Some(Unlocked {
                key: vk,
                docs: HashMap::new(),
            }),
        })
    }

    /// Creates a local replica of an existing vault on a newly paired device. `vault_key` is
    /// the key received through the pairing channel; the new device gets its own device id and
    /// its own unlock wraps.
    pub fn create_replica(
        path: impl AsRef<Path>,
        vault_id: Uuid,
        vault_key: &SymmetricKey,
        opts: &CreateOptions<'_>,
    ) -> Result<Self, VaultError> {
        let path = path.as_ref().to_path_buf();
        if path.exists() {
            return Err(VaultError::Invalid(format!(
                "{} already exists",
                path.display()
            )));
        }
        if opts.password.is_none() && opts.device_key.is_none() {
            return Err(VaultError::LastUnlockMethod);
        }
        let conn = open_connection(&path)?;
        restrict_permissions(&path)?;
        let mut header = Header {
            version: FORMAT_VERSION,
            vault_id,
            created_at: now_ms(),
            password: None,
            device: None,
        };
        if let Some(pw) = opts.password {
            header.password = Some(wrap_password(vault_key, pw, opts.kdf.clone(), vault_id)?);
        }
        if let Some(dk) = opts.device_key {
            header.device = Some(wrap_device(vault_key, dk, vault_id)?);
        }
        let mut dev = [0u8; 8];
        random_bytes(&mut dev)?;
        let device = DeviceId(u64::from_le_bytes(dev));
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('header', ?1), ('device', ?2)",
            params![serde_json::to_string(&header)?, device.to_string()],
        )?;
        Ok(Self {
            path,
            conn,
            header,
            clock: Clock::new(device),
            state: Some(Unlocked {
                key: vault_key.clone(),
                docs: HashMap::new(),
            }),
        })
    }

    /// Opens an existing vault in the locked state.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, VaultError> {
        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            return Err(VaultError::NotFound(path.display().to_string()));
        }
        let conn = open_connection(&path)?;
        let header: String = conn
            .query_row("SELECT value FROM meta WHERE key = 'header'", [], |r| {
                r.get(0)
            })
            .optional()?
            .ok_or_else(|| VaultError::Corrupt("missing header".into()))?;
        let header: Header = serde_json::from_str(&header)?;
        if header.version > FORMAT_VERSION {
            return Err(VaultError::UnsupportedVersion(header.version));
        }
        let device: String = conn
            .query_row("SELECT value FROM meta WHERE key = 'device'", [], |r| {
                r.get(0)
            })
            .optional()?
            .ok_or_else(|| VaultError::Corrupt("missing device id".into()))?;
        let device = u64::from_str_radix(&device, 16)
            .map_err(|_| VaultError::Corrupt("bad device id".into()))?;
        Ok(Self {
            path,
            conn,
            header,
            clock: Clock::new(DeviceId(device)),
            state: None,
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn vault_id(&self) -> Uuid {
        self.header.vault_id
    }

    #[must_use]
    pub fn device_id(&self) -> DeviceId {
        self.clock.device()
    }

    #[must_use]
    pub fn is_locked(&self) -> bool {
        self.state.is_none()
    }

    #[must_use]
    pub fn unlock_methods(&self) -> UnlockMethods {
        UnlockMethods {
            password: self.header.password.is_some(),
            device_key: self.header.device.is_some(),
        }
    }

    /// Unlocks with the master password.
    pub fn unlock_with_password(&mut self, password: &[u8]) -> Result<(), VaultError> {
        let w = self
            .header
            .password
            .as_ref()
            .ok_or(VaultError::NoSuchUnlockMethod)?;
        let kdf = w
            .kdf
            .as_ref()
            .ok_or_else(|| VaultError::Corrupt("missing kdf parameters".into()))?;
        let kek = kdf.derive(password)?;
        let vk = unwrap(&kek, w, "password", self.header.vault_id)?;
        self.load(vk)
    }

    /// Unlocks with the device key held by the OS key store.
    pub fn unlock_with_device_key(&mut self, device_key: &SymmetricKey) -> Result<(), VaultError> {
        let w = self
            .header
            .device
            .as_ref()
            .ok_or(VaultError::NoSuchUnlockMethod)?;
        let vk = unwrap(device_key, w, "device", self.header.vault_id)?;
        self.load(vk)
    }

    /// Locks the vault, dropping the vault key and all decrypted records from memory.
    pub fn lock(&mut self) {
        self.state = None;
    }

    fn load(&mut self, key: SymmetricKey) -> Result<(), VaultError> {
        let vault_id = self.header.vault_id;
        let mut docs = HashMap::new();
        let mut stmt = self.conn.prepare("SELECT record, blob FROM docs")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?;
        let mut max_hlc = Hlc::ZERO;
        for row in rows {
            let (record, blob) = row?;
            if record.starts_with("secret:") {
                continue;
            }
            let plain = key.open(doc_aad(vault_id, &record).as_bytes(), &blob)?;
            let doc: Doc = serde_json::from_slice(&plain)?;
            max_hlc = max_hlc.max(doc.latest());
            docs.insert(record, doc);
        }
        drop(stmt);
        self.clock.observe(max_hlc);
        self.state = Some(Unlocked { key, docs });
        Ok(())
    }

    fn unlocked(&self) -> Result<&Unlocked, VaultError> {
        self.state.as_ref().ok_or(VaultError::Locked)
    }

    // ----- unlock method management -------------------------------------------------------

    /// Sets or replaces the master password.
    pub fn set_password(&mut self, password: &[u8], kdf: KdfParams) -> Result<(), VaultError> {
        let vault_id = self.header.vault_id;
        let wrapped = wrap_password(&self.unlocked()?.key, password, kdf, vault_id)?;
        let mut header = self.header.clone();
        header.password = Some(wrapped);
        self.write_header(header)
    }

    /// Removes the master password (requires a device key to remain).
    pub fn remove_password(&mut self) -> Result<(), VaultError> {
        self.unlocked()?;
        if self.header.device.is_none() {
            return Err(VaultError::LastUnlockMethod);
        }
        let mut header = self.header.clone();
        header.password = None;
        self.write_header(header)
    }

    /// Sets or replaces the device key.
    pub fn set_device_key(&mut self, device_key: &SymmetricKey) -> Result<(), VaultError> {
        let vault_id = self.header.vault_id;
        let wrapped = wrap_device(&self.unlocked()?.key, device_key, vault_id)?;
        let mut header = self.header.clone();
        header.device = Some(wrapped);
        self.write_header(header)
    }

    /// Removes the device key (requires a password to remain).
    pub fn remove_device_key(&mut self) -> Result<(), VaultError> {
        self.unlocked()?;
        if self.header.password.is_none() {
            return Err(VaultError::LastUnlockMethod);
        }
        let mut header = self.header.clone();
        header.device = None;
        self.write_header(header)
    }

    fn write_header(&mut self, header: Header) -> Result<(), VaultError> {
        self.conn.execute(
            "UPDATE meta SET value = ?1 WHERE key = 'header'",
            params![serde_json::to_string(&header)?],
        )?;
        self.header = header;
        Ok(())
    }

    /// Derives a purpose-specific 256-bit subkey from the vault key with HKDF-SHA256
    /// (e.g. `"session-log"`). Subkeys never decrypt vault records.
    pub fn derive_subkey(&self, purpose: &str) -> Result<SymmetricKey, VaultError> {
        if purpose.is_empty() || purpose.starts_with("vaultport/") {
            return Err(VaultError::Invalid("invalid subkey purpose".into()));
        }
        let vk = &self.unlocked()?.key;
        let hk =
            hkdf::Hkdf::<sha2::Sha256>::new(Some(self.header.vault_id.as_bytes()), vk.as_bytes());
        let mut out = zeroize::Zeroizing::new([0u8; crate::crypto::KEY_LEN]);
        hk.expand(
            format!("vaultport/subkey/v1|{purpose}").as_bytes(),
            out.as_mut(),
        )
        .map_err(|_| VaultError::Crypto("hkdf expand".into()))?;
        Ok(SymmetricKey::from_bytes(*out))
    }

    /// Exports the vault key wrapped with `transfer_key` for device pairing.
    pub fn export_wrapped_key(&self, transfer_key: &SymmetricKey) -> Result<String, VaultError> {
        let w = wrap_device(&self.unlocked()?.key, transfer_key, self.header.vault_id)?;
        Ok(w.blob)
    }

    // ----- typed entity access ------------------------------------------------------------

    /// Inserts or updates an entity. Returns `true` if anything changed.
    pub fn put<T: Entity>(&mut self, entity: &T) -> Result<bool, VaultError> {
        entity.check()?;
        let record = format!("{}:{}", T::KIND, entity.record_id());
        let value = serde_json::to_value(entity)?;
        if !value.is_object() {
            return Err(VaultError::Invalid(
                "entities must serialize to objects".into(),
            ));
        }
        self.write_value(T::KIND, &record, &value, entity.device_local())
    }

    /// Gets an entity by id.
    pub fn get<T: Entity>(&self, id: &str) -> Result<Option<T>, VaultError> {
        let record = format!("{}:{id}", T::KIND);
        let state = self.unlocked()?;
        match state.docs.get(&record).and_then(Doc::to_value) {
            Some(v) => Ok(Some(serde_json::from_value(v)?)),
            None => Ok(None),
        }
    }

    /// Lists all live entities of a kind. Records that fail to decode (e.g. created by a newer
    /// app version) are skipped and logged rather than failing the whole listing.
    pub fn list<T: Entity>(&self) -> Result<Vec<T>, VaultError> {
        let prefix = format!("{}:", T::KIND);
        let state = self.unlocked()?;
        let mut out = Vec::new();
        for (record, doc) in &state.docs {
            if !record.starts_with(&prefix) {
                continue;
            }
            if let Some(v) = doc.to_value() {
                match serde_json::from_value::<T>(v) {
                    Ok(e) => out.push(e),
                    Err(e) => tracing::warn!(record, error = %e, "skipping undecodable record"),
                }
            }
        }
        Ok(out)
    }

    /// Deletes an entity. Returns `true` if it existed.
    pub fn delete<T: Entity>(&mut self, id: &str) -> Result<bool, VaultError> {
        let record = format!("{}:{id}", T::KIND);
        self.delete_record(T::KIND, &record)
    }

    // ----- settings -----------------------------------------------------------------------

    /// Stores a settings document (an arbitrary JSON object) under `name`.
    pub fn put_settings(
        &mut self,
        name: &str,
        value: &Value,
        local: bool,
    ) -> Result<bool, VaultError> {
        if !value.is_object() {
            return Err(VaultError::Invalid("settings must be an object".into()));
        }
        let record = format!("{SETTINGS_KIND}:{name}");
        self.write_value(SETTINGS_KIND, &record, value, local)
    }

    /// Reads a settings document.
    pub fn get_settings(&self, name: &str) -> Result<Option<Value>, VaultError> {
        let record = format!("{SETTINGS_KIND}:{name}");
        Ok(self.unlocked()?.docs.get(&record).and_then(Doc::to_value))
    }

    // ----- secrets ------------------------------------------------------------------------

    /// Stores a secret blob (password, private key, token). Secrets are decrypted only on
    /// demand and never cached in memory.
    pub fn put_secret(&mut self, secret: &[u8], local_only: bool) -> Result<SecretRef, VaultError> {
        let id = SecretRef::new();
        self.update_secret(id, secret, local_only)?;
        Ok(id)
    }

    /// Replaces the value of an existing (or new) secret id.
    pub fn update_secret(
        &mut self,
        id: SecretRef,
        secret: &[u8],
        local_only: bool,
    ) -> Result<(), VaultError> {
        let record = format!("{SECRET_KIND}:{id}");
        let value = serde_json::json!({ "v": B64.encode(secret) });
        let old = self.read_doc(&record)?;
        let hlc = self.clock.tick();
        let mut doc = old.unwrap_or_else(|| Doc::new(SECRET_KIND));
        let change = Change::Set {
            path: "/v".into(),
            value: value["v"].clone(),
        };
        doc.apply(hlc, &change);
        let op = Op {
            id: Uuid::now_v7(),
            hlc,
            record: record.clone(),
            kind: SECRET_KIND.into(),
            change,
        };
        self.persist(&[op], &record, &doc, local_only, false)?;
        Ok(())
    }

    /// Reads a secret. The returned buffer is zeroized on drop.
    pub fn get_secret(&self, id: SecretRef) -> Result<Zeroizing<Vec<u8>>, VaultError> {
        let record = format!("{SECRET_KIND}:{id}");
        let doc = self
            .read_doc(&record)?
            .ok_or_else(|| VaultError::NotFound(format!("secret {id}")))?;
        let value = doc
            .to_value()
            .ok_or_else(|| VaultError::NotFound(format!("secret {id}")))?;
        let encoded = value
            .get("v")
            .and_then(Value::as_str)
            .ok_or_else(|| VaultError::Corrupt("secret without value".into()))?;
        let bytes = B64
            .decode(encoded)
            .map_err(|_| VaultError::Corrupt("secret encoding".into()))?;
        Ok(Zeroizing::new(bytes))
    }

    /// Deletes a secret.
    pub fn delete_secret(&mut self, id: SecretRef) -> Result<bool, VaultError> {
        let record = format!("{SECRET_KIND}:{id}");
        self.delete_record(SECRET_KIND, &record)
    }

    // ----- replication --------------------------------------------------------------------

    /// Returns locally created, syncable ops with `seq > cursor`, and the new cursor.
    pub fn local_ops_since(&self, cursor: i64) -> Result<(Vec<SealedOp>, i64), VaultError> {
        self.unlocked()?;
        let mut stmt = self.conn.prepare(
            "SELECT seq, id, blob FROM ops WHERE seq > ?1 AND local = 0 AND remote = 0 ORDER BY seq",
        )?;
        let mut max = cursor;
        let mut out = Vec::new();
        let rows = stmt.query_map([cursor], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        for row in rows {
            let (seq, id, blob) = row?;
            max = max.max(seq);
            let id = Uuid::parse_str(&id).map_err(|_| VaultError::Corrupt("op id".into()))?;
            out.push(SealedOp { id, blob });
        }
        Ok((out, max))
    }

    /// All syncable ops (local and remote) — used for full snapshots / encrypted exports.
    pub fn all_syncable_ops(&self) -> Result<Vec<SealedOp>, VaultError> {
        self.unlocked()?;
        let mut stmt = self
            .conn
            .prepare("SELECT id, blob FROM ops WHERE local = 0 ORDER BY seq")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, blob) = row?;
            let id = Uuid::parse_str(&id).map_err(|_| VaultError::Corrupt("op id".into()))?;
            out.push(SealedOp { id, blob });
        }
        Ok(out)
    }

    /// Applies ops received from another device. Unknown or duplicate ops are ignored; ops that
    /// fail authentication abort the import (the backend may be hostile).
    pub fn apply_remote(&mut self, ops: &[SealedOp]) -> Result<Vec<ChangeEvent>, VaultError> {
        let vault_id = self.header.vault_id;
        let key = self.unlocked()?.key.clone();
        let mut decoded = Vec::with_capacity(ops.len());
        for sealed in ops {
            let exists: bool = self.conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM ops WHERE id = ?1)",
                [sealed.id.to_string()],
                |r| r.get(0),
            )?;
            if exists {
                continue;
            }
            let plain = key.open(op_aad(vault_id, sealed.id).as_bytes(), &sealed.blob)?;
            let op: Op = serde_json::from_slice(&plain)?;
            if op.id != sealed.id {
                return Err(VaultError::Corrupt("op id mismatch".into()));
            }
            decoded.push((sealed, op));
        }
        let mut events = Vec::new();
        let tx = self.conn.unchecked_transaction()?;
        for (sealed, op) in decoded {
            self.clock.observe(op.hlc);
            let mut doc = match self.state.as_ref().and_then(|s| s.docs.get(&op.record)) {
                Some(d) => d.clone(),
                None => read_doc_raw(&tx, &key, vault_id, &op.record)?
                    .unwrap_or_else(|| Doc::new(op.kind.clone())),
            };
            let before = doc.to_value();
            doc.apply(op.hlc, &op.change);
            tx.execute(
                "INSERT INTO ops (id, local, remote, blob) VALUES (?1, 0, 1, ?2)",
                params![sealed.id.to_string(), sealed.blob],
            )?;
            let blob = key.seal(
                doc_aad(vault_id, &op.record).as_bytes(),
                &serde_json::to_vec(&doc)?,
            )?;
            tx.execute(
                "INSERT INTO docs (record, blob) VALUES (?1, ?2)
                 ON CONFLICT(record) DO UPDATE SET blob = excluded.blob",
                params![op.record, blob],
            )?;
            let after = doc.to_value();
            if before != after {
                events.push(ChangeEvent {
                    kind: doc.kind.clone(),
                    record: op.record.clone(),
                    deleted: after.is_none(),
                });
            }
            if !op.record.starts_with("secret:")
                && let Some(state) = self.state.as_mut()
            {
                state.docs.insert(op.record.clone(), doc);
            }
        }
        tx.commit()?;
        Ok(events)
    }

    // ----- internals ----------------------------------------------------------------------

    fn read_doc(&self, record: &str) -> Result<Option<Doc>, VaultError> {
        let state = self.unlocked()?;
        if let Some(d) = state.docs.get(record) {
            return Ok(Some(d.clone()));
        }
        read_doc_raw(&self.conn, &state.key, self.header.vault_id, record)
    }

    fn write_value(
        &mut self,
        kind: &str,
        record: &str,
        value: &Value,
        local: bool,
    ) -> Result<bool, VaultError> {
        let old = self.read_doc(record)?;
        let old_value = old.as_ref().and_then(Doc::to_value);
        let changes = diff(old_value.as_ref(), value);
        let resurrect = old.as_ref().is_some_and(Doc::is_deleted);
        if changes.is_empty() && !resurrect {
            return Ok(false);
        }
        let hlc = self.clock.tick();
        let mut doc = old.unwrap_or_else(|| Doc::new(kind));
        // When resurrecting a deleted record, rewrite every field so it beats the tombstone.
        let changes = if resurrect {
            diff(None, value)
        } else {
            changes
        };
        let mut ops = Vec::with_capacity(changes.len());
        for change in changes {
            doc.apply(hlc, &change);
            ops.push(Op {
                id: Uuid::now_v7(),
                hlc,
                record: record.to_owned(),
                kind: kind.to_owned(),
                change,
            });
        }
        self.persist(&ops, record, &doc, local, true)?;
        Ok(true)
    }

    fn delete_record(&mut self, kind: &str, record: &str) -> Result<bool, VaultError> {
        let Some(mut doc) = self.read_doc(record)? else {
            return Ok(false);
        };
        if doc.is_deleted() {
            return Ok(false);
        }
        let hlc = self.clock.tick();
        doc.apply(hlc, &Change::Delete);
        let op = Op {
            id: Uuid::now_v7(),
            hlc,
            record: record.to_owned(),
            kind: kind.to_owned(),
            change: Change::Delete,
        };
        let cache = !record.starts_with("secret:");
        self.persist(&[op], record, &doc, false, cache)?;
        Ok(true)
    }

    fn persist(
        &mut self,
        ops: &[Op],
        record: &str,
        doc: &Doc,
        local: bool,
        cache: bool,
    ) -> Result<(), VaultError> {
        let vault_id = self.header.vault_id;
        let key = self.unlocked()?.key.clone();
        let tx = self.conn.unchecked_transaction()?;
        for op in ops {
            let blob = key.seal(op_aad(vault_id, op.id).as_bytes(), &serde_json::to_vec(op)?)?;
            tx.execute(
                "INSERT INTO ops (id, local, remote, blob) VALUES (?1, ?2, 0, ?3)",
                params![op.id.to_string(), i64::from(local), blob],
            )?;
        }
        let blob = key.seal(
            doc_aad(vault_id, record).as_bytes(),
            &serde_json::to_vec(doc)?,
        )?;
        tx.execute(
            "INSERT INTO docs (record, blob) VALUES (?1, ?2)
             ON CONFLICT(record) DO UPDATE SET blob = excluded.blob",
            params![record, blob],
        )?;
        tx.commit()?;
        if cache && let Some(state) = self.state.as_mut() {
            state.docs.insert(record.to_owned(), doc.clone());
        }
        Ok(())
    }
}

fn read_doc_raw(
    conn: &Connection,
    key: &SymmetricKey,
    vault_id: Uuid,
    record: &str,
) -> Result<Option<Doc>, VaultError> {
    let blob: Option<Vec<u8>> = conn
        .query_row("SELECT blob FROM docs WHERE record = ?1", [record], |r| {
            r.get(0)
        })
        .optional()?;
    match blob {
        None => Ok(None),
        Some(b) => {
            let plain = key.open(doc_aad(vault_id, record).as_bytes(), &b)?;
            Ok(Some(serde_json::from_slice(&plain)?))
        }
    }
}

fn doc_aad(vault_id: Uuid, record: &str) -> String {
    format!("vaultport/doc/v1|{vault_id}|{record}")
}

fn op_aad(vault_id: Uuid, op: Uuid) -> String {
    format!("vaultport/op/v1|{vault_id}|{op}")
}

fn wrap_aad(method: &str, vault_id: Uuid) -> String {
    format!("vaultport/vk-wrap/v1|{method}|{vault_id}")
}

fn wrap_password(
    vk: &SymmetricKey,
    password: &[u8],
    kdf: KdfParams,
    vault_id: Uuid,
) -> Result<Wrapped, VaultError> {
    if password.is_empty() {
        return Err(VaultError::Invalid("password must not be empty".into()));
    }
    let kek = kdf.derive(password)?;
    let blob = kek.seal(wrap_aad("password", vault_id).as_bytes(), vk.as_bytes())?;
    Ok(Wrapped {
        kdf: Some(kdf),
        blob: B64.encode(blob),
    })
}

fn wrap_device(
    vk: &SymmetricKey,
    dk: &SymmetricKey,
    vault_id: Uuid,
) -> Result<Wrapped, VaultError> {
    let blob = dk.seal(wrap_aad("device", vault_id).as_bytes(), vk.as_bytes())?;
    Ok(Wrapped {
        kdf: None,
        blob: B64.encode(blob),
    })
}

fn unwrap(
    kek: &SymmetricKey,
    w: &Wrapped,
    method: &str,
    vault_id: Uuid,
) -> Result<SymmetricKey, VaultError> {
    let blob = B64
        .decode(&w.blob)
        .map_err(|_| VaultError::Corrupt("wrapped key encoding".into()))?;
    let vk = kek.open(wrap_aad(method, vault_id).as_bytes(), &blob)?;
    SymmetricKey::from_slice(&vk)
}

/// Unwraps a vault key exported with [`Vault::export_wrapped_key`] (used during pairing).
pub fn unwrap_exported_key(
    transfer_key: &SymmetricKey,
    blob: &str,
    vault_id: Uuid,
) -> Result<SymmetricKey, VaultError> {
    unwrap(
        transfer_key,
        &Wrapped {
            kdf: None,
            blob: blob.to_owned(),
        },
        "device",
        vault_id,
    )
}
