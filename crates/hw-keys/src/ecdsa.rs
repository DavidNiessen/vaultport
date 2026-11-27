// SPDX-License-Identifier: GPL-3.0-or-later
//! ECDSA P-256 signature conversion between platform formats and SSH.
//!
//! Platform key stores return ECDSA signatures either as ASN.1 DER
//! (`ECDSA-Sig-Value ::= SEQUENCE { r INTEGER, s INTEGER }`: Secure Enclave, Android
//! Keystore, FIDO2 ES256) or as the fixed-width IEEE P1363 concatenation `r || s` (Windows
//! CNG). SSH ([RFC 5656 §3.1.2]) encodes the signature blob as
//!
//! ```text
//! string  "ecdsa-sha2-nistp256"
//! string  ecdsa_signature_blob      ; mpint r || mpint s
//! ```
//!
//! The DER parser is deliberately strict (minimal lengths, minimal positive integers, no
//! trailing data) and both scalars are range-checked against the curve order, so a
//! malformed or malicious platform response can never turn into a bogus signature.
//!
//! [RFC 5656 §3.1.2]: https://www.rfc-editor.org/rfc/rfc5656#section-3.1.2

use crate::error::{Error, Result};
use crate::wire::{Reader, Writer, left_pad};

/// SSH algorithm name of plain ECDSA P-256 keys and signatures.
pub const ECDSA_P256_ALGORITHM: &str = "ecdsa-sha2-nistp256";

const WHAT: &str = "ECDSA signature";
const SCALAR_LEN: usize = 32;

/// A validated ECDSA P-256 signature as big-endian `r` and `s` scalars.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EcdsaP256Signature {
    r: [u8; SCALAR_LEN],
    s: [u8; SCALAR_LEN],
}

impl std::fmt::Debug for EcdsaP256Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Signatures are public, but there is no value in dumping them into logs.
        f.debug_struct("EcdsaP256Signature").finish_non_exhaustive()
    }
}

impl EcdsaP256Signature {
    /// Builds a signature from big-endian scalars, checking `0 < r, s < n`.
    pub fn from_scalars(r: [u8; SCALAR_LEN], s: [u8; SCALAR_LEN]) -> Result<Self> {
        p256::ecdsa::Signature::from_scalars(r, s)
            .map_err(|_| Error::malformed(WHAT, "scalar out of range"))?;
        Ok(Self { r, s })
    }

    /// Parses a strict ASN.1 DER `ECDSA-Sig-Value`.
    pub fn from_der(der: &[u8]) -> Result<Self> {
        let mut input = der;
        let body = der_tlv(&mut input, 0x30)?;
        if !input.is_empty() {
            return Err(Error::malformed(WHAT, "trailing data after DER sequence"));
        }
        let mut body = body;
        let r = der_positive_integer(&mut body)?;
        let s = der_positive_integer(&mut body)?;
        if !body.is_empty() {
            return Err(Error::malformed(WHAT, "trailing data inside DER sequence"));
        }
        Self::from_scalars(r, s)
    }

    /// Parses the 64-byte IEEE P1363 form `r || s`.
    pub fn from_p1363(raw: &[u8]) -> Result<Self> {
        if raw.len() != 2 * SCALAR_LEN {
            return Err(Error::malformed(WHAT, "P1363 signature must be 64 bytes"));
        }
        let mut r = [0u8; SCALAR_LEN];
        let mut s = [0u8; SCALAR_LEN];
        r.copy_from_slice(&raw[..SCALAR_LEN]);
        s.copy_from_slice(&raw[SCALAR_LEN..]);
        Self::from_scalars(r, s)
    }

    /// Parses the inner SSH blob `mpint r || mpint s`.
    pub fn from_ssh_blob(blob: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(blob, WHAT);
        let r = reader.mpint_unsigned(SCALAR_LEN)?;
        let s = reader.mpint_unsigned(SCALAR_LEN)?;
        reader.finish()?;
        let r = left_pad(r).ok_or(Error::malformed(WHAT, "scalar too large"))?;
        let s = left_pad(s).ok_or(Error::malformed(WHAT, "scalar too large"))?;
        Self::from_scalars(r, s)
    }

