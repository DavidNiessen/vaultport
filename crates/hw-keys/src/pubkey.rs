// SPDX-License-Identifier: GPL-3.0-or-later
//! Public key helpers: building SSH public keys from platform encodings, OpenSSH
//! `authorized_keys` lines and `SHA256:` fingerprints.

use ssh_key::{HashAlg, PublicKey};

use crate::ecdsa::ECDSA_P256_ALGORITHM;
use crate::error::{Error, Result};
use crate::wire::Writer;

/// SSH name of RSA public keys.
pub const RSA_ALGORITHM: &str = "ssh-rsa";
/// SSH name of RSA PKCS#1 v1.5 / SHA-256 signatures ([RFC 8332]).
///
/// [RFC 8332]: https://www.rfc-editor.org/rfc/rfc8332
pub const RSA_SHA256_SIGNATURE_ALGORITHM: &str = "rsa-sha2-256";

/// Minimum accepted RSA modulus size.
const MIN_RSA_BITS: usize = 2048;
/// Maximum accepted RSA modulus size (matches OpenSSH's `SSHBUF_MAX_BIGNUM`).
const MAX_RSA_BITS: usize = 16384;

/// Parses a P-256 public key given either as a SEC1 point (uncompressed `04 || x || y` or
/// compressed) or as a DER `SubjectPublicKeyInfo` (what Android's `PublicKey.getEncoded()`
/// returns), validates that it is on the curve and returns the `ecdsa-sha2-nistp256` key.
pub fn ecdsa_p256_public_key(encoded: &[u8]) -> Result<PublicKey> {
    let point = p256_uncompressed_point(encoded)?;
    let mut w = Writer::new();
    w.string(ECDSA_P256_ALGORITHM.as_bytes())
        .string(b"nistp256")
        .string(&point);
    Ok(PublicKey::from_bytes(&w.into_vec())?)
}

/// Validates a P-256 point and returns its 65-byte uncompressed SEC1 encoding.
pub(crate) fn p256_uncompressed_point(encoded: &[u8]) -> Result<[u8; 65]> {
    use p256::elliptic_curve::sec1::ToSec1Point as _;
    use p256::pkcs8::DecodePublicKey as _;

    let key = if encoded.first() == Some(&0x30) {
        p256::PublicKey::from_public_key_der(encoded)
            .map_err(|_| Error::malformed("P-256 public key", "invalid SubjectPublicKeyInfo"))?
    } else {
        p256::PublicKey::from_sec1_bytes(encoded)
            .map_err(|_| Error::malformed("P-256 public key", "invalid SEC1 point"))?
    };
    let point = key.to_sec1_point(false);
    <[u8; 65]>::try_from(point.as_bytes())
        .map_err(|_| Error::malformed("P-256 public key", "unexpected point length"))
}

