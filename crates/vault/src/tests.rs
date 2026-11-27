// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used)]

use crate::*;
use core_model::{Group, Host, KeyAlgorithm, KeyId, KeyMeta, KeyStorage};
use rusqlite::Connection;

fn opts(pw: &[u8]) -> CreateOptions<'_> {
    CreateOptions {
        password: Some(pw),
        device_key: None,
        kdf: KdfParams::insecure_for_tests(),
    }
}

#[test]
fn create_unlock_lock_cycle() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v.db");
    let host = Host::new("web", "10.0.0.1");
    {
        let mut v = Vault::create(&path, &opts(b"hunter2")).unwrap();
        assert!(v.put(&host).unwrap());
        assert!(!v.put(&host).unwrap(), "no-op writes are skipped");
    }
    let mut v = Vault::open(&path).unwrap();
    assert!(v.is_locked());
    assert!(matches!(v.list::<Host>(), Err(VaultError::Locked)));
    assert!(matches!(
        v.unlock_with_password(b"wrong"),
        Err(VaultError::Authentication)
    ));
    v.unlock_with_password(b"hunter2").unwrap();
    assert_eq!(v.list::<Host>().unwrap(), vec![host.clone()]);
    v.lock();
    assert!(v.get::<Host>(&host.id.to_string()).is_err());
}

#[test]
fn validation_is_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let mut v = Vault::create(dir.path().join("v.db"), &opts(b"pw")).unwrap();
    let bad = Host::new("", "not a host");
    assert!(matches!(v.put(&bad), Err(VaultError::Validation(_))));
}

#[test]
fn device_key_and_password_management() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v.db");
    let dk = SymmetricKey::generate().unwrap();
    {
        let mut v = Vault::create(&path, &opts(b"pw")).unwrap();
        v.put(&Group::new("prod")).unwrap();
        v.set_device_key(&dk).unwrap();
        v.remove_password().unwrap();
        assert!(matches!(
            v.remove_device_key(),
            Err(VaultError::LastUnlockMethod)
        ));
    }
    let mut v = Vault::open(&path).unwrap();
    assert_eq!(
        v.unlock_methods(),
        UnlockMethods {
            password: false,
            device_key: true
        }
    );
    assert!(matches!(
        v.unlock_with_password(b"pw"),
        Err(VaultError::NoSuchUnlockMethod)
    ));
    let other = SymmetricKey::generate().unwrap();
    assert!(v.unlock_with_device_key(&other).is_err());
    v.unlock_with_device_key(&dk).unwrap();
    assert_eq!(v.list::<Group>().unwrap().len(), 1);
    v.set_password(b"new", KdfParams::insecure_for_tests())
        .unwrap();
    v.lock();
    v.unlock_with_password(b"new").unwrap();
}

#[test]
fn secrets_are_stored_encrypted_and_not_cached() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v.db");
    let mut v = Vault::create(&path, &opts(b"pw")).unwrap();
    let id = v.put_secret(b"super-secret-password", false).unwrap();
    assert_eq!(&*v.get_secret(id).unwrap(), b"super-secret-password");
    v.update_secret(id, b"rotated", false).unwrap();
    assert_eq!(&*v.get_secret(id).unwrap(), b"rotated");

    // The plaintext never touches the database file.
    let raw = std::fs::read(&path).unwrap();
    let wal = std::fs::read(path.with_extension("db-wal")).unwrap_or_default();
    for hay in [&raw, &wal] {
        assert!(!hay.windows(7).any(|w| w == b"rotated"));
        assert!(!hay.windows(5).any(|w| w == b"super"));
    }

    assert!(v.delete_secret(id).unwrap());
    assert!(v.get_secret(id).is_err());
}

#[test]
fn tampered_records_fail_authentication() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v.db");
    {
        let mut v = Vault::create(&path, &opts(b"pw")).unwrap();
        v.put(&Host::new("a", "a.example")).unwrap();
        v.put(&Host::new("b", "b.example")).unwrap();
    }
    {
        // Swap the ciphertexts of two records: AAD binding must reject it.
        let c = Connection::open(&path).unwrap();
        let rows: Vec<(String, Vec<u8>)> = c
            .prepare("SELECT record, blob FROM docs")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        c.execute(
            "UPDATE docs SET blob = ?1 WHERE record = ?2",
            rusqlite::params![rows[1].1, rows[0].0],
        )
        .unwrap();
    }
    let mut v = Vault::open(&path).unwrap();
    assert!(matches!(
        v.unlock_with_password(b"pw"),
        Err(VaultError::Authentication)
    ));
}

#[test]
fn delete_and_resurrect() {
    let dir = tempfile::tempdir().unwrap();
    let mut v = Vault::create(dir.path().join("v.db"), &opts(b"pw")).unwrap();
    let h = Host::new("a", "a.example");
    v.put(&h).unwrap();
    assert!(v.delete::<Host>(&h.id.to_string()).unwrap());
    assert!(!v.delete::<Host>(&h.id.to_string()).unwrap());
    assert!(v.list::<Host>().unwrap().is_empty());
    v.put(&h).unwrap();
    assert_eq!(v.list::<Host>().unwrap(), vec![h]);
}

