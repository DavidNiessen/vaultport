// SPDX-License-Identifier: GPL-3.0-or-later
//! A tiny, incremental VT/ANSI escape sequence scanner.
//!
//! [`VtScanner`] splits a terminal output byte stream into printable text, C0 control bytes
//! and complete OSC payloads, and silently drops everything else (CSI, DCS, APC, PM, SOS and
//! two-byte escapes). It is a state machine, so input may be split at arbitrary byte
//! boundaries. Memory is bounded: OSC payloads longer than [`MAX_OSC_LEN`] are discarded.
//!
//! The stream is assumed to be UTF-8, so 8-bit C1 controls are not interpreted (they collide
//! with UTF-8 continuation bytes).
//!
//! On top of the scanner, [`AnsiStripper`] reconstructs plain text lines, which is used for
//! password prompt detection and session log search.

/// Maximum accepted OSC payload length in bytes. Longer payloads are dropped.
pub const MAX_OSC_LEN: usize = 8 * 1024;

/// Default maximum length of a reconstructed text line in bytes.
pub const DEFAULT_MAX_LINE_LEN: usize = 4 * 1024;

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;
const DEL: u8 = 0x7f;

/// An item recognized by [`VtScanner`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VtEvent<'a> {
    /// A run of printable bytes (UTF-8 text, possibly split mid-character at chunk edges).
    Print(&'a [u8]),
    /// A C0 control byte such as `\n`, `\r`, `\t`, backspace or BEL.
    Control(u8),
    /// The payload of a complete OSC sequence (between `ESC ]` and BEL / ST).
    Osc(&'a [u8]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Ground,
    Escape,
    EscIntermediate,
    Csi,
    Osc,
    OscEsc,
    /// DCS, SOS, PM, APC: ignored until ST.
    Str,
    StrEsc,
}

/// Incremental escape sequence scanner. See the [module docs](self).
#[derive(Debug, Clone)]
pub struct VtScanner {
    state: State,
    osc: Vec<u8>,
    osc_overflow: bool,
}

impl Default for VtScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl VtScanner {
    /// Creates a scanner in the ground state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: State::Ground,
            osc: Vec::new(),
            osc_overflow: false,
        }
    }

    /// Feeds a chunk of output, calling `f` for every recognized item, in order.
    pub fn feed(&mut self, data: &[u8], mut f: impl FnMut(VtEvent<'_>)) {
        let mut i = 0;
        while i < data.len() {
            let b = data[i];
            match self.state {
                State::Ground => {
                    if is_printable(b) {
                        let start = i;
                        while i < data.len() && is_printable(data[i]) {
                            i += 1;
                        }
                        f(VtEvent::Print(&data[start..i]));
                        continue;
                    }
                    if b == ESC {
                        self.state = State::Escape;
                    } else if b != DEL {
                        f(VtEvent::Control(b));
                    }
                }
                State::Escape => match b {
                    b']' => {
                        self.osc.clear();
                        self.osc_overflow = false;
                        self.state = State::Osc;
                    }
                    b'[' => self.state = State::Csi,
                    b'P' | b'X' | b'^' | b'_' => self.state = State::Str,
                    ESC => {}
                    CAN | SUB => self.state = State::Ground,
                    0x20..=0x2f => self.state = State::EscIntermediate,
                    _ => self.state = State::Ground,
                },
                State::EscIntermediate => match b {
                    0x20..=0x2f => {}
                    ESC => self.state = State::Escape,
                    _ => self.state = State::Ground,
                },
                State::Csi => match b {
                    0x40..=0x7e | CAN | SUB => self.state = State::Ground,
                    ESC => self.state = State::Escape,
                    _ => {}
                },
                State::Osc => match b {
                    BEL => {
                        self.finish_osc(&mut f);
                    }
                    ESC => self.state = State::OscEsc,
                    CAN | SUB => self.state = State::Ground,
                    _ => {
                        if self.osc.len() < MAX_OSC_LEN {
                            self.osc.push(b);
                        } else {
                            self.osc_overflow = true;
                        }
                    }
                },
                State::OscEsc => {
                    if b == b'\\' {
                        self.finish_osc(&mut f);
                    } else {
                        // ESC not followed by '\' aborts the OSC and starts a new escape.
                        self.state = State::Escape;
                        continue;
                    }
                }
                State::Str => match b {
                    ESC => self.state = State::StrEsc,
                    CAN | SUB => self.state = State::Ground,
                    _ => {}
                },
                State::StrEsc => {
                    if b == b'\\' {
                        self.state = State::Ground;
                    } else {
                        self.state = State::Escape;
                        continue;
                    }
                }
            }
            i += 1;
        }
    }

    fn finish_osc(&mut self, f: &mut impl FnMut(VtEvent<'_>)) {
        self.state = State::Ground;
        if !self.osc_overflow {
            f(VtEvent::Osc(&self.osc));
        }
        self.osc.clear();
        self.osc_overflow = false;
    }
}

fn is_printable(b: u8) -> bool {
    b >= 0x20 && b != DEL
}

/// Plain-text line reconstruction with carriage return / backspace handling.
///
/// A lone `\r` (not followed by `\n`) is treated as "overwrite the line", which approximates
/// progress bars and spinners well. Backspace removes the last character.
#[derive(Debug, Clone)]
pub struct LineBuf {
    bytes: Vec<u8>,
    pending_cr: bool,
    max: usize,
}

impl Default for LineBuf {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_LINE_LEN)
    }
}

