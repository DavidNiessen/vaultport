// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal SSH wire encoding ([RFC 4251 §5]) used by the key and signature formats in
//! this crate.
//!
//! The [`Reader`] is strict: every length is bounds-checked, strings may not exceed the
//! remaining input, and callers are expected to call [`Reader::finish`] so trailing garbage
//! is rejected. It never panics on untrusted input.
//!
//! [RFC 4251 §5]: https://www.rfc-editor.org/rfc/rfc4251#section-5

use crate::error::{Error, Result};

/// Appends SSH wire primitives to a byte buffer.
#[derive(Debug, Default, Clone)]
pub(crate) struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn u8(&mut self, value: u8) -> &mut Self {
        self.buf.push(value);
        self
    }

    pub(crate) fn u32(&mut self, value: u32) -> &mut Self {
        self.buf.extend_from_slice(&value.to_be_bytes());
        self
    }

    /// Raw bytes without a length prefix.
    pub(crate) fn raw(&mut self, bytes: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(bytes);
        self
    }

    /// A length-prefixed `string`.
    ///
    /// Inputs in this crate are far below 4 GiB. Should one ever exceed it, the length
    /// saturates instead of wrapping, so the result fails to decode rather than being
    /// silently misinterpreted.
    pub(crate) fn string(&mut self, bytes: &[u8]) -> &mut Self {
        let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        self.u32(len).raw(bytes)
    }

    /// An `mpint` from an unsigned big-endian magnitude (leading zeros are stripped and a
    /// zero byte is prepended when the most significant bit is set).
    pub(crate) fn mpint_unsigned(&mut self, magnitude: &[u8]) -> &mut Self {
        let first = magnitude.iter().position(|&b| b != 0);
        let Some(first) = first else {
            // Zero is encoded as the empty string.
            return self.u32(0);
        };
        let digits = &magnitude[first..];
        if digits[0] & 0x80 != 0 {
            let len = u32::try_from(digits.len() + 1).unwrap_or(u32::MAX);
            self.u32(len).u8(0).raw(digits)
        } else {
            self.string(digits)
        }
    }

    pub(crate) fn into_vec(self) -> Vec<u8> {
        self.buf
    }

    pub(crate) fn len(&self) -> usize {
        self.buf.len()
    }
}

/// Reads SSH wire primitives from a byte slice.
#[derive(Debug, Clone)]
pub(crate) struct Reader<'a> {
    buf: &'a [u8],
    what: &'static str,
}

impl<'a> Reader<'a> {
    /// `what` names the structure being decoded for error messages.
    pub(crate) fn new(buf: &'a [u8], what: &'static str) -> Self {
        Self { buf, what }
    }

    fn malformed(&self, reason: &'static str) -> Error {
        Error::Malformed {
            what: self.what,
            reason,
        }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.buf.len()
    }

    pub(crate) fn raw(&mut self, len: usize) -> Result<&'a [u8]> {
        if self.buf.len() < len {
            return Err(self.malformed("truncated input"));
        }
        let (head, tail) = self.buf.split_at(len);
        self.buf = tail;
        Ok(head)
    }

    pub(crate) fn u8(&mut self) -> Result<u8> {
        Ok(self.raw(1)?[0])
    }

    pub(crate) fn u32(&mut self) -> Result<u32> {
        let bytes = self.raw(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub(crate) fn string(&mut self) -> Result<&'a [u8]> {
        let len = self.u32()?;
        let len = usize::try_from(len).map_err(|_| self.malformed("length overflow"))?;
        self.raw(len)
    }

    pub(crate) fn utf8(&mut self) -> Result<&'a str> {
        let bytes = self.string()?;
        std::str::from_utf8(bytes).map_err(|_| self.malformed("string is not UTF-8"))
    }

    /// Reads a string and requires it to equal `expected`.
    pub(crate) fn expect_string(&mut self, expected: &[u8], reason: &'static str) -> Result<()> {
        if self.string()? == expected {
            Ok(())
        } else {
            Err(self.malformed(reason))
        }
    }

    /// Reads a non-negative `mpint` and returns its magnitude without leading zeros.
    ///
    /// Negative values are rejected. A redundant leading zero byte is tolerated (as OpenSSH
    /// does), but the magnitude may be at most `max_len` bytes.
    pub(crate) fn mpint_unsigned(&mut self, max_len: usize) -> Result<&'a [u8]> {
        let bytes = self.string()?;
        if bytes.first().is_some_and(|b| b & 0x80 != 0) {
            return Err(self.malformed("negative mpint"));
        }
        let first = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len());
        let magnitude = &bytes[first..];
        if magnitude.len() > max_len {
            return Err(self.malformed("mpint too large"));
        }
        Ok(magnitude)
    }

    /// Fails unless the whole input has been consumed.
    pub(crate) fn finish(&self) -> Result<()> {
        if self.buf.is_empty() {
            Ok(())
        } else {
            Err(self.malformed("trailing data"))
        }
    }
}

