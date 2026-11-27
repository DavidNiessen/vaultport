// SPDX-License-Identifier: GPL-3.0-or-later
//! Encrypted export/import and device pairing.

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used)]

mod common;

use common::{add_host, labels, new_vault, opts};
use sync::backends::FolderBackend;
use sync::pairing::{PAIRING_TTL_MS, PairingKdf, PairingPayload, accept_offer_at, create_offer_at};
use sync::{
    SyncEngine, SyncError, SyncState, export_file, export_file_with, import_file, inspect_file,
};
use vault::{KdfParams, Vault};

#[test]
fn export_import_roundtrip_and_failures() {
    let tmp = tempfile::tempdir().unwrap();
    let mut a = new_vault(tmp.path(), "a.db");
    add_host(&mut a, "web");
    add_host(&mut a, "db");
    let secret = a.put_secret(b"hunter2", false).unwrap();
    a.put_secret(b"device-only", true).unwrap();

    let file = export_file_with(&a, b"correct horse", KdfParams::insecure_for_tests()).unwrap();
    assert_eq!(&file[..8], b"VAULTPRT");
    assert!(!file.windows(7).any(|w| w == b"hunter2"));
    let info = inspect_file(&file).unwrap();
    assert_eq!(info.vault_id, a.vault_id());
    assert_eq!(info.op_count, a.all_syncable_ops().unwrap().len());

    // Wrong passphrase: nothing is created.
    let dest = tmp.path().join("imported.db");
    assert!(matches!(
        import_file(&file, b"wrong", &dest, &opts()),
        Err(SyncError::Authentication)
    ));
    assert!(!dest.exists());

    // Tampering with the header or the payload is detected.
    let header_end = 14 + u32::from_be_bytes(file[10..14].try_into().unwrap()) as usize;
    let mut t = file.clone();
    let pos = t[14..header_end]
        .windows(9)
        .position(|w| w == b"createdAt")
        .unwrap()
        + 14
        + 12;
    t[pos] = if t[pos] == b'1' { b'2' } else { b'1' };
    assert!(matches!(
        import_file(&t, b"correct horse", &dest, &opts()),
        Err(SyncError::Authentication)
    ));
    let mut t = file.clone();
    let last = t.len() - 1;
    t[last] ^= 1;
    assert!(matches!(
        import_file(&t, b"correct horse", &dest, &opts()),
        Err(SyncError::Authentication)
    ));
    assert!(matches!(
        import_file(b"VAULTPRT\0\x09", b"x", &dest, &opts()),
        Err(SyncError::Format(_))
    ));
    assert!(!dest.exists());

    let b = import_file(&file, b"correct horse", &dest, &opts()).unwrap();
    assert_eq!(b.vault_id(), a.vault_id());
    assert_ne!(b.device_id(), a.device_id());
    assert_eq!(labels(&b), labels(&a));
    assert_eq!(&*b.get_secret(secret).unwrap(), b"hunter2");
    drop(b);
    // The import is a normal vault protected by its own local unlock method.
    let mut reopened = Vault::open(&dest).unwrap();
    reopened.unlock_with_password(b"pw").unwrap();
    assert_eq!(labels(&reopened), vec!["db", "web"]);

    assert!(export_file_with(&a, b"", KdfParams::insecure_for_tests()).is_err());
}

#[test]
fn export_with_default_kdf() {
    let tmp = tempfile::tempdir().unwrap();
    let mut a = new_vault(tmp.path(), "a.db");
    add_host(&mut a, "web");
    let file = export_file(&a, b"pass").unwrap();
    let b = import_file(&file, b"pass", &tmp.path().join("b.db"), &opts()).unwrap();
    assert_eq!(labels(&b), vec!["web"]);
}

fn cheap() -> PairingKdf {
    PairingKdf {
        memory_kib: 64,
        iterations: 1,
        parallelism: 1,
    }
}

#[tokio::test]
async fn pairing_roundtrip_then_sync() {
    let tmp = tempfile::tempdir().unwrap();
    let storage = FolderBackend::new(tmp.path().join("storage"));
    let mut a = new_vault(tmp.path(), "a.db");
    add_host(&mut a, "web");
    let mut sa = SyncState::new(a.vault_id());
    SyncEngine::default()
        .sync(&mut a, &storage, &mut sa)
        .await
        .unwrap();

    let now = 1_800_000_000_000;
    let backend = serde_json::json!({"type": "folder", "path": "/Users/me/iCloud/vaultport"});
    let offer =
        create_offer_at(&a, backend.clone(), Some(b"webdav-password"), cheap(), now).unwrap();
    assert_eq!(offer.code().len(), 9);
    assert!(!format!("{offer:?}").contains(offer.code()));
    let qr = offer.payload.to_qr_string().unwrap();
    assert!(qr.starts_with("vaultport-pair:1:"));

    // Device B scans the QR code and types the code (any case, without the dash).
    let scanned = PairingPayload::from_qr_string(&qr).unwrap();
    let typed = offer.code().replace('-', " ").to_lowercase();
    let accepted = accept_offer_at(&scanned, &typed, now + 60_000).unwrap();
    assert_eq!(accepted.vault_id, a.vault_id());
    assert_eq!(accepted.backend, backend);
    assert_eq!(
        accepted.backend_secret.as_deref().map(Vec::as_slice),
        Some(&b"webdav-password"[..])
    );

    let mut b = Vault::create_replica(
        tmp.path().join("b.db"),
        accepted.vault_id,
        &accepted.vault_key,
        &opts(),
    )
    .unwrap();
    let mut sb = SyncState::new(b.vault_id());
    let r = SyncEngine::default()
        .sync(&mut b, &storage, &mut sb)
        .await
        .unwrap();
    assert!(r.is_clean());
    assert_eq!(labels(&b), vec!["web"]);
}

#[test]
fn pairing_rejects_wrong_codes_expiry_and_tampering() {
    let tmp = tempfile::tempdir().unwrap();
    let a = new_vault(tmp.path(), "a.db");
    let now = 1_800_000_000_000;
    let offer = create_offer_at(&a, serde_json::json!({"type": "s3"}), None, cheap(), now).unwrap();
    let p = &offer.payload;

    let wrong = if offer.code().starts_with('A') {
        "BBBB-BBBB"
    } else {
        "AAAA-AAAA"
    };
    assert!(matches!(
        accept_offer_at(p, wrong, now),
        Err(SyncError::Authentication)
    ));
    assert!(matches!(
        accept_offer_at(p, "short", now),
        Err(SyncError::Authentication)
    ));
    assert!(matches!(
        accept_offer_at(p, offer.code(), now + PAIRING_TTL_MS + 1),
        Err(SyncError::Expired)
    ));

    // Extending the expiry or redirecting the backend invalidates the code.
    let mut extended = p.clone();
    extended.expires_at = now + PAIRING_TTL_MS + 1;
    assert!(matches!(
        accept_offer_at(&extended, offer.code(), now + PAIRING_TTL_MS),
        Err(SyncError::Authentication)
    ));
    let mut redirected = p.clone();
    redirected.backend = serde_json::json!({"type": "s3", "endpoint": "https://evil.example"});
    assert!(matches!(
        accept_offer_at(&redirected, offer.code(), now),
        Err(SyncError::Authentication)
    ));
    assert!(accept_offer_at(p, offer.code(), now).is_ok());

    assert!(PairingPayload::from_qr_string("https://example.com").is_err());
    assert!(PairingPayload::from_qr_string("vaultport-pair:1:!!!").is_err());
}
