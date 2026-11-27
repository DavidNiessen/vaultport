// SPDX-License-Identifier: GPL-3.0-or-later
//! Format detection and the format-independent entry points.

use crate::bundle::{ImportBundle, WarningKind};
use crate::error::ImportError;
use crate::text::decode_text;
use crate::{csv, known_hosts, mobaxterm, putty, securecrt, ssh_config};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A supported source format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum Format {
    /// OpenSSH client configuration (`~/.ssh/config`).
    OpenSshConfig,
    /// OpenSSH `known_hosts`.
    KnownHosts,
    /// CSV with a header row ([`csv::TEMPLATE_HEADER`]).
    Csv,
    /// PuTTY registry export (`.reg`).
    PuttyReg,
    /// MobaXterm `.mobaconf` / `.mxtsession`.
    MobaXterm,
    /// SecureCRT XML export.
    SecureCrtXml,
}

impl Format {
    /// All formats, in detection order.
    pub const ALL: [Self; 6] = [
        Self::PuttyReg,
        Self::SecureCrtXml,
        Self::MobaXterm,
        Self::KnownHosts,
        Self::Csv,
        Self::OpenSshConfig,
    ];

    /// Human readable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::OpenSshConfig => "OpenSSH config",
            Self::KnownHosts => "OpenSSH known_hosts",
            Self::Csv => "CSV",
            Self::PuttyReg => "PuTTY registry export",
            Self::MobaXterm => "MobaXterm sessions",
            Self::SecureCrtXml => "SecureCRT XML export",
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// `ssh_config` keywords that commonly start a file.
const SSH_CONFIG_KEYWORDS: &[&str] = &[
    "host",
    "match",
    "include",
    "hostname",
    "user",
    "port",
    "identityfile",
    "identitiesonly",
    "proxyjump",
    "proxycommand",
    "forwardagent",
    "serveraliveinterval",
    "serveralivecountmax",
    "stricthostkeychecking",
    "userknownhostsfile",
    "addkeystoagent",
    "usekeychain",
    "controlmaster",
    "controlpath",
    "controlpersist",
    "compression",
    "connecttimeout",
    "sendenv",
    "setenv",
    "hashknownhosts",
    "canonicalizehostname",
    "pubkeyauthentication",
    "passwordauthentication",
    "localforward",
    "remoteforward",
    "dynamicforward",
    "loglevel",
    "visualhostkey",
    "ciphers",
    "macs",
    "kexalgorithms",
    "hostkeyalgorithms",
];

/// How much of the input format sniffing looks at.
const SNIFF_BYTES: usize = 64 * 1024;

/// Guesses the format of `input`. Returns `None` if it matches none of the formats.
#[must_use]
pub fn detect_format(input: &[u8]) -> Option<Format> {
    let head = input.get(..SNIFF_BYTES.min(input.len()))?;
    // Cutting the head may split a UTF-16 code unit or a UTF-8 sequence.
    let head = if head.len() % 2 == 1
        && (head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]))
    {
        head.get(..head.len() - 1)?
    } else {
        head
    };
    let decoded = decode_text(head).ok()?;
    let text = decoded.text.trim_start_matches('\u{feff}').trim_start();
    if text.starts_with("Windows Registry Editor") || text.starts_with("REGEDIT4") {
        return Some(Format::PuttyReg);
    }
    if text.starts_with('<') && text.contains("<VanDyke") {
        return Some(Format::SecureCrtXml);
    }
    let meaningful = || {
        text.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with(';'))
    };
    if meaningful().any(|l| l.to_ascii_lowercase().starts_with("[bookmarks")) {
        return Some(Format::MobaXterm);
    }
    let sample: Vec<&str> = meaningful().take(5).collect();
    let first = sample.first()?;
    if sample.iter().all(|l| known_hosts::parse_line(l).is_ok()) {
        return Some(Format::KnownHosts);
    }
    if first.contains(',')
        && first.split(',').any(|c| {
            matches!(
                c.trim().trim_matches('"').to_ascii_lowercase().as_str(),
                "address" | "host" | "hostname"
            )
        })
    {
        return Some(Format::Csv);
    }
    if let Ok(Some((kw, _))) = ssh_config::split_line(first)
        && SSH_CONFIG_KEYWORDS.contains(&kw.as_str())
    {
        return Some(Format::OpenSshConfig);
    }
    None
}