impl LineBuf {
    /// Creates an empty line buffer that holds at most `max` bytes (minimum 16).
    #[must_use]
    pub fn new(max: usize) -> Self {
        Self {
            bytes: Vec::new(),
            pending_cr: false,
            max: max.max(16),
        }
    }

    /// The current, incomplete line (lossily decoded).
    #[must_use]
    pub fn current(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }

    /// The raw bytes of the current line.
    #[must_use]
    pub fn current_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Whether the current line holds no text.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    fn resolve_cr(&mut self) {
        if self.pending_cr {
            self.pending_cr = false;
            self.bytes.clear();
        }
    }

    /// Appends printable bytes. When the line exceeds the limit, the full part is passed to
    /// `on_line` (forced wrap at a UTF-8 boundary).
    pub fn print(&mut self, text: &[u8], on_line: &mut impl FnMut(&str)) {
        self.resolve_cr();
        let mut rest = text;
        while !rest.is_empty() {
            let room = self.max.saturating_sub(self.bytes.len());
            if rest.len() <= room {
                self.bytes.extend_from_slice(rest);
                return;
            }
            self.bytes.extend_from_slice(&rest[..room]);
            rest = &rest[room..];
            // Keep an incomplete trailing UTF-8 sequence for the next line.
            let cut = utf8_boundary(&self.bytes);
            let tail = self.bytes.split_off(cut);
            on_line(&String::from_utf8_lossy(&self.bytes));
            self.bytes = tail;
        }
    }

    /// Handles a control byte. Returns after calling `on_line` if it completed a line.
    pub fn control(&mut self, b: u8, on_line: &mut impl FnMut(&str)) {
        match b {
            b'\n' => {
                self.pending_cr = false;
                on_line(&String::from_utf8_lossy(&self.bytes));
                self.bytes.clear();
            }
            b'\r' => self.pending_cr = true,
            0x08 => {
                self.resolve_cr();
                while let Some(last) = self.bytes.pop() {
                    if last & 0xc0 != 0x80 {
                        break;
                    }
                }
            }
            b'\t' => self.print(b"\t", on_line),
            _ => {}
        }
    }

    /// Discards the current line.
    pub fn clear(&mut self) {
        self.bytes.clear();
        self.pending_cr = false;
    }
}

/// Largest index `<= bytes.len()` that does not split a UTF-8 sequence at the end.
fn utf8_boundary(bytes: &[u8]) -> usize {
    let len = bytes.len();
    for back in 1..=3.min(len) {
        let b = bytes[len - back];
        if b & 0xc0 == 0x80 {
            continue;
        }
        let need = if b >= 0xf0 {
            4
        } else if b >= 0xe0 {
            3
        } else if b >= 0xc0 {
            2
        } else {
            1
        };
        return if need > back { len - back } else { len };
    }
    len
}

/// Incremental ANSI/VT stripper producing plain text lines.
#[derive(Debug, Clone, Default)]
pub struct AnsiStripper {
    scanner: VtScanner,
    line: LineBuf,
}

impl AnsiStripper {
    /// Creates a stripper with the default maximum line length.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a stripper with a custom maximum line length.
    #[must_use]
    pub fn with_max_line_len(max: usize) -> Self {
        Self {
            scanner: VtScanner::new(),
            line: LineBuf::new(max),
        }
    }

