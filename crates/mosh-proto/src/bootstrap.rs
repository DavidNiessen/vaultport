// SPDX-License-Identifier: GPL-3.0-or-later
//! Parsing of `mosh-server new` output.
//!
//! The app runs `mosh-server new -s -c 256 -l LANG=en_US.UTF-8` over an SSH exec channel; the
//! server prints `MOSH CONNECT <port> <key>` on stdout, then detaches.

use crate::key::{MoshKey, PRINTABLE_KEY_LEN};

/// Parses one `MOSH CONNECT <port> <key>` line (surrounding whitespace and `\r` allowed).
pub fn parse_mosh_connect(line: &str) -> Option<(u16, MoshKey)> {
    let mut parts = line.split_ascii_whitespace();
    if parts.next()? != "MOSH" || parts.next()? != "CONNECT" {
        return None;
    }
    let port = parts.next()?;
    let key = parts.next()?;
    if parts.next().is_some() || !port.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let port: u16 = port.parse().ok().filter(|p| *p != 0)?;
    if key.len() != PRINTABLE_KEY_LEN {
        return None;
    }
    Some((port, MoshKey::parse(key).ok()?))
}

/// Finds the `MOSH CONNECT` line anywhere in the captured output of `mosh-server new`.
pub fn find_mosh_connect(output: &str) -> Option<(u16, MoshKey)> {
    output.lines().find_map(parse_mosh_connect)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn parses_connect_line() {
        let (port, key) =
            parse_mosh_connect("MOSH CONNECT 60001 AAECAwQFBgcICQoLDA0ODw\r\n").unwrap();
        assert_eq!(port, 60001);
        assert_eq!(key.as_bytes()[15], 15);
        assert!(parse_mosh_connect("  MOSH CONNECT 60001 AAECAwQFBgcICQoLDA0ODw").is_some());
    }

    #[test]
    fn rejects_malformed_lines() {
        for bad in [
            "",
            "MOSH CONNECT",
            "MOSH CONNECT 60001",
            "MOSH CONNECT 0 AAECAwQFBgcICQoLDA0ODw",
            "MOSH CONNECT 70000 AAECAwQFBgcICQoLDA0ODw",
            "MOSH CONNECT +6000 AAECAwQFBgcICQoLDA0ODw",
            "MOSH CONNECT 60001 AAECAwQFBgcICQoLDA0OD",
            "MOSH CONNECT 60001 AAECAwQFBgcICQoLDA0ODw extra",
            "mosh connect 60001 AAECAwQFBgcICQoLDA0ODw",
            "XMOSH CONNECT 60001 AAECAwQFBgcICQoLDA0ODw",
        ] {
            assert!(parse_mosh_connect(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn finds_line_in_output() {
        let out = "\r\nmosh-server (mosh 1.4.0) [build mosh 1.4.0]\r\n\
                   MOSH CONNECT 60004 AAECAwQFBgcICQoLDA0ODw\r\n\r\n";
        assert_eq!(find_mosh_connect(out).unwrap().0, 60004);
        assert!(find_mosh_connect("no luck\n").is_none());
    }
}