#[test]
fn two_devices_converge() {
    let dir = tempfile::tempdir().unwrap();
    let pa = dir.path().join("a.db");
    let pb = dir.path().join("b.db");
    let mut a = Vault::create(&pa, &opts(b"pw")).unwrap();

    let mut host = Host::new("web", "10.0.0.1");
    a.put(&host).unwrap();

    // Pair device B: transfer the vault key, then replay A's ops.
    let transfer = SymmetricKey::generate().unwrap();
    let wrapped = a.export_wrapped_key(&transfer).unwrap();
    let vk = unwrap_exported_key(&transfer, &wrapped, a.vault_id()).unwrap();
    let mut b = Vault::create_replica(&pb, a.vault_id(), &vk, &opts(b"pw-b")).unwrap();
    let (ops, cursor_a) = a.local_ops_since(0).unwrap();
    b.apply_remote(&ops).unwrap();
    assert_eq!(b.list::<Host>().unwrap(), vec![host.clone()]);

    // Concurrent edits to different fields on both devices.
    host.config.ssh.port = Some(2222);
    a.put(&host).unwrap();
    let mut host_b = b.get::<Host>(&host.id.to_string()).unwrap().unwrap();
    host_b.tags = vec!["prod".into()];
    b.put(&host_b).unwrap();

    let (from_a, _) = a.local_ops_since(cursor_a).unwrap();
    let (from_b, _) = b.local_ops_since(0).unwrap();
    let events = b.apply_remote(&from_a).unwrap();
    assert_eq!(events.len(), 1);
    a.apply_remote(&from_b).unwrap();
    // Re-applying is idempotent.
    assert!(a.apply_remote(&from_b).unwrap().is_empty());

    let ha = a.list::<Host>().unwrap();
    let hb = b.list::<Host>().unwrap();
    assert_eq!(ha, hb);
    assert_eq!(ha[0].config.ssh.port, Some(2222));
    assert_eq!(ha[0].tags, vec!["prod".to_owned()]);
}

#[test]
fn device_local_records_are_not_replicated() {
    let dir = tempfile::tempdir().unwrap();
    let mut v = Vault::create(dir.path().join("v.db"), &opts(b"pw")).unwrap();
    let key = KeyMeta {
        id: KeyId::new(),
        label: "Secure Enclave".into(),
        algorithm: KeyAlgorithm::EcdsaP256,
        bits: None,
        public_key: "ecdsa-sha2-nistp256 AAAA".into(),
        fingerprint: "SHA256:x".into(),
        storage: KeyStorage::Hardware {
            platform_ref: "se:1".into(),
            device: "mac".into(),
        },
        encrypted: false,
        passphrase: None,
        certificate: None,
        created_at: 0,
        updated_at: 0,
    };
    v.put(&key).unwrap();
    v.put_secret(b"local", true).unwrap();
    let (ops, _) = v.local_ops_since(0).unwrap();
    assert!(ops.is_empty());
    assert!(v.all_syncable_ops().unwrap().is_empty());
}

#[test]
fn hostile_ops_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = Vault::create(dir.path().join("a.db"), &opts(b"pw")).unwrap();
    let mut b = Vault::create(dir.path().join("b.db"), &opts(b"pw")).unwrap();
    a.put(&Host::new("x", "x.example")).unwrap();
    let (mut ops, _) = a.local_ops_since(0).unwrap();
    // Ops from another vault (different key) must fail.
    assert!(matches!(
        b.apply_remote(&ops),
        Err(VaultError::Authentication)
    ));
    // Bit flips must fail.
    let n = ops[0].blob.len();
    ops[0].blob[n - 1] ^= 0x80;
    assert!(
        a.apply_remote(&ops).is_ok(),
        "already-known op ids are skipped"
    );
    ops[0].id = uuid::Uuid::now_v7();
    assert!(a.apply_remote(&ops).is_err());
}

#[test]
fn settings_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let mut v = Vault::create(dir.path().join("v.db"), &opts(b"pw")).unwrap();
    let s = serde_json::json!({"theme": "dark", "terminal": {"fontSize": 14}});
    v.put_settings("app", &s, false).unwrap();
    assert_eq!(v.get_settings("app").unwrap(), Some(s));
    assert!(v.put_settings("app", &serde_json::json!(1), false).is_err());
}

#[test]
fn subkeys_are_deterministic_and_distinct() {
    let dir = tempfile::tempdir().unwrap();
    let mut v = Vault::create(dir.path().join("v.db"), &opts(b"pw")).unwrap();
    let a = v.derive_subkey("session-log").unwrap();
    let b = v.derive_subkey("session-log").unwrap();
    let c = v.derive_subkey("other").unwrap();
    assert_eq!(a.as_bytes(), b.as_bytes());
    assert_ne!(a.as_bytes(), c.as_bytes());
    assert!(v.derive_subkey("").is_err());
    v.lock();
    assert!(matches!(v.derive_subkey("x"), Err(VaultError::Locked)));
}
