// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests for the security key formats.
//!
//! Two kinds of vectors are used:
//! * signatures produced by real authenticators through OpenSSH (taken from the `ssh-key`
//!   crate's test suite), which pin our verifier to OpenSSH's behaviour, and
//! * signatures constructed here with fixed software keys that emulate an authenticator
//!   byte-for-byte per PROTOCOL.u2f, which pin the encoder.

use super::*;
use p256::ecdsa::signature::{Signer as _, Verifier as _};
use sha2::Sha512;

fn hex(s: &str) -> Vec<u8> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn b64_blob(line: &str) -> Vec<u8> {
    BASE64
        .decode(line.split_whitespace().nth(1).unwrap())
        .unwrap()
}

/// `id_sk_ecdsa_p256_2.pub` (ssh-key test suite, created by OpenSSH with a real key).
const SK_ECDSA_PUB: &str = "sk-ecdsa-sha2-nistp256@openssh.com AAAAInNrLWVjZHNhLXNoYTItbmlzdHAyNTZAb3BlbnNzaC5jb20AAAAIbmlzdHAyNTYAAABBBNdo6XfhTK080uz5UbGyOcNMo+R3nPXMBxurwH2M1bDtQYbDT6qBE7EdQGkcy/EJDXbzT0KlU9rROjcX+JsgtGAAAAAEc3NoOg==";
/// `id_sk_ed25519_2.pub`.
const SK_ED25519_PUB: &str = "sk-ssh-ed25519@openssh.com AAAAGnNrLXNzaC1lZDI1NTE5QG9wZW5zc2guY29tAAAAINSoElFleH+nN83FoLqqepJjN+y7Gs5lrn7qXjBqQZyuAAAABHNzaDo=";

/// ssh-agent signature of `b"testing"` by the key in [`SK_ECDSA_PUB`].
const SK_ECDSA_AGENT_SIGNATURE: &str = "
    00000022736b2d65636473612d736861322d6e69737470323536406f70656e73
    73682e636f6d00000049000000201b35a1c6469a43a3d09d490d6ff8ca1bc248
    6a2edeb8aa7d119e4c70b9c1811000000021009724a2a4449a90357485ed1df0
    161274d20083342b02756794bc3f068fcdc15e01000000ec";

/// Raw signature + flags + counter from an `ssh-keygen -Y sign -n example` SSHSIG of
/// `b"testing"` by the key in [`SK_ED25519_PUB`] (hash algorithm sha512).
const SK_ED25519_SSHSIG_SIGNATURE: &str = "
    2f5670b6f93465d17423878a74084bf331767031ed240c627c8eb79ab8fa1b93
    5a1fd993f52f5a13fec1797f8a434f943a6096246aea8dd5c8aa922cba3d9506";

fn key_from_pub(line: &str) -> SkKey {
    // Decode the public blob with our own reader and rebuild the key with a dummy handle.
    let blob = b64_blob(line);
    let mut r = Reader::new(&blob, "test");
    let alg = SkAlgorithm::from_ssh_name(r.string().unwrap()).unwrap();
    let public = match alg {
        SkAlgorithm::Ed25519 => r.string().unwrap(),
        SkAlgorithm::EcdsaP256 => {
            r.expect_string(b"nistp256", "curve").unwrap();
            r.string().unwrap()
        }
    };
    let app = r.utf8().unwrap().to_owned();
    r.finish().unwrap();
    let key = SkKey::new(
        alg,
        public,
        app,
        key_flags::USER_PRESENCE_REQUIRED,
        vec![1; 64],
    )
    .unwrap();
    assert_eq!(
        key.public_key_blob(),
        blob,
        "public key re-encoding must be identical"
    );
    key
}

#[test]
fn public_keys_match_openssh_files() {
    let ecdsa = key_from_pub(SK_ECDSA_PUB);
    assert_eq!(ecdsa.algorithm(), SkAlgorithm::EcdsaP256);
    assert_eq!(ecdsa.application(), "ssh:");
    assert_eq!(ecdsa.public_key_openssh(""), SK_ECDSA_PUB);
    assert_eq!(
        ecdsa.public_key_openssh("user@example.com"),
        format!("{SK_ECDSA_PUB} user@example.com")
    );

    let ed = key_from_pub(SK_ED25519_PUB);
    assert_eq!(ed.algorithm(), SkAlgorithm::Ed25519);
    assert_eq!(ed.public_key_openssh(""), SK_ED25519_PUB);

    // Fingerprint is SHA256 over the public key blob, unpadded base64.
    let expected = format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD
            .encode(Sha256::digest(b64_blob(SK_ED25519_PUB)))
    );
    assert_eq!(ed.fingerprint(), expected);
}