/// Parses a Windows CNG `BCRYPT_RSAPUBLIC_BLOB` (as returned by
/// `KeyCredential.RetrievePublicKey(CryptographicPublicKeyBlobType.BCryptPublicKey)`) into an
/// `ssh-rsa` public key.
///
/// Layout (little-endian header, big-endian numbers):
/// `u32 magic "RSA1" | u32 bit_length | u32 cb_public_exp | u32 cb_modulus | u32 cb_prime1 |
/// u32 cb_prime2 | exponent | modulus`.
pub fn rsa_public_key_from_bcrypt_blob(blob: &[u8]) -> Result<PublicKey> {
    const WHAT: &str = "BCRYPT_RSAPUBLIC_BLOB";
    const MAGIC: u32 = 0x3141_5352; // "RSA1"
    const HEADER: usize = 24;

    let field = |i: usize| -> Result<u32> {
        blob.get(i * 4..i * 4 + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or(Error::malformed(WHAT, "truncated header"))
    };
    if field(0)? != MAGIC {
        return Err(Error::malformed(WHAT, "not an RSA public key blob"));
    }
    let bit_length = usize::try_from(field(1)?).map_err(|_| Error::malformed(WHAT, "size"))?;
    let exp_len = usize::try_from(field(2)?).map_err(|_| Error::malformed(WHAT, "size"))?;
    let mod_len = usize::try_from(field(3)?).map_err(|_| Error::malformed(WHAT, "size"))?;
    if field(4)? != 0 || field(5)? != 0 {
        return Err(Error::malformed(WHAT, "private key material present"));
    }
    if !(MIN_RSA_BITS..=MAX_RSA_BITS).contains(&bit_length) || mod_len != bit_length.div_ceil(8) {
        return Err(Error::malformed(WHAT, "unsupported modulus size"));
    }
    if exp_len == 0 || exp_len > 8 {
        return Err(Error::malformed(WHAT, "unsupported exponent size"));
    }
    let expected = HEADER
        .checked_add(exp_len)
        .and_then(|n| n.checked_add(mod_len))
        .ok_or(Error::malformed(WHAT, "size overflow"))?;
    if blob.len() != expected {
        return Err(Error::malformed(WHAT, "length mismatch"));
    }
    let exponent = &blob[HEADER..HEADER + exp_len];
    let modulus = &blob[HEADER + exp_len..];
    if modulus[0] == 0 || modulus[mod_len - 1] & 1 == 0 {
        return Err(Error::malformed(WHAT, "invalid modulus"));
    }
    if exponent.last().is_none_or(|b| b & 1 == 0) || exponent.iter().all(|&b| b == 0) {
        return Err(Error::malformed(WHAT, "invalid exponent"));
    }
    let mut w = Writer::new();
    w.string(RSA_ALGORITHM.as_bytes())
        .mpint_unsigned(exponent)
        .mpint_unsigned(modulus);
    Ok(PublicKey::from_bytes(&w.into_vec())?)
}

/// Builds a complete `rsa-sha2-256` SSH signature from a raw PKCS#1 v1.5 signature,
/// left-padding it to the modulus length as OpenSSH expects.
pub fn rsa_sha256_ssh_signature(public_key: &PublicKey, raw: &[u8]) -> Result<Vec<u8>> {
    let rsa = public_key
        .key_data()
        .rsa()
        .ok_or(Error::malformed("RSA signature", "key is not an RSA key"))?;
    let modulus_len = rsa
        .n()
        .as_positive_bytes()
        .map(<[u8]>::len)
        .ok_or(Error::malformed("RSA signature", "invalid modulus"))?;
    let significant = raw.iter().position(|&b| b != 0).unwrap_or(raw.len());
    let digits = &raw[significant..];
    if digits.is_empty() || digits.len() > modulus_len {
        return Err(Error::malformed(
            "RSA signature",
            "length does not match modulus",
        ));
    }
    let mut padded = vec![0u8; modulus_len - digits.len()];
    padded.extend_from_slice(digits);
    let mut w = Writer::new();
    w.string(RSA_SHA256_SIGNATURE_ALGORITHM.as_bytes())
        .string(&padded);
    Ok(w.into_vec())
}

/// Formats `key` as an OpenSSH `authorized_keys` line (`<algorithm> <base64> [comment]`).
pub fn openssh_line(key: &PublicKey, comment: &str) -> String {
    let mut key = key.clone();
    key.set_comment(comment);
    // Encoding a parsed public key into a String cannot fail; fall back to the bare
    // algorithm name rather than panicking should that invariant ever break.
    key.to_openssh()
        .unwrap_or_else(|_| key.algorithm().as_str().to_owned())
}

/// The OpenSSH `SHA256:<base64>` fingerprint of `key`.
pub fn fingerprint_sha256(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The public key from `id_sk_ecdsa_p256_2.pub` in the `ssh-key` test suite, as a SEC1
    /// point.
    const POINT_HEX: &str = "04d768e977e14cad3cd2ecf951b1b239c34ca3e4779cf5cc071babc07d8cd5b0ed4186c34faa8113b11d40691ccbf1090d76f34f42a553dad13a3717f89b20b460";

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn ecdsa_key_from_sec1_and_spki() {
        let point = hex(POINT_HEX);
        let key = ecdsa_p256_public_key(&point).unwrap();
        let line = openssh_line(&key, "me@host");
        assert_eq!(
            line,
            "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBNdo6XfhTK080uz5UbGyOcNMo+R3nPXMBxurwH2M1bDtQYbDT6qBE7EdQGkcy/EJDXbzT0KlU9rROjcX+JsgtGA= me@host"
        );
        assert!(fingerprint_sha256(&key).starts_with("SHA256:"));

        // Compressed form of the same point.
        let mut compressed = vec![0x02 | (point[64] & 1)];
        compressed.extend_from_slice(&point[1..33]);
        assert_eq!(ecdsa_p256_public_key(&compressed).unwrap(), key);

        // SubjectPublicKeyInfo: SEQUENCE { SEQUENCE { id-ecPublicKey, prime256v1 }, BIT STRING }
        let mut spki = hex("3059301306072a8648ce3d020106082a8648ce3d030107034200");
        spki.extend_from_slice(&point);
        assert_eq!(ecdsa_p256_public_key(&spki).unwrap(), key);
    }

    #[test]
    fn ecdsa_key_rejects_garbage() {
        let mut point = hex(POINT_HEX);
        point[64] ^= 1; // off the curve
        assert!(ecdsa_p256_public_key(&point).is_err());
        assert!(ecdsa_p256_public_key(&[]).is_err());
        assert!(ecdsa_p256_public_key(&[0x30, 0x00]).is_err());
        assert!(ecdsa_p256_public_key(&[0x04; 10]).is_err());
    }

    fn bcrypt_blob(bits: u32, exponent: &[u8], modulus: &[u8]) -> Vec<u8> {
        let mut blob = Vec::new();
        for v in [
            0x3141_5352,
            bits,
            exponent.len() as u32,
            modulus.len() as u32,
            0,
            0,
        ] {
            blob.extend_from_slice(&u32::to_le_bytes(v));
        }
        blob.extend_from_slice(exponent);
        blob.extend_from_slice(modulus);
        blob
    }

    #[test]
    fn rsa_key_from_bcrypt_blob() {
        let mut modulus = vec![0xc5; 256];
        modulus[255] = 0x01;
        let blob = bcrypt_blob(2048, &[0x01, 0x00, 0x01], &modulus);
        let key = rsa_public_key_from_bcrypt_blob(&blob).unwrap();
        let rsa = key.key_data().rsa().unwrap();
        assert_eq!(rsa.e().as_positive_bytes().unwrap(), &[0x01, 0x00, 0x01]);
        assert_eq!(rsa.n().as_positive_bytes().unwrap(), modulus.as_slice());
        assert!(openssh_line(&key, "").starts_with("ssh-rsa AAAAB3NzaC1yc2E"));

        // Signature is padded to the modulus length.
        let sig = rsa_sha256_ssh_signature(&key, &[0x00, 0x00, 0x42]).unwrap();
        let parsed = ssh_key::Signature::try_from(sig.as_slice()).unwrap();
        assert_eq!(parsed.algorithm().as_str(), "rsa-sha2-256");
        assert_eq!(parsed.as_bytes().len(), 256);
        assert_eq!(parsed.as_bytes()[255], 0x42);
        assert!(rsa_sha256_ssh_signature(&key, &[0x01; 257]).is_err());
        assert!(rsa_sha256_ssh_signature(&key, &[0x00; 4]).is_err());
    }

    #[test]
    fn rsa_blob_rejects_malformed() {
        let mut modulus = vec![0xc5; 256];
        modulus[255] = 0x01;
        let good = bcrypt_blob(2048, &[0x01, 0x00, 0x01], &modulus);
        assert!(rsa_public_key_from_bcrypt_blob(&good).is_ok());

        let mut bad_magic = good.clone();
        bad_magic[0] = 0;
        let mut truncated = good.clone();
        truncated.pop();
        let mut trailing = good.clone();
        trailing.push(0);
        let mut private = good.clone();
        private[16] = 1;
        let small = bcrypt_blob(1024, &[1, 0, 1], &modulus[..128]);
        let even_exp = bcrypt_blob(2048, &[1, 0, 0], &modulus);
        let mut even_mod = modulus.clone();
        even_mod[255] = 0x02;
        let even_mod = bcrypt_blob(2048, &[1, 0, 1], &even_mod);
        let wrong_bits = bcrypt_blob(2056, &[1, 0, 1], &modulus);

        for (i, case) in [
            bad_magic, truncated, trailing, private, small, even_exp, even_mod, wrong_bits,
        ]
        .iter()
        .enumerate()
        {
            assert!(rsa_public_key_from_bcrypt_blob(case).is_err(), "case {i}");
        }
        assert!(rsa_public_key_from_bcrypt_blob(&[]).is_err());
    }
}