    /// Big-endian `r`.
    pub const fn r(&self) -> &[u8; SCALAR_LEN] {
        &self.r
    }

    /// Big-endian `s`.
    pub const fn s(&self) -> &[u8; SCALAR_LEN] {
        &self.s
    }

    /// The inner SSH blob `mpint r || mpint s` (as embedded in both `ecdsa-sha2-nistp256`
    /// and `sk-ecdsa-sha2-nistp256@openssh.com` signatures).
    pub fn to_ssh_blob(self) -> Vec<u8> {
        let mut w = Writer::new();
        w.mpint_unsigned(&self.r).mpint_unsigned(&self.s);
        w.into_vec()
    }

    /// The complete `ecdsa-sha2-nistp256` SSH signature
    /// (`string algorithm || string blob`).
    pub fn to_ssh_signature(self) -> Vec<u8> {
        let mut w = Writer::new();
        w.string(ECDSA_P256_ALGORITHM.as_bytes())
            .string(&self.to_ssh_blob());
        w.into_vec()
    }

    pub(crate) fn to_p256(self) -> Result<p256::ecdsa::Signature> {
        p256::ecdsa::Signature::from_scalars(self.r, self.s)
            .map_err(|_| Error::malformed(WHAT, "scalar out of range"))
    }
}

/// Converts a DER ECDSA P-256 signature into a complete `ecdsa-sha2-nistp256` SSH signature.
pub fn der_to_ssh_signature(der: &[u8]) -> Result<Vec<u8>> {
    Ok(EcdsaP256Signature::from_der(der)?.to_ssh_signature())
}

/// Converts a P1363 (`r || s`) ECDSA P-256 signature into a complete SSH signature.
pub fn p1363_to_ssh_signature(raw: &[u8]) -> Result<Vec<u8>> {
    Ok(EcdsaP256Signature::from_p1363(raw)?.to_ssh_signature())
}

/// Reads one DER TLV with the expected tag and returns its contents.
fn der_tlv<'a>(input: &mut &'a [u8], tag: u8) -> Result<&'a [u8]> {
    let (&actual_tag, rest) = input
        .split_first()
        .ok_or(Error::malformed(WHAT, "truncated DER"))?;
    if actual_tag != tag {
        return Err(Error::malformed(WHAT, "unexpected DER tag"));
    }
    let (&len_byte, mut rest) = rest
        .split_first()
        .ok_or(Error::malformed(WHAT, "truncated DER length"))?;
    let len = match len_byte {
        0..=0x7f => usize::from(len_byte),
        0x81 => {
            let (&len, tail) = rest
                .split_first()
                .ok_or(Error::malformed(WHAT, "truncated DER length"))?;
            if len < 0x80 {
                return Err(Error::malformed(WHAT, "non-minimal DER length"));
            }
            rest = tail;
            usize::from(len)
        }
        // A P-256 signature is at most 72 bytes; longer forms and the indefinite form are
        // never valid here.
        _ => return Err(Error::malformed(WHAT, "unsupported DER length form")),
    };
    if rest.len() < len {
        return Err(Error::malformed(WHAT, "truncated DER value"));
    }
    let (value, tail) = rest.split_at(len);
    *input = tail;
    Ok(value)
}