#[test]
fn verifies_real_openssh_sk_ecdsa_signature() {
    let key = key_from_pub(SK_ECDSA_PUB);
    let blob = hex(SK_ECDSA_AGENT_SIGNATURE);
    let sig = key.verify(b"testing", &blob).unwrap();
    assert_eq!(sig.flags(), 0x01);
    assert_eq!(sig.counter(), 0xec);
    assert!(sig.satisfies_key_policy(&key));
    assert_eq!(sig.encode(), blob, "re-encoding must be byte-identical");
    assert!(matches!(
        key.verify(b"Bogus!", &blob),
        Err(Error::InvalidSignature)
    ));
    // Tampering with the counter or flags breaks the signature.
    let mut tampered = blob.clone();
    *tampered.last_mut().unwrap() ^= 1;
    assert!(key.verify(b"testing", &tampered).is_err());
    let mut tampered = blob.clone();
    let flags_at = tampered.len() - 5;
    tampered[flags_at] = 0x05;
    assert!(key.verify(b"testing", &tampered).is_err());
}

#[test]
fn verifies_real_openssh_sk_ed25519_sshsig() {
    let key = key_from_pub(SK_ED25519_PUB);
    let mut w = Writer::new();
    w.string(SK_ED25519_ALGORITHM.as_bytes())
        .string(&hex(SK_ED25519_SSHSIG_SIGNATURE))
        .u8(0x01)
        .u32(9);
    let blob = w.into_vec();

    // PROTOCOL.sshsig: the signed message is the SSHSIG preamble structure.
    let mut msg = Writer::new();
    msg.raw(b"SSHSIG")
        .string(b"example")
        .string(b"")
        .string(b"sha512")
        .string(&Sha512::digest(b"testing"));
    let message = msg.into_vec();

    let sig = key.verify(&message, &blob).unwrap();
    assert_eq!((sig.flags(), sig.counter()), (0x01, 9));
    assert!(key.verify(b"testing", &blob).is_err());
}

/// Emulates an authenticator: signs `authData || SHA256(message)` like CTAP2 does.
fn emulate_ecdsa_assertion(
    signing_key: &p256::ecdsa::SigningKey,
    application: &str,
    flags: u8,
    counter: u32,
    message: &[u8],
) -> (Vec<u8>, Vec<u8>) {
    let mut auth_data = Sha256::digest(application.as_bytes()).to_vec();
    auth_data.push(flags);
    auth_data.extend_from_slice(&counter.to_be_bytes());
    let mut to_sign = auth_data.clone();
    to_sign.extend_from_slice(&Sha256::digest(message));
    let sig: p256::ecdsa::Signature = signing_key.sign(&to_sign);
    (auth_data, sig.to_der().as_bytes().to_vec())
}

fn fixed_p256_key() -> p256::ecdsa::SigningKey {
    let scalar: [u8; 32] = core::array::from_fn(|i| (i as u8).wrapping_mul(7).wrapping_add(1));
    p256::ecdsa::SigningKey::from_bytes(&scalar.into()).unwrap()
}

#[test]
fn constructed_sk_ecdsa_signature_verifies_with_both_implementations() {
    let signing = fixed_p256_key();
    let point = signing.verifying_key().to_sec1_point(false);
    let key = SkKey::new(
        SkAlgorithm::EcdsaP256,
        point.as_bytes(),
        DEFAULT_APPLICATION,
        key_flags::USER_PRESENCE_REQUIRED | key_flags::USER_VERIFICATION_REQUIRED,
        b"credential-id".to_vec(),
    )
    .unwrap();
    let message = b"session id || SSH_MSG_USERAUTH_REQUEST ...";
    let (auth_data, der) = emulate_ecdsa_assertion(&signing, "ssh:", 0x05, 0x0102_0304, message);

    let sig = SkSignature::from_assertion(&key, &auth_data, &der).unwrap();
    let blob = sig.encode();

    // Layout: string type, string (mpint r, mpint s), byte flags, uint32 counter.
    let mut r = Reader::new(&blob, "test");
    assert_eq!(r.string().unwrap(), SK_ECDSA_P256_ALGORITHM.as_bytes());
    let inner = r.string().unwrap();
    assert_eq!(r.u8().unwrap(), 0x05);
    assert_eq!(r.u32().unwrap(), 0x0102_0304);
    r.finish().unwrap();
    let parsed = EcdsaP256Signature::from_ssh_blob(inner).unwrap();
    assert_eq!(parsed, EcdsaP256Signature::from_der(&der).unwrap());

    // Our OpenSSH-equivalent verifier.
    let verified = key.verify(message, &blob).unwrap();
    assert!(verified.satisfies_key_policy(&key));
    assert!(key.verify(b"other", &blob).is_err());

    // Independent implementation (ssh-key) agrees.
    let ssh_sig = sig.to_ssh_key_signature().unwrap();
    key.public_key()
        .key_data()
        .verify(message, &ssh_sig)
        .unwrap();
    assert!(
        key.public_key()
            .key_data()
            .verify(b"other", &ssh_sig)
            .is_err()
    );

    // A touch-only assertion does not satisfy a verify-required key.
    let (auth_data, der) = emulate_ecdsa_assertion(&signing, "ssh:", 0x01, 1, message);
    let sig = SkSignature::from_assertion(&key, &auth_data, &der).unwrap();
    assert!(key.verify(message, &sig.encode()).is_ok());
    assert!(!sig.satisfies_key_policy(&key));
}

