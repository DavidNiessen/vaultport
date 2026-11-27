// SPDX-License-Identifier: GPL-3.0-or-later
//! AES-128-OCB3 datagram sealing, as in mosh's `src/crypto/crypto.cc`.
//!
//! * The 96-bit OCB nonce is four zero bytes followed by a 64-bit big-endian value whose top
//!   bit is the direction ([`TO_CLIENT`]) and whose lower 63 bits are the sequence number.
//! * On the wire a datagram is `nonce[4..12] || ciphertext || tag(16)`; no associated data.

use aes::Aes128;
use ocb3::aead::AeadInOut;
use ocb3::aead::KeyInit;
use ocb3::aead::consts::{U12, U16};
use ocb3::{Nonce, Ocb3, Tag};

use crate::key::MoshKey;

/// Direction bit of the nonce value: set for server → client packets.
pub const TO_CLIENT: u64 = 1 << 63;
/// Mask of the sequence number inside the nonce value.
pub const SEQUENCE_MASK: u64 = !TO_CLIENT;
/// Bytes of the nonce carried on the wire.
pub const NONCE_WIRE_LEN: usize = 8;
/// OCB authentication tag length.
pub const TAG_LEN: usize = 16;
/// Bytes added to every plaintext by sealing.
pub const OVERHEAD: usize = NONCE_WIRE_LEN + TAG_LEN;

type Aes128Ocb3 = Ocb3<Aes128, U12, U16>;

/// Builds the 12-byte OCB nonce for a direction/sequence value.
pub fn nonce_bytes(value: u64) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[4..].copy_from_slice(&value.to_be_bytes());
    n
}

/// Authentication failure or malformed datagram. Deliberately carries no detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("datagram failed authentication")]
pub struct AuthError;

/// A keyed AES-128-OCB3 instance.
///
/// The AES key schedule is wiped on drop by the `aes` crate's `zeroize` feature. The OCB
/// key-dependent offsets held by the `ocb3` crate are overwritten on drop (best effort, as the
/// crate offers no zeroization hook of its own).
pub struct Cipher {
    aead: Box<Aes128Ocb3>,
}

impl Cipher {
    /// Creates a cipher for `key`.
    pub fn new(key: &MoshKey) -> Self {
        let aead = Aes128Ocb3::new(key.as_bytes().into());
        Self {
            aead: Box::new(aead),
        }
    }

    /// Seals `plaintext` under the nonce `value` (direction bit | sequence number) and returns
    /// the full wire datagram.
    pub fn seal(&self, value: u64, plaintext: &[u8]) -> Vec<u8> {
        let nonce_full = nonce_bytes(value);
        let nonce = Nonce::<U12>::from(nonce_full);
        let mut out = Vec::with_capacity(plaintext.len() + OVERHEAD);
        out.extend_from_slice(&nonce_full[4..]);
        out.extend_from_slice(plaintext);
        let tag = self
            .aead
            .encrypt_inout_detached(&nonce, &[], (&mut out[NONCE_WIRE_LEN..]).into())
            // Only fails for inputs over 1 GiB, which cannot be a datagram.
            .expect("datagram too large for OCB");
        out.extend_from_slice(&tag);
        out
    }

    /// Authenticates and opens a wire datagram, returning the nonce value and the plaintext.
    /// Nothing is parsed before the tag has been verified.
    pub fn open(&self, wire: &[u8]) -> Result<(u64, Vec<u8>), AuthError> {
        if wire.len() < OVERHEAD {
            return Err(AuthError);
        }
        let mut value_bytes = [0u8; 8];
        value_bytes.copy_from_slice(&wire[..NONCE_WIRE_LEN]);
        let value = u64::from_be_bytes(value_bytes);
        let nonce = Nonce::<U12>::from(nonce_bytes(value));
        let (body, tag) = wire[NONCE_WIRE_LEN..].split_at(wire.len() - OVERHEAD);
        let tag = Tag::<U16>::try_from(tag).map_err(|_| AuthError)?;
        let mut plaintext = body.to_vec();
        self.aead
            .decrypt_inout_detached(&nonce, &[], plaintext.as_mut_slice().into(), &tag)
            .map_err(|_| AuthError)?;
        Ok((value, plaintext))
    }
}

impl Drop for Cipher {
    fn drop(&mut self) {
        // Replace the key-dependent OCB state in place with that of an all-zero key. The old
        // value's AES key schedule zeroizes itself when it is dropped by this assignment.
        *self.aead = Aes128Ocb3::new(&[0u8; 16].into());
        std::hint::black_box(&*self.aead);
    }
}

impl std::fmt::Debug for Cipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Cipher(AES-128-OCB3)")
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// RFC 7253 appendix A, AES-128, TAGLEN 128, N = BBAA99887766554433221101, A = P = 0001..07:
    /// checks that the `ocb3` parameterisation (12-byte nonce, 16-byte tag) is the RFC one.
    #[test]
    fn rfc7253_vector() {
        let aead = Aes128Ocb3::new(
            &hex("000102030405060708090A0B0C0D0E0F")[..]
                .try_into()
                .unwrap(),
        );
        let nonce = Nonce::<U12>::try_from(&hex("BBAA99887766554433221101")[..]).unwrap();
        let ad = hex("0001020304050607");
        let mut buf = hex("0001020304050607");
        let tag = aead
            .encrypt_inout_detached(&nonce, &ad, buf.as_mut_slice().into())
            .unwrap();
        buf.extend_from_slice(&tag);
        assert_eq!(buf, hex("6820B3657B6F615A5725BDA0D3B4EB3A257C9AF1F8F03009"));
    }

    /// Vector generated independently with Python `cryptography`'s `AESOCB3`:
    /// key = 00..0f (MOSH_KEY "AAECAwQFBgcICQoLDA0ODw"), value = TO_CLIENT | 5,
    /// plaintext = timestamps 0x1234/0xffff || "hello".
    #[test]
    fn mosh_known_vector() {
        let key = MoshKey::parse("AAECAwQFBgcICQoLDA0ODw").unwrap();
        let c = Cipher::new(&key);
        let pt = hex("1234ffff68656c6c6f");
        let wire = c.seal(TO_CLIENT | 5, &pt);
        assert_eq!(
            wire,
            hex("8000000000000005\
                 7536957660dbae0ca6e1b76b9c5b048786f34d266674fb8fb8")
        );
        let (v, back) = c.open(&wire).unwrap();
        assert_eq!(v, TO_CLIENT | 5);
        assert_eq!(back, pt);
    }

    #[test]
    fn tamper_and_truncation_rejected() {
        let c = Cipher::new(&MoshKey::from_bytes([9; 16]));
        let wire = c.seal(42, b"payload");
        assert_eq!(c.open(&wire).unwrap(), (42, b"payload".to_vec()));
        for i in 0..wire.len() {
            let mut t = wire.clone();
            t[i] ^= 0x01;
            assert_eq!(c.open(&t), Err(AuthError), "flip at {i}");
        }
        assert_eq!(c.open(&wire[..OVERHEAD - 1]), Err(AuthError));
        assert_eq!(c.open(&wire[..wire.len() - 1]), Err(AuthError));
        let other = Cipher::new(&MoshKey::from_bytes([8; 16]));
        assert_eq!(other.open(&wire), Err(AuthError));
    }

    #[test]
    fn empty_plaintext() {
        let c = Cipher::new(&MoshKey::from_bytes([1; 16]));
        let wire = c.seal(0, b"");
        assert_eq!(wire.len(), OVERHEAD);
        assert_eq!(c.open(&wire).unwrap(), (0, vec![]));
    }
}