    /// Feeds output bytes; `on_line` receives each completed line without escape sequences.
    pub fn feed(&mut self, data: &[u8], mut on_line: impl FnMut(&str)) {
        let line = &mut self.line;
        self.scanner.feed(data, |ev| match ev {
            VtEvent::Print(t) => line.print(t, &mut on_line),
            VtEvent::Control(c) => line.control(c, &mut on_line),
            VtEvent::Osc(_) => {}
        });
    }

    /// The current incomplete line.
    #[must_use]
    pub fn current_line(&self) -> String {
        self.line.current()
    }

    /// Whether the current incomplete line is empty.
    #[must_use]
    pub fn current_line_is_empty(&self) -> bool {
        self.line.is_empty()
    }
}

/// Strips escape sequences from a complete buffer, returning the text lines joined by `\n`.
#[must_use]
pub fn strip_ansi(data: &[u8]) -> String {
    let mut out = String::new();
    let mut s = AnsiStripper::new();
    s.feed(data, |l| {
        out.push_str(l);
        out.push('\n');
    });
    out.push_str(&s.current_line());
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use proptest::prelude::*;

    fn collect(chunks: &[&[u8]]) -> Vec<String> {
        let mut sc = VtScanner::new();
        let mut out = Vec::new();
        let mut text: Vec<u8> = Vec::new();
        let flush = |text: &mut Vec<u8>, out: &mut Vec<String>| {
            if !text.is_empty() {
                out.push(format!("T:{}", String::from_utf8_lossy(text)));
                text.clear();
            }
        };
        for c in chunks {
            sc.feed(c, |ev| match ev {
                VtEvent::Print(t) => text.extend_from_slice(t),
                VtEvent::Control(b) => {
                    flush(&mut text, &mut out);
                    out.push(format!("C:{b}"));
                }
                VtEvent::Osc(p) => {
                    flush(&mut text, &mut out);
                    out.push(format!("O:{}", String::from_utf8_lossy(p)));
                }
            });
        }
        flush(&mut text, &mut out);
        out
    }

    #[test]
    fn strips_csi_and_reports_osc() {
        let ev = collect(&[b"\x1b[1;31mred\x1b[0m\x1b]0;title\x07x\x1b]7;file://h/p\x1b\\\n"]);
        assert_eq!(ev, ["T:red", "O:0;title", "T:x", "O:7;file://h/p", "C:10"]);
    }

    #[test]
    fn split_sequences_are_reassembled() {
        let ev = collect(&[b"a\x1b", b"]133", b";A\x1b", b"\\b\x1b[", b"2", b"Kc"]);
        assert_eq!(ev, ["T:a", "O:133;A", "T:bc"]);
    }

    #[test]
    fn dcs_and_charset_escapes_are_ignored() {
        let ev = collect(&[b"\x1bP1$r0m\x1b\\a\x1b(Bb\x1b7c\x1b_apc\x1b\\d"]);
        assert_eq!(ev, ["T:abcd"]);
    }

    #[test]
    fn oversized_osc_is_dropped() {
        let mut data = b"\x1b]0;".to_vec();
        data.extend(std::iter::repeat_n(b'x', MAX_OSC_LEN + 10));
        data.extend_from_slice(b"\x07ok");
        assert_eq!(collect(&[&data]), ["T:ok"]);
    }

    #[test]
    fn stripper_handles_cr_and_backspace() {
        assert_eq!(strip_ansi(b"10%\r50%\r100%\r\ndone"), "100%\ndone");
        assert_eq!(strip_ansi(b"abx\x08c\r\n"), "abc\n");
        assert_eq!(strip_ansi("h\u{e9}\x08\x08i".as_bytes()), "i");
    }

    #[test]
    fn long_lines_wrap_on_char_boundaries() {
        let mut s = AnsiStripper::with_max_line_len(16);
        let mut lines = Vec::new();
        s.feed("ééééééééééé".as_bytes(), |l| {
            lines.push(l.to_owned());
        });
        assert_eq!(lines, ["éééééééé"]);
        assert_eq!(s.current_line(), "ééé");
    }

    proptest! {
        #[test]
        fn chunking_does_not_change_output(data in proptest::collection::vec(any::<u8>(), 0..400), cut in 0usize..400) {
            let cut = cut.min(data.len());
            let whole = collect(&[&data]);
            let split = collect(&[&data[..cut], &data[cut..]]);
            prop_assert_eq!(whole, split);
        }
    }
}