/// Imports `input` (raw file contents) as `format`.
///
/// The input may be UTF-8 (with or without BOM) or UTF-16 with BOM; invalid UTF-8 is decoded
/// lossily with a warning. OpenSSH `Include` directives are not followed here; use
/// [`ssh_config::parse_with_includes`] for that.
pub fn import(format: Format, input: &[u8]) -> Result<ImportBundle, ImportError> {
    let decoded = decode_text(input)?;
    let text = decoded.text.trim_start_matches('\u{feff}');
    let mut bundle = match format {
        Format::OpenSshConfig => ssh_config::parse(text),
        Format::KnownHosts => known_hosts::parse(text),
        Format::Csv => csv::parse(text),
        Format::PuttyReg => putty::parse(text),
        Format::MobaXterm => mobaxterm::parse(text),
        Format::SecureCrtXml => securecrt::parse(text),
    }?;
    if decoded.lossy {
        bundle.warnings.insert(
            0,
            crate::ImportWarning {
                kind: WarningKind::Invalid,
                line: None,
                item: None,
                message: "input is not valid UTF-8; unreadable characters were replaced".into(),
            },
        );
    }
    Ok(bundle)
}

/// Detects the format of `input` and imports it.
pub fn import_auto(input: &[u8]) -> Result<(Format, ImportBundle), ImportError> {
    let format = detect_format(input).ok_or(ImportError::UnknownFormat)?;
    import(format, input).map(|b| (format, b))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn utf16le(s: &str) -> Vec<u8> {
        let mut v = vec![0xFF, 0xFE];
        for u in s.encode_utf16() {
            v.extend_from_slice(&u.to_le_bytes());
        }
        v
    }

    #[test]
    fn detects_every_format() {
        let cases: [(&[u8], Format); 8] = [
            (b"# my config\n\nHost web\n  HostName web.example.com\n", Format::OpenSshConfig),
            (b"IdentityFile ~/.ssh/id_ed25519\n", Format::OpenSshConfig),
            (
                b"github.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl\n",
                Format::KnownHosts,
            ),
            (b"label,address,port\nweb,10.0.0.1,22\n", Format::Csv),
            (b"\xEF\xBB\xBF\"Name\",\"Host\"\nx,y\n", Format::Csv),
            (b"REGEDIT4\n\n[HKEY_CURRENT_USER\\Software]\n", Format::PuttyReg),
            (b"[Misc]\r\nx=1\r\n[Bookmarks]\r\nSubRep=\r\n", Format::MobaXterm),
            (
                b"<?xml version=\"1.0\"?>\n<VanDyke version=\"3.0\"></VanDyke>",
                Format::SecureCrtXml,
            ),
        ];
        for (input, want) in cases {
            assert_eq!(
                detect_format(input),
                Some(want),
                "{:?}",
                String::from_utf8_lossy(input)
            );
        }
        let reg = utf16le("Windows Registry Editor Version 5.00\r\n\r\n");
        assert_eq!(detect_format(&reg), Some(Format::PuttyReg));
        assert_eq!(detect_format(b""), None);
        assert_eq!(detect_format(b"\n\n"), None);
        assert_eq!(detect_format(b"hello world"), None);
        assert_eq!(detect_format(&[0xFF, 0xFE, 0x00]), None);
    }

    #[test]
    fn import_dispatches_and_reports_lossy_text() {
        let (format, b) = import_auto(b"Host caf\xE9\n  HostName cafe.example.com\n").unwrap();
        assert_eq!(format, Format::OpenSshConfig);
        assert_eq!(b.warnings[0].kind, WarningKind::Invalid);
        assert_eq!(b.hosts.len(), 1);
        let reg = utf16le(
            "Windows Registry Editor Version 5.00\r\n\r\n\
             [HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\a]\r\n\
             \"HostName\"=\"a.example.com\"\r\n",
        );
        let b = import(Format::PuttyReg, &reg).unwrap();
        assert_eq!(b.hosts[0].address, "a.example.com");
        assert_eq!(
            import_auto(b"what is this").err(),
            Some(ImportError::UnknownFormat)
        );
        assert!(matches!(
            import(Format::Csv, &vec![b'a'; crate::MAX_INPUT_BYTES + 1]),
            Err(ImportError::TooLarge { .. })
        ));
    }

    #[test]
    fn format_names() {
        assert_eq!(Format::PuttyReg.to_string(), "PuTTY registry export");
        assert_eq!(Format::ALL.len(), 6);
    }
}
