// SPDX-License-Identifier: GPL-3.0-or-later
//! Telnet command bytes (RFC 854) and option codes.

use std::fmt;

/// Interpret As Command.
pub const IAC: u8 = 255;
/// Refuse / stop: "please don't".
pub const DONT: u8 = 254;
/// Request / confirm: "please do".
pub const DO: u8 = 253;
/// Refuse / stop: "I won't".
pub const WONT: u8 = 252;
/// Offer / confirm: "I will".
pub const WILL: u8 = 251;
/// Subnegotiation begin.
pub const SB: u8 = 250;
/// Go ahead.
pub const GA: u8 = 249;
/// Erase line.
pub const EL: u8 = 248;
/// Erase character.
pub const EC: u8 = 247;
/// Are you there.
pub const AYT: u8 = 246;
/// Abort output.
pub const AO: u8 = 245;
/// Interrupt process.
pub const IP: u8 = 244;
/// Break.
pub const BRK: u8 = 243;
/// Data mark (synch).
pub const DM: u8 = 242;
/// No operation.
pub const NOP: u8 = 241;
/// Subnegotiation end.
pub const SE: u8 = 240;

/// Carriage return.
pub const CR: u8 = b'\r';
/// Line feed.
pub const LF: u8 = b'\n';
/// NUL.
pub const NUL: u8 = 0;

/// A Telnet option code.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TelnetOption(pub u8);

impl TelnetOption {
    /// RFC 856.
    pub const BINARY: Self = Self(0);
    /// RFC 857.
    pub const ECHO: Self = Self(1);
    /// RFC 858.
    pub const SUPPRESS_GO_AHEAD: Self = Self(3);
    /// RFC 859.
    pub const STATUS: Self = Self(5);
    /// RFC 860.
    pub const TIMING_MARK: Self = Self(6);
    /// RFC 1091.
    pub const TERMINAL_TYPE: Self = Self(24);
    /// RFC 1073.
    pub const NAWS: Self = Self(31);
    /// RFC 1079.
    pub const TERMINAL_SPEED: Self = Self(32);
    /// RFC 1184.
    pub const LINEMODE: Self = Self(34);
    /// RFC 1408 (obsolete ENVIRON).
    pub const OLD_ENVIRON: Self = Self(36);
    /// RFC 1572.
    pub const NEW_ENVIRON: Self = Self(39);

    /// Human-readable name, if known.
    pub fn name(self) -> Option<&'static str> {
        Some(match self.0 {
            0 => "BINARY",
            1 => "ECHO",
            3 => "SUPPRESS-GO-AHEAD",
            5 => "STATUS",
            6 => "TIMING-MARK",
            24 => "TERMINAL-TYPE",
            31 => "NAWS",
            32 => "TERMINAL-SPEED",
            34 => "LINEMODE",
            36 => "ENVIRON",
            39 => "NEW-ENVIRON",
            _ => return None,
        })
    }
}

impl fmt::Display for TelnetOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(n) => f.write_str(n),
            None => write!(f, "OPTION-{}", self.0),
        }
    }
}

impl fmt::Debug for TelnetOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// TERMINAL-TYPE / NEW-ENVIRON subnegotiation verbs.
pub(crate) mod sub {
    pub const IS: u8 = 0;
    pub const SEND: u8 = 1;
    // NEW-ENVIRON type bytes.
    pub const VAR: u8 = 0;
    pub const VALUE: u8 = 1;
    pub const ESC: u8 = 2;
    pub const USERVAR: u8 = 3;
}