#[test]
fn constructed_sk_ed25519_signature_verifies_with_both_implementations() {
    use ed25519_dalek::Signer as _;
    let signing = ed25519_dalek::SigningKey::from_bytes(&[0x42; 32]);
    let key = SkKey::new(
        SkAlgorithm::Ed25519,
        signing.verifying_key().as_bytes(),
        "ssh:work",
        key_flags::USER_PRESENCE_REQUIRED,
        vec![0xaa; 100],
    )
    .unwrap();
    let message = b"hello";
    let signed = signed_data("ssh:work", 0x01, 7, message);
    let sig = signing.sign(&signed);

    let mut auth_data = signed[..37].to_vec();
    let s = SkSignature::from_assertion(&key, &auth_data, &sig.to_bytes()).unwrap();
    let blob = s.encode();

    let mut expected = Writer::new();
    expected
        .string(SK_ED25519_ALGORITHM.as_bytes())
        .string(&sig.to_bytes())
        .u8(0x01)
        .u32(7);
    assert_eq!(blob, expected.into_vec());

    key.verify(message, &blob).unwrap();
    key.public_key()
        .key_data()
        .verify(message, &s.to_ssh_key_signature().unwrap())
        .unwrap();

    // The rpIdHash must match the key's application.
    auth_data[0] ^= 1;
    assert!(SkSignature::from_assertion(&key, &auth_data, &sig.to_bytes()).is_err());
}

