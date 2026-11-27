// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal SSH wire-format helpers (RFC 4251 `string` / `mpint`) and fingerprints.

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use sha2::{Digest, Sha256};

/// OpenSSH-style `SHA256:` fingerprint of a public key blob (base64 without padding).
#[must_use]
pub fn sha256_fingerprint(blob: &[u8]) -> String {
    format!("SHA256:{}", STANDARD_NO_PAD.encode(Sha256::digest(blob)))
}

/// Decodes a base64 public key blob as found in `known_hosts` / `authorized_keys`.
pub(crate) fn decode_blob(b64: &str) -> Option<Vec<u8>> {
    STANDARD
        .decode(b64)
        .or_else(|_| STANDARD_NO_PAD.decode(b64))
        .ok()
        .filter(|b| !b.is_empty())
}

pub(crate) fn encode_blob(blob: &[u8]) -> String {
    STANDARD.encode(blob)
}

/// Reads one `string` field; returns `(field, rest)`.
pub(crate) fn read_string(buf: &[u8]) -> Option<(&[u8], &[u8])> {
    let (len, rest) = buf.split_first_chunk::<4>()?;
    let len = usize::try_from(u32::from_be_bytes(*len)).ok()?;
    if rest.len() < len {
        return None;
    }
    Some(rest.split_at(len))
}

/// The key type embedded at the start of a public key blob.
pub(crate) fn blob_key_type(blob: &[u8]) -> Option<&str> {
    let (ty, _) = read_string(blob)?;
    std::str::from_utf8(ty).ok().filter(|t| !t.is_empty())
}

pub(crate) fn put_string(buf: &mut Vec<u8>, data: &[u8]) {
    let len = u32::try_from(data.len()).unwrap_or(u32::MAX);
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(data);
}

/// Writes an unsigned big-endian integer as an SSH `mpint`.
pub(crate) fn put_mpint(buf: &mut Vec<u8>, be: &[u8]) {
    let start = be.iter().position(|b| *b != 0).unwrap_or(be.len());
    let digits = be.get(start..).unwrap_or_default();
    if digits.first().is_some_and(|b| b & 0x80 != 0) {
        let mut v = Vec::with_capacity(digits.len() + 1);
        v.push(0);
        v.extend_from_slice(digits);
        put_string(buf, &v);
    } else {
        put_string(buf, digits);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn fingerprint_matches_openssh() {
        // github.com's published ed25519 host key and fingerprint.
        let blob =
            decode_blob("AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl")
                .unwrap();
        assert_eq!(blob_key_type(&blob), Some("ssh-ed25519"));
        assert_eq!(
            sha256_fingerprint(&blob),
            "SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU"
        );
    }

    #[test]
    fn wire_roundtrip() {
        let mut buf = Vec::new();
        put_string(&mut buf, b"ssh-rsa");
        put_mpint(&mut buf, &[0x00, 0x01, 0x00, 0x01]);
        put_mpint(&mut buf, &[0x80]);
        put_mpint(&mut buf, &[0x00]);
        let (ty, rest) = read_string(&buf).unwrap();
        assert_eq!(ty, b"ssh-rsa");
        let (e, rest) = read_string(rest).unwrap();
        assert_eq!(e, [0x01, 0x00, 0x01]);
        let (n, rest) = read_string(rest).unwrap();
        assert_eq!(n, [0x00, 0x80]);
        let (z, rest) = read_string(rest).unwrap();
        assert!(z.is_empty() && rest.is_empty());
        assert!(read_string(&[0, 0, 0, 5, 1]).is_none());
        assert!(read_string(&[0, 0]).is_none());
    }
}