/// Left-pads a big-endian magnitude to exactly `N` bytes.
pub(crate) fn left_pad<const N: usize>(magnitude: &[u8]) -> Option<[u8; N]> {
    if magnitude.len() > N {
        return None;
    }
    let mut out = [0u8; N];
    out[N - magnitude.len()..].copy_from_slice(magnitude);
    Some(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn mpint_encoding_matches_rfc4251_examples() {
        // RFC 4251 §5 examples.
        let mut w = Writer::new();
        w.mpint_unsigned(&[]);
        assert_eq!(w.into_vec(), [0, 0, 0, 0]);

        let mut w = Writer::new();
        w.mpint_unsigned(&[0x00, 0x00, 0x80]);
        assert_eq!(w.into_vec(), [0, 0, 0, 2, 0x00, 0x80]);

        let mut w = Writer::new();
        w.mpint_unsigned(&[0x09, 0xa3, 0x78, 0xf9, 0xb2, 0xe3, 0x32, 0xa7]);
        assert_eq!(
            w.into_vec(),
            [0, 0, 0, 8, 0x09, 0xa3, 0x78, 0xf9, 0xb2, 0xe3, 0x32, 0xa7]
        );
    }

    #[test]
    fn mpint_roundtrip_and_rejections() {
        let mut w = Writer::new();
        w.mpint_unsigned(&[0xff, 0x01]);
        let bytes = w.into_vec();
        let mut r = Reader::new(&bytes, "test");
        assert_eq!(r.mpint_unsigned(32).unwrap(), &[0xff, 0x01]);
        r.finish().unwrap();

        // Negative.
        let mut r = Reader::new(&[0, 0, 0, 1, 0x80], "test");
        assert!(r.mpint_unsigned(32).is_err());
        // Too large.
        let mut r = Reader::new(&[0, 0, 0, 2, 0x01, 0x02], "test");
        assert!(r.mpint_unsigned(1).is_err());
        // Redundant leading zero is tolerated.
        let mut r = Reader::new(&[0, 0, 0, 2, 0x00, 0x01], "test");
        assert_eq!(r.mpint_unsigned(1).unwrap(), &[0x01]);
    }

    #[test]
    fn reader_rejects_truncation_and_trailing_data() {
        let mut r = Reader::new(&[0, 0, 0, 5, 1, 2], "test");
        assert!(r.string().is_err());
        let mut r = Reader::new(&[0xff, 0xff, 0xff, 0xff], "test");
        assert!(r.string().is_err());
        let mut r = Reader::new(&[0, 0, 0, 0, 9], "test");
        assert_eq!(r.string().unwrap(), b"");
        assert!(r.finish().is_err());
        let mut r = Reader::new(&[0, 0, 0, 1, 0xff], "test");
        assert!(r.utf8().is_err());
    }

    #[test]
    fn left_pad_works() {
        assert_eq!(left_pad::<4>(&[1, 2]), Some([0, 0, 1, 2]));
        assert_eq!(left_pad::<1>(&[1, 2]), None);
    }
}
