// SPDX-License-Identifier: GPL-3.0-or-later
//! asciicast v2 data model.
//!
//! A `.cast` file is a JSON header line followed by one JSON array per event:
//! `[time_in_seconds, code, data]` with codes `o` (output), `i` (input), `r` (resize,
//! `"COLSxROWS"`) and `m` (marker).

use crate::LogError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The asciicast v2 header (first line of a `.cast` file).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CastHeader {
    /// Always 2.
    pub version: u8,
    pub width: u16,
    pub height: u16,
    /// Start time, unix seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_time_limit: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Usually `TERM` and `SHELL`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

impl CastHeader {
    /// A header for a `width`×`height` terminal started at `timestamp` (unix seconds).
    #[must_use]
    pub fn new(width: u16, height: u16, timestamp: Option<i64>) -> Self {
        Self {
            version: 2,
            width,
            height,
            timestamp,
            idle_time_limit: None,
            title: None,
            env: BTreeMap::new(),
        }
    }
}

/// Event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    /// Data written to the terminal (`o`).
    Output,
    /// Data typed by the user (`i`).
    Input,
    /// Terminal resize (`r`), data is `COLSxROWS`.
    Resize,
    /// A marker / bookmark (`m`), data is its label.
    Marker,
}

impl EventKind {
    /// The asciicast event code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Output => "o",
            Self::Input => "i",
            Self::Resize => "r",
            Self::Marker => "m",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        Some(match code {
            "o" => Self::Output,
            "i" => Self::Input,
            "r" => Self::Resize,
            "m" => Self::Marker,
            _ => return None,
        })
    }
}

/// One recorded event.
#[derive(Debug, Clone, PartialEq)]
pub struct CastEvent {
    /// Seconds since the start of the recording (microsecond resolution).
    pub time: f64,
    pub kind: EventKind,
    pub data: String,
}

impl CastEvent {
    /// Serializes as an asciicast line (without the trailing newline).
    pub fn to_line(&self) -> Result<String, LogError> {
        Ok(serde_json::to_string(&(
            self.time,
            self.kind.code(),
            &self.data,
        ))?)
    }

    /// Parses an asciicast event line.
    pub fn from_line(line: &str) -> Result<Self, LogError> {
        let (time, code, data): (f64, String, String) = serde_json::from_str(line)?;
        let kind = EventKind::from_code(&code)
            .ok_or_else(|| LogError::Corrupt(format!("unknown event code {code:?}")))?;
        if !time.is_finite() || time < 0.0 {
            return Err(LogError::Corrupt("invalid event time".into()));
        }
        Ok(Self { time, kind, data })
    }

    /// For resize events: the new `(cols, rows)`.
    #[must_use]
    pub fn resize_dimensions(&self) -> Option<(u16, u16)> {
        if self.kind != EventKind::Resize {
            return None;
        }
        let (c, r) = self.data.split_once('x')?;
        Some((c.parse().ok()?, r.parse().ok()?))
    }
}

/// Converts microseconds to asciicast seconds.
pub(crate) fn micros_to_secs(us: u64) -> f64 {
    us as f64 / 1_000_000.0
}

/// Incremental UTF-8 decoder: keeps incomplete trailing sequences for the next call and
/// replaces invalid bytes with U+FFFD.
#[derive(Debug, Default, Clone)]
pub(crate) struct Utf8Stream {
    pending: Vec<u8>,
}

impl Utf8Stream {
    pub(crate) fn decode(&mut self, data: &[u8]) -> String {
        let mut buf = std::mem::take(&mut self.pending);
        buf.extend_from_slice(data);
        let mut out = String::with_capacity(buf.len());
        let mut rest = buf.as_slice();
        loop {
            match std::str::from_utf8(rest) {
                Ok(s) => {
                    out.push_str(s);
                    break;
                }
                Err(e) => {
                    let (valid, after) = rest.split_at(e.valid_up_to());
                    out.push_str(std::str::from_utf8(valid).unwrap_or_default());
                    if let Some(n) = e.error_len() {
                        out.push('\u{FFFD}');
                        rest = &after[n..];
                    } else {
                        self.pending = after.to_vec();
                        break;
                    }
                }
            }
        }
        out
    }

    /// Flushes an incomplete trailing sequence as U+FFFD.
    pub(crate) fn finish(&mut self) -> String {
        if self.pending.is_empty() {
            String::new()
        } else {
            self.pending.clear();
            "\u{FFFD}".to_owned()
        }
    }

    pub(crate) fn reset(&mut self) {
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn event_lines_roundtrip() {
        let e = CastEvent {
            time: 1.234_567,
            kind: EventKind::Output,
            data: "a\"b\u{1b}[0m\n".into(),
        };
        let line = e.to_line().unwrap();
        assert_eq!(line, r#"[1.234567,"o","a\"b\u001b[0m\n"]"#);
        assert_eq!(CastEvent::from_line(&line).unwrap(), e);
        assert!(CastEvent::from_line(r#"[1,"x","a"]"#).is_err());
        assert!(CastEvent::from_line(r#"[-1,"o","a"]"#).is_err());
        let r = CastEvent {
            time: 0.0,
            kind: EventKind::Resize,
            data: "120x40".into(),
        };
        assert_eq!(r.resize_dimensions(), Some((120, 40)));
    }

    #[test]
    fn utf8_stream_handles_splits_and_garbage() {
        let mut s = Utf8Stream::default();
        let bytes = "aé€😀".as_bytes();
        let mut out = String::new();
        for b in bytes {
            out.push_str(&s.decode(std::slice::from_ref(b)));
        }
        assert_eq!(out, "aé€😀");
        assert_eq!(s.decode(b"x\xffy"), "x\u{fffd}y");
        assert_eq!(s.decode(b"\xe2\x82"), "");
        assert_eq!(s.finish(), "\u{fffd}");
    }

    #[test]
    fn header_json_shape() {
        let mut h = CastHeader::new(80, 24, Some(1_700_000_000));
        h.env.insert("TERM".into(), "xterm-256color".into());
        let json = serde_json::to_string(&h).unwrap();
        assert_eq!(
            json,
            r#"{"version":2,"width":80,"height":24,"timestamp":1700000000,"env":{"TERM":"xterm-256color"}}"#
        );
    }
}
