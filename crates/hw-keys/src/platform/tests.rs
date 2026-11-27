// SPDX-License-Identifier: GPL-3.0-or-later
//! Platform signer tests using a software P-256 key behind the [`BridgeBackend`], which is
//! exactly the path an Android Keystore plugin takes.

use std::collections::HashMap;
use std::sync::Mutex;

use p256::ecdsa::SigningKey;
use p256::ecdsa::signature::{Signer as _, Verifier as _};
use p256::pkcs8::EncodePublicKey as _;

use super::*;

/// A fake key store holding software keys.
#[derive(Default)]
struct SoftStore {
    keys: Mutex<HashMap<String, SigningKey>>,
}

impl SoftStore {
    fn insert(&self, alias: &str, seed: u8) -> Vec<u8> {
        let scalar: [u8; 32] = core::array::from_fn(|i| seed ^ (i as u8 + 1));
        let key = SigningKey::from_bytes(&scalar.into()).unwrap();
        let spki = key
            .verifying_key()
            .to_public_key_der()
            .unwrap()
            .as_bytes()
            .to_vec();
        self.keys.lock().unwrap().insert(alias.to_owned(), key);
        spki
    }

    fn sign(&self, alias: &str, data: &[u8]) -> Result<p256::ecdsa::Signature> {
        let keys = self.keys.lock().unwrap();
        let key = keys.get(alias).ok_or(Error::KeyNotFound)?;
        Ok(key.sign(data))
    }
}

fn bridge(store: &Arc<SoftStore>, encoding: BridgeSignatureEncoding) -> BridgeBackend {
    let sign_store = Arc::clone(store);
    let create_store = Arc::clone(store);
    let delete_store = Arc::clone(store);
    BridgeBackend::new("Test Keystore", move |alias, data| {
        let store = Arc::clone(&sign_store);
        async move {
            let sig = store.sign(&alias, &data)?;
            Ok(match encoding {
                BridgeSignatureEncoding::Der => sig.to_der().as_bytes().to_vec(),
                BridgeSignatureEncoding::P1363 => sig.to_bytes().to_vec(),
            })
        }
    })
    .with_signature_encoding(encoding)
    .with_create(move |label, policy| {
        let store = Arc::clone(&create_store);
        async move {
            assert_eq!(policy, AccessPolicy::BiometryCurrentSet);
            let alias = format!("alias-{label}");
            let public_key = store.insert(&alias, 7);
            Ok(BridgeKey {
                platform_ref: alias,
                public_key,
            })
        }
    })
    .with_delete(move |alias| {
        let store = Arc::clone(&delete_store);
        async move {
            store
                .keys
                .lock()
                .unwrap()
                .remove(&alias)
                .map(|_| ())
                .ok_or(Error::KeyNotFound)
        }
    })
}

#[tokio::test]
async fn bridge_signer_produces_verifiable_ecdsa_signatures() {
    for encoding in [BridgeSignatureEncoding::Der, BridgeSignatureEncoding::P1363] {
        let store = Arc::new(SoftStore::default());
        let backend: Arc<dyn PlatformKeyBackend> = Arc::new(bridge(&store, encoding));
        let signer = PlatformSigner::create(backend.clone(), "laptop", AccessPolicy::default())
            .await
            .unwrap();
        assert_eq!(signer.platform_ref(), "alias-laptop");
        assert_eq!(signer.key_algorithm(), core_model::KeyAlgorithm::EcdsaP256);
        assert_eq!(
            signer.key_storage(),
            core_model::KeyStorage::Hardware {
                platform_ref: "alias-laptop".into(),
                device: "Test Keystore".into(),
            }
        );
        assert_eq!(signer.signature_algorithm(), "ecdsa-sha2-nistp256");
        assert!(
            signer
                .public_key_openssh()
                .starts_with("ecdsa-sha2-nistp256 AAAA")
        );
        assert!(signer.fingerprint().starts_with("SHA256:"));

        let data = b"session-id || userauth request";
        let blob = signer.sign(data).await.unwrap();
        let sig = ssh_key::Signature::try_from(blob.as_slice()).unwrap();
        signer.public_key().key_data().verify(data, &sig).unwrap();
        assert!(signer.public_key().key_data().verify(b"x", &sig).is_err());
        assert_eq!(
            signer.sign_signature(data).await.unwrap().algorithm(),
            sig.algorithm()
        );

        backend.delete("alias-laptop").await.unwrap();
        assert!(matches!(signer.sign(data).await, Err(Error::KeyNotFound)));
        assert!(matches!(
            backend.delete("alias-laptop").await,
            Err(Error::KeyNotFound)
        ));
    }
}

#[tokio::test]
async fn signature_from_another_key_is_rejected() {
    let store = Arc::new(SoftStore::default());
    let backend: Arc<dyn PlatformKeyBackend> =
        Arc::new(bridge(&store, BridgeSignatureEncoding::Der));
    let a = PlatformSigner::create(backend.clone(), "a", AccessPolicy::default())
        .await
        .unwrap();
    // Replace the stored key behind "alias-a" with a different one.
    store.insert("alias-a", 99);
    assert!(matches!(
        a.sign(b"data").await,
        Err(Error::InvalidSignature)
    ));
}