/// Reads a minimally encoded, strictly positive DER INTEGER of at most 32 significant bytes.
fn der_positive_integer(input: &mut &[u8]) -> Result<[u8; SCALAR_LEN]> {
    let value = der_tlv(input, 0x02)?;
    let (&first, _) = value
        .split_first()
        .ok_or(Error::malformed(WHAT, "empty DER integer"))?;
    if first & 0x80 != 0 {
        return Err(Error::malformed(WHAT, "negative DER integer"));
    }
    let magnitude = if first == 0 {
        match value.get(1) {
            None => return Err(Error::malformed(WHAT, "zero scalar")),
            Some(next) if next & 0x80 == 0 => {
                return Err(Error::malformed(WHAT, "non-minimal DER integer"));
            }
            Some(_) => &value[1..],
        }
    } else {
        value
    };
    left_pad(magnitude).ok_or(Error::malformed(WHAT, "DER integer too large"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use p256::ecdsa::signature::Signer as _;
    use ssh_key::{PublicKey, Signature, public::KeyData};

    fn test_key() -> p256::ecdsa::SigningKey {
        // Fixed scalar so the vectors are reproducible.
        let scalar = [
            0x51, 0x9b, 0x42, 0x3d, 0x71, 0x5f, 0x8b, 0x58, 0x1f, 0x4f, 0xa8, 0xee, 0x59, 0xf4,
            0x77, 0x1a, 0x5b, 0x44, 0xc8, 0x13, 0x0b, 0x4e, 0x3e, 0xac, 0xca, 0x54, 0xa5, 0x6d,
            0xda, 0x72, 0xb4, 0x64,
        ];
        p256::ecdsa::SigningKey::from_bytes(&scalar.into()).unwrap()
    }

    fn ssh_public_key(key: &p256::ecdsa::SigningKey) -> PublicKey {
        let point = key.verifying_key().to_sec1_point(false);
        let ecdsa = ssh_key::public::EcdsaPublicKey::from_sec1_bytes(point.as_bytes()).unwrap();
        PublicKey::new(KeyData::Ecdsa(ecdsa), "")
    }

    #[test]
    fn der_signature_converts_to_verifiable_ssh_signature() {
        let key = test_key();
        let public = ssh_public_key(&key);
        for message in [&b""[..], b"hello", &[0xa5; 1000]] {
            let sig: p256::ecdsa::Signature = key.sign(message);
            let der = sig.to_der();
            let blob = der_to_ssh_signature(der.as_bytes()).unwrap();
            let parsed = Signature::try_from(blob.as_slice()).unwrap();
            p256::ecdsa::signature::Verifier::verify(public.key_data(), message, &parsed).unwrap();

            // P1363 path produces the identical blob.
            let raw = sig.to_bytes();
            assert_eq!(p1363_to_ssh_signature(&raw).unwrap(), blob);
        }
    }

    #[test]
    fn ssh_blob_roundtrip() {
        let sig: p256::ecdsa::Signature = test_key().sign(b"roundtrip");
        let parsed = EcdsaP256Signature::from_p1363(&sig.to_bytes()).unwrap();
        let blob = parsed.to_ssh_blob();
        assert_eq!(EcdsaP256Signature::from_ssh_blob(&blob).unwrap(), parsed);
    }

    #[test]
    fn fixed_der_vector() {
        // r has its high bit set (needs a 0x00 pad in DER and in the SSH mpint); s is short.
        let mut der = vec![0x30, 0x27, 0x02, 0x21, 0x00];
        der.extend_from_slice(&[0x80; 32]);
        der.extend_from_slice(&[0x02, 0x02, 0x01, 0x02]);
        let sig = EcdsaP256Signature::from_der(&der).unwrap();
        assert_eq!(sig.r(), &[0x80; 32]);
        let mut s = [0u8; 32];
        s[30] = 1;
        s[31] = 2;
        assert_eq!(sig.s(), &s);

        let mut expected = Vec::new();
        expected.extend_from_slice(&[0, 0, 0, 0x13]);
        expected.extend_from_slice(b"ecdsa-sha2-nistp256");
        expected.extend_from_slice(&[0, 0, 0, 0x2b]);
        expected.extend_from_slice(&[0, 0, 0, 0x21, 0x00]);
        expected.extend_from_slice(&[0x80; 32]);
        expected.extend_from_slice(&[0, 0, 0, 2, 1, 2]);
        assert_eq!(sig.to_ssh_signature(), expected);
    }

    #[test]
    fn rejects_malformed_der() {
        let valid = {
            let sig: p256::ecdsa::Signature = test_key().sign(b"x");
            sig.to_der().as_bytes().to_vec()
        };
        assert!(EcdsaP256Signature::from_der(&valid).is_ok());

        let mut cases: Vec<Vec<u8>> = vec![
            vec![],
            vec![0x30],
            vec![0x30, 0x00],
            vec![0x31, 0x06, 0x02, 0x01, 0x01, 0x02, 0x01, 0x01], // wrong outer tag
            vec![0x30, 0x06, 0x03, 0x01, 0x01, 0x02, 0x01, 0x01], // wrong inner tag
            vec![0x30, 0x07, 0x02, 0x01, 0x01, 0x02, 0x01, 0x01], // length too long
            vec![0x30, 0x05, 0x02, 0x01, 0x01, 0x02, 0x01, 0x01], // length too short
            vec![0x30, 0x06, 0x02, 0x01, 0x81, 0x02, 0x01, 0x01], // negative r
            vec![0x30, 0x06, 0x02, 0x01, 0x01, 0x02, 0x01, 0xff], // negative s
            vec![0x30, 0x06, 0x02, 0x01, 0x00, 0x02, 0x01, 0x01], // zero r
            vec![0x30, 0x07, 0x02, 0x02, 0x00, 0x01, 0x02, 0x01, 0x01], // non-minimal r
            vec![0x30, 0x06, 0x02, 0x00, 0x02, 0x02, 0x01, 0x01], // empty integer
            vec![0x30, 0x80, 0x02, 0x01, 0x01, 0x02, 0x01, 0x01, 0x00, 0x00], // indefinite
            vec![0x30, 0x81, 0x06, 0x02, 0x01, 0x01, 0x02, 0x01, 0x01], // non-minimal length
            vec![0x30, 0x82, 0x00, 0x06, 0x02, 0x01, 0x01, 0x02, 0x01, 0x01], // long form
            vec![0x30, 0x03, 0x02, 0x01, 0x01],                   // missing s
            vec![
                0x30, 0x09, 0x02, 0x01, 0x01, 0x02, 0x01, 0x01, 0x02, 0x01, 0x01,
            ], // extra int
        ];
        // Trailing byte after the sequence.
        let mut trailing = valid.clone();
        trailing.push(0);
        cases.push(trailing);
        // Truncated valid signature.
        cases.push(valid[..valid.len() - 1].to_vec());
        // r of 33 significant bytes.
        let mut big = vec![0x30, 0x26, 0x02, 0x21];
        big.extend_from_slice(&[0x01; 33]);
        big.extend_from_slice(&[0x02, 0x01, 0x01]);
        cases.push(big);
        // r equal to the group order n.
        let n = [
            0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xbc, 0xe6, 0xfa, 0xad, 0xa7, 0x17, 0x9e, 0x84, 0xf3, 0xb9, 0xca, 0xc2,
            0xfc, 0x63, 0x25, 0x51,
        ];
        let mut order = vec![0x30, 0x26, 0x02, 0x21, 0x00];
        order.extend_from_slice(&n);
        order.extend_from_slice(&[0x02, 0x01, 0x01]);
        cases.push(order);

        for (i, case) in cases.iter().enumerate() {
            assert!(
                EcdsaP256Signature::from_der(case).is_err(),
                "case {i} should be rejected: {case:02x?}"
            );
        }
    }

    #[test]
    fn rejects_malformed_p1363_and_ssh_blob() {
        assert!(EcdsaP256Signature::from_p1363(&[1; 63]).is_err());
        assert!(EcdsaP256Signature::from_p1363(&[0; 64]).is_err());
        assert!(EcdsaP256Signature::from_p1363(&[0xff; 64]).is_err());
        assert!(EcdsaP256Signature::from_ssh_blob(&[0, 0, 0, 1, 1]).is_err());
        assert!(EcdsaP256Signature::from_ssh_blob(&[0, 0, 0, 1, 1, 0, 0, 0, 1, 0x80]).is_err());
        assert!(
            EcdsaP256Signature::from_ssh_blob(&[0, 0, 0, 1, 1, 0, 0, 0, 1, 1, 0]).is_err(),
            "trailing data"
        );
    }
}
