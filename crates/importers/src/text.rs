// SPDX-License-Identifier: GPL-3.0-or-later
//! Input size limits and text decoding.

use crate::error::ImportError;
use std::borrow::Cow;

/// Maximum accepted input size (16 MiB). Applies to the raw bytes and, for `ssh_config`, to
/// the main file plus everything pulled in through `Include`.
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

pub(crate) fn check_size(len: usize) -> Result<(), ImportError> {
    if len > MAX_INPUT_BYTES {
        Err(ImportError::TooLarge {
            size: len,
            max: MAX_INPUT_BYTES,
        })
    } else {
        Ok(())
    }
}

/// Result of decoding raw bytes into text.
#[derive(Debug)]
pub(crate) struct Decoded<'a> {
    pub text: Cow<'a, str>,
    /// `true` when invalid UTF-8 sequences were replaced (legacy ANSI code page exports).
    pub lossy: bool,
}

/// Decodes `bytes` as UTF-16 (LE/BE, with BOM) or UTF-8 (BOM optional). Invalid UTF-8 is
/// decoded lossily (Windows tools often write the ANSI code page); invalid UTF-16 is an
/// error.
pub(crate) fn decode_text(bytes: &[u8]) -> Result<Decoded<'_>, ImportError> {
    check_size(bytes.len())?;
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16(rest, u16::from_le_bytes);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16(rest, u16::from_be_bytes);
    }
    let rest = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(rest) {
        Ok(s) => Ok(Decoded {
            text: Cow::Borrowed(s),
            lossy: false,
        }),
        Err(_) => Ok(Decoded {
            text: Cow::Owned(String::from_utf8_lossy(rest).into_owned()),
            lossy: true,
        }),
    }
}

fn decode_utf16(bytes: &[u8], conv: fn([u8; 2]) -> u16) -> Result<Decoded<'static>, ImportError> {
    if !bytes.len().is_multiple_of(2) {
        return Err(ImportError::Encoding(
            "odd number of bytes in UTF-16 input".into(),
        ));
    }
    let units = bytes.chunks_exact(2).map(|c| conv([c[0], c[1]]));
    let text: Result<String, _> = char::decode_utf16(units).collect();
    text.map(|t| Decoded {
        text: Cow::Owned(t),
        lossy: false,
    })
    .map_err(|e| ImportError::Encoding(format!("invalid UTF-16: {e}")))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn utf16le_with_bom() {
        let mut bytes = vec![0xFF, 0xFE];
        for u in "Hi ✓".encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        let d = decode_text(&bytes).unwrap();
        assert_eq!(d.text, "Hi ✓");
        assert!(!d.lossy);
    }

    #[test]
    fn utf16be_and_utf8_bom() {
        let mut bytes = vec![0xFE, 0xFF];
        for u in "ab".encode_utf16() {
            bytes.extend_from_slice(&u.to_be_bytes());
        }
        assert_eq!(decode_text(&bytes).unwrap().text, "ab");
        assert_eq!(decode_text(b"\xEF\xBB\xBFx=1").unwrap().text, "x=1");
    }

    #[test]
    fn invalid_inputs() {
        assert!(decode_text(&[0xFF, 0xFE, 0x41]).is_err());
        assert!(decode_text(&[0xFF, 0xFE, 0x00, 0xD8]).is_err());
        let d = decode_text(b"caf\xE9").unwrap();
        assert!(d.lossy);
        assert!(d.text.starts_with("caf"));
    }

    #[test]
    fn size_limit() {
        assert!(check_size(MAX_INPUT_BYTES).is_ok());
        assert_eq!(
            check_size(MAX_INPUT_BYTES + 1),
            Err(ImportError::TooLarge {
                size: MAX_INPUT_BYTES + 1,
                max: MAX_INPUT_BYTES
            })
        );
    }
}