#[tokio::test]
async fn malformed_bridge_output_is_rejected() {
    let backend: Arc<dyn PlatformKeyBackend> = Arc::new(
        BridgeBackend::new("Broken", |_, _| async { Ok(vec![0x30, 0x00]) }).with_create(
            |_, _| async {
                Ok(BridgeKey {
                    platform_ref: "x".into(),
                    public_key: vec![1, 2, 3],
                })
            },
        ),
    );
    assert!(matches!(
        backend.create("x", AccessPolicy::default()).await,
        Err(Error::Malformed { .. })
    ));

    let key = SigningKey::from_bytes(&[3u8; 32].into()).unwrap();
    let spki = key.verifying_key().to_public_key_der().unwrap();
    let public = crate::pubkey::ecdsa_p256_public_key(spki.as_bytes()).unwrap();
    let signer = PlatformSigner::new(backend, "x", public).unwrap();
    assert!(matches!(
        signer.sign(b"data").await,
        Err(Error::Malformed { .. })
    ));
}

#[tokio::test]
async fn rsa_signature_type_must_match_key() {
    #[derive(Debug)]
    struct RsaOnly;
    #[async_trait::async_trait]
    impl PlatformKeyBackend for RsaOnly {
        fn name(&self) -> &'static str {
            "rsa"
        }
        async fn is_available(&self) -> bool {
            true
        }
        async fn create(&self, _: &str, _: AccessPolicy) -> Result<PlatformKeyInfo> {
            Err(Error::Unsupported("test"))
        }
        async fn sign(&self, _: &str, _: &[u8]) -> Result<PlatformSignature> {
            Ok(PlatformSignature::RsaPkcs1Sha256(vec![0x11; 256]))
        }
        async fn delete(&self, _: &str) -> Result<()> {
            Ok(())
        }
    }

    // An ECDSA key with an RSA-producing backend is a type mismatch.
    let key = SigningKey::from_bytes(&[3u8; 32].into()).unwrap();
    let spki = key.verifying_key().to_public_key_der().unwrap();
    let ecdsa = crate::pubkey::ecdsa_p256_public_key(spki.as_bytes()).unwrap();
    let signer = PlatformSigner::new(Arc::new(RsaOnly), "r", ecdsa).unwrap();
    assert!(signer.sign(b"d").await.is_err());

    // An RSA key gets an rsa-sha2-256 blob padded to the modulus size.
    let mut blob = Vec::new();
    for v in [0x3141_5352u32, 2048, 3, 256, 0, 0] {
        blob.extend_from_slice(&v.to_le_bytes());
    }
    blob.extend_from_slice(&[1, 0, 1]);
    let mut modulus = vec![0xd1; 256];
    modulus[255] |= 1;
    blob.extend_from_slice(&modulus);
    let rsa = crate::pubkey::rsa_public_key_from_bcrypt_blob(&blob).unwrap();
    let signer = PlatformSigner::new(Arc::new(RsaOnly), "r", rsa).unwrap();
    assert_eq!(signer.signature_algorithm(), "rsa-sha2-256");
    let sig = signer.sign_signature(b"d").await.unwrap();
    assert_eq!(sig.algorithm().as_str(), "rsa-sha2-256");
    assert_eq!(sig.as_bytes(), &[0x11; 256][..]);
}

#[tokio::test]
async fn unsupported_backend_reports_unsupported() {
    let backend = UnsupportedBackend::new("nope");
    assert!(!backend.is_available().await);
    assert!(matches!(
        backend.create("x", AccessPolicy::UserPresence).await,
        Err(Error::Unsupported("nope"))
    ));
    assert!(matches!(
        backend.sign("x", b"").await,
        Err(Error::Unsupported(_))
    ));
    assert!(matches!(
        backend.delete("x").await,
        Err(Error::Unsupported(_))
    ));
    assert!(matches!(
        backend.public_key("x").await,
        Err(Error::Unsupported(_))
    ));

    #[cfg(target_os = "linux")]
    assert!(!default_backend().is_available().await);
}

#[test]
fn rejects_unsupported_key_types() {
    let ed = ssh_key::PublicKey::from_openssh(
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILM+rvN+ot98qgEN796jTiQfZfG1KaT0PtFDJ/XFSqti",
    )
    .unwrap();
    assert!(PlatformSigner::new(Arc::new(UnsupportedBackend::new("x")), "r", ed).is_err());
}

#[test]
fn label_sanitizing() {
    assert_eq!(sanitize_label("work laptop"), "work laptop");
    assert_eq!(sanitize_label("a/b\\c\0d"), "abcd");
    assert_eq!(sanitize_label("  "), "ssh key");
    assert_eq!(sanitize_label(&"x".repeat(100)).len(), 64);
}

#[test]
fn debug_output_is_redacted() {
    let sig = PlatformSignature::EcdsaDer(vec![0xab; 70]);
    assert_eq!(format!("{sig:?}"), "EcdsaDer(<70 bytes>)");
}