#[test]
fn signed_data_layout() {
    let data = signed_data("ssh:", 0x05, 0xdead_beef, b"msg");
    assert_eq!(&data[..32], Sha256::digest(b"ssh:").as_slice());
    assert_eq!(data[32], 0x05);
    assert_eq!(&data[33..37], &[0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(&data[37..], Sha256::digest(b"msg").as_slice());
}

#[test]
fn fixed_signature_encoding_vector() {
    // r = 1, s = 0x80..00 (needs an mpint pad byte).
    let mut r = [0u8; 32];
    r[31] = 1;
    let mut s = [0u8; 32];
    s[0] = 0x80;
    let sig = SkSignature {
        body: SkSignatureBody::EcdsaP256(EcdsaP256Signature::from_scalars(r, s).unwrap()),
        flags: 0x01,
        counter: 2,
    };
    let expected = hex(
        "00000022 736b2d65636473612d736861322d6e69737470323536406f70656e7373682e636f6d
         0000002a
           00000001 01
           00000021 00 8000000000000000000000000000000000000000000000000000000000000000
         01
         00000002",
    );
    assert_eq!(sig.encode(), expected);
    assert_eq!(SkSignature::decode(&expected).unwrap(), sig);
}

#[test]
fn from_assertion_rejects_bad_input() {
    let signing = fixed_p256_key();
    let key = key_from_pub(SK_ECDSA_PUB);
    let (auth_data, der) = emulate_ecdsa_assertion(&signing, "ssh:", 0x01, 1, b"m");
    assert!(SkSignature::from_assertion(&key, &auth_data, &der).is_ok());

    assert!(SkSignature::from_assertion(&key, &auth_data[..36], &der).is_err());
    let mut with_ext = auth_data.clone();
    with_ext[32] |= auth_data_flags::EXTENSION_DATA;
    with_ext.extend_from_slice(&[0xa0]);
    assert!(SkSignature::from_assertion(&key, &with_ext, &der).is_err());
    let mut longer = auth_data.clone();
    longer.push(0);
    assert!(SkSignature::from_assertion(&key, &longer, &der).is_err());
    let (other_app, der2) = emulate_ecdsa_assertion(&signing, "ssh:other", 0x01, 1, b"m");
    assert!(SkSignature::from_assertion(&key, &other_app, &der2).is_err());
    assert!(SkSignature::from_assertion(&key, &auth_data, &der[..der.len() - 1]).is_err());
    assert!(SkSignature::from_assertion(&key, &auth_data, &[0; 64]).is_err());
}

#[test]
fn decode_rejects_bad_blobs() {
    let blob = hex(SK_ECDSA_AGENT_SIGNATURE);
    assert!(SkSignature::decode(&blob).is_ok());
    let mut trailing = blob.clone();
    trailing.push(0);
    assert!(SkSignature::decode(&trailing).is_err());
    assert!(SkSignature::decode(&blob[..blob.len() - 1]).is_err());
    assert!(SkSignature::decode(&[]).is_err());

    let mut w = Writer::new();
    w.string(b"ecdsa-sha2-nistp256")
        .string(&[0; 8])
        .u8(1)
        .u32(1);
    assert!(SkSignature::decode(&w.into_vec()).is_err());
    let mut w = Writer::new();
    w.string(SK_ED25519_ALGORITHM.as_bytes())
        .string(&[0; 63])
        .u8(1)
        .u32(1);
    assert!(SkSignature::decode(&w.into_vec()).is_err());

    // Signature algorithm must match the key.
    let ed = key_from_pub(SK_ED25519_PUB);
    assert!(ed.verify(b"testing", &blob).is_err());
}

#[test]
fn key_validation() {
    let ed = ed25519_dalek::SigningKey::from_bytes(&[0x42; 32])
        .verifying_key()
        .to_bytes()
        .to_vec();
    assert!(SkKey::new(SkAlgorithm::Ed25519, &ed[..31], "ssh:", 1, vec![1]).is_err());
    assert!(SkKey::new(SkAlgorithm::Ed25519, &ed, "", 1, vec![1]).is_err());
    assert!(SkKey::new(SkAlgorithm::Ed25519, &ed, "ssh:", 1, vec![]).is_err());
    assert!(SkKey::new(SkAlgorithm::Ed25519, &ed, "ssh:", 1, vec![1; 1024]).is_err());
    assert!(SkKey::new(SkAlgorithm::EcdsaP256, &[4; 65], "ssh:", 1, vec![1]).is_err());
    assert!(SkKey::new(SkAlgorithm::Ed25519, &ed, "ssh:", 1, vec![1; 1023]).is_ok());

    let key = SkKey::new(
        SkAlgorithm::Ed25519,
        &ed,
        "ssh:",
        key_flags::USER_PRESENCE_REQUIRED | key_flags::RESIDENT_KEY,
        vec![0x5a; 16],
    )
    .unwrap();
    assert!(key.requires_user_presence());
    assert!(!key.requires_user_verification());
    assert!(key.is_resident());
    let debug = format!("{key:?}");
    assert!(debug.contains("<16 bytes>"), "{debug}");
    assert!(!debug.contains("90") && !debug.contains("5a"), "{debug}");
}

#[test]
fn private_key_file_roundtrip_and_ssh_key_compat() {
    for key in [key_from_pub(SK_ECDSA_PUB), key_from_pub(SK_ED25519_PUB)] {
        let pem = key.to_openssh_private_key("me@laptop").unwrap();
        assert!(pem.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----\n"));
        assert!(pem.ends_with("-----END OPENSSH PRIVATE KEY-----\n"));
        assert!(pem.lines().all(|l| l.len() <= 70));

        let (parsed, comment) = SkKey::from_openssh_private_key(&pem).unwrap();
        assert_eq!(parsed, key);
        assert_eq!(comment, "me@laptop");

        // ssh-key (an independent implementation of openssh-key-v1) reads it too.
        let other = ssh_key::PrivateKey::from_openssh(pem.as_bytes()).unwrap();
        assert_eq!(other.public_key().key_data(), key.public_key().key_data());
        assert_eq!(other.comment().as_bytes(), b"me@laptop");
        let kp = other.key_data();
        let (flags, handle) = match key.algorithm() {
            SkAlgorithm::EcdsaP256 => {
                let sk = kp.sk_ecdsa_p256().unwrap();
                (sk.flags(), sk.key_handle().to_vec())
            }
            SkAlgorithm::Ed25519 => {
                let sk = kp.sk_ed25519().unwrap();
                (sk.flags(), sk.key_handle().to_vec())
            }
        };
        assert_eq!(flags, key.flags());
        assert_eq!(handle, key.key_handle());

        // And what ssh-key writes, we read.
        let theirs = other.to_openssh(ssh_key::LineEnding::CRLF).unwrap();
        let (back, _) = SkKey::from_openssh_private_key(&theirs).unwrap();
        assert_eq!(back, key);
    }
}

#[test]
fn private_key_file_rejections() {
    let key = key_from_pub(SK_ED25519_PUB);
    // With this comment length the private section ends in 7 padding bytes.
    let pem = key.to_openssh_private_key("cc").unwrap();
    let body: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    let raw = BASE64.decode(body).unwrap();
    let rewrap = |raw: &[u8]| format!("{PEM_BEGIN}\n{}\n{PEM_END}\n", BASE64.encode(raw));
    assert!(SkKey::from_openssh_private_key(&rewrap(&raw)).is_ok());

    assert!(SkKey::from_openssh_private_key("").is_err());
    assert!(SkKey::from_openssh_private_key(&format!("{PEM_BEGIN}\n!!!\n{PEM_END}")).is_err());

    let mut bad_magic = raw.clone();
    bad_magic[0] = b'x';
    assert!(SkKey::from_openssh_private_key(&rewrap(&bad_magic)).is_err());

    // Flip a check byte (first byte of the private section, located from the end).
    let mut r = Reader::new(&raw[AUTH_MAGIC.len()..], "t");
    for _ in 0..3 {
        r.string().unwrap();
    }
    r.u32().unwrap();
    r.string().unwrap();
    let private_len = r.string().unwrap().len();
    let private_start = raw.len() - private_len;
    let mut bad_check = raw.clone();
    bad_check[private_start] ^= 0xff;
    assert!(SkKey::from_openssh_private_key(&rewrap(&bad_check)).is_err());

    let mut bad_pad = raw.clone();
    let last = bad_pad.len() - 1;
    bad_pad[last] ^= 0x40;
    assert!(SkKey::from_openssh_private_key(&rewrap(&bad_pad)).is_err());

    let mut trailing = raw.clone();
    trailing.push(0);
    assert!(SkKey::from_openssh_private_key(&rewrap(&trailing)).is_err());
    assert!(SkKey::from_openssh_private_key(&rewrap(&raw[..raw.len() - 3])).is_err());

    // An encrypted key is reported as unsupported.
    let mut w = Writer::new();
    w.raw(AUTH_MAGIC)
        .string(b"aes256-ctr")
        .string(b"bcrypt")
        .string(b"")
        .u32(1);
    assert!(matches!(
        SkKey::from_openssh_private_key(&rewrap(&w.into_vec())),
        Err(Error::Unsupported(_))
    ));

    // A non-sk key file is rejected.
    let ed = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACCzPq7zfqLffKoBDe/eo04kH2XxtSmk9D7RQyf1xUqrYgAAAJgAIAxdACAM
XQAAAAtzc2gtZWQyNTUxOQAAACCzPq7zfqLffKoBDe/eo04kH2XxtSmk9D7RQyf1xUqrYg
AAAEC2BsIi0QwW2uFscKTUUXNHLsYX4FxlaSDSblbAj7WR7bM+rvN+ot98qgEN796jTiQf
ZfG1KaT0PtFDJ/XFSqtiAAAAEHVzZXJAZXhhbXBsZS5jb20BAgMEBQ==
-----END OPENSSH PRIVATE KEY-----";
    assert!(matches!(
        SkKey::from_openssh_private_key(ed),
        Err(Error::Malformed { .. })
    ));
}

#[test]
fn rebuild_from_stored_metadata() {
    let original = key_from_pub(SK_ECDSA_PUB);
    let core_model::KeyStorage::Fido2 {
        application,
        require_pin,
        require_presence,
        ..
    } = original.key_storage(core_model::SecretRef::new())
    else {
        panic!("expected Fido2 storage");
    };
    assert_eq!(application, "ssh:");
    assert!(require_presence && !require_pin);

    // What the app persists: the OpenSSH line in KeyMeta, the handle in the vault.
    let line = original.public_key_openssh("comment");
    let public = PublicKey::from_openssh(&line).unwrap();
    let flags = key_flags::from_requirements(require_presence, require_pin);
    let rebuilt = SkKey::from_public_key(&public, flags, original.key_handle().to_vec()).unwrap();
    assert_eq!(rebuilt, original);
    assert_eq!(
        core_model::KeyAlgorithm::from(rebuilt.algorithm()).ssh_name(),
        SK_ECDSA_P256_ALGORITHM
    );

    let not_sk = PublicKey::from_openssh(
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILM+rvN+ot98qgEN796jTiQfZfG1KaT0PtFDJ/XFSqti",
    )
    .unwrap();
    assert!(SkKey::from_public_key(&not_sk, 1, vec![1]).is_err());
    assert_eq!(key_flags::from_requirements(false, true), 0x04);
}
