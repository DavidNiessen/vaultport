// SPDX-License-Identifier: GPL-3.0-or-later
//! The 128-bit session key printed by `mosh-server` (`MOSH_KEY`).

use std::fmt;
use std::str::FromStr;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::Error;

/// Length of the printable key: base64 of 16 bytes with the `==` padding stripped.
pub const PRINTABLE_KEY_LEN: usize = 22;

/// A Mosh session key (AES-128). The bytes are wiped from memory on drop and never printed.
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct MoshKey([u8; 16]);

impl MoshKey {
    /// Parses the 22-character base64 form used by `MOSH_KEY` and `MOSH CONNECT`.
    ///
    /// Non-canonical encodings (non-zero trailing bits) are rejected, like `mosh-client`.
    pub fn parse(printable: &str) -> Result<Self, Error> {
        if printable.len() != PRINTABLE_KEY_LEN {
            return Err(Error::InvalidKey);
        }
        let mut out = [0u8; 16];
        let mut decoded = zeroize::Zeroizing::new([0u8; 18]);
        let n = STANDARD_NO_PAD
            .decode_slice(printable.as_bytes(), &mut decoded[..])
            .map_err(|_| Error::InvalidKey)?;
        if n != 16 {
            return Err(Error::InvalidKey);
        }
        out.copy_from_slice(&decoded[..16]);
        Ok(Self(out))
    }

    /// Builds a key from raw bytes.
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Raw key bytes. Handle with care.
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// The 22-character printable form (as passed in `MOSH_KEY`).
    pub fn to_printable(&self) -> zeroize::Zeroizing<String> {
        zeroize::Zeroizing::new(STANDARD_NO_PAD.encode(self.0))
    }
}

impl FromStr for MoshKey {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Debug for MoshKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MoshKey(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn parses_and_round_trips() {
        let k = MoshKey::parse("AAECAwQFBgcICQoLDA0ODw").unwrap();
        assert_eq!(
            k.as_bytes(),
            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
        );
        assert_eq!(k.to_printable().as_str(), "AAECAwQFBgcICQoLDA0ODw");
    }

    #[test]
    fn rejects_bad_keys() {
        for bad in [
            "",
            "AAECAwQFBgcICQoLDA0ODw==",
            "AAECAwQFBgcICQoLDA0OD",
            "AAECAwQFBgcICQoLDA0OD!",
            // non-canonical: trailing bits set
            "AAECAwQFBgcICQoLDA0ODx",
        ] {
            assert!(MoshKey::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn debug_is_redacted() {
        let k = MoshKey::from_bytes([7; 16]);
        assert_eq!(format!("{k:?}"), "MoshKey(<redacted>)");
    }
}
