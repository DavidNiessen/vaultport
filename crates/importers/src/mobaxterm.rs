// SPDX-License-Identifier: GPL-3.0-or-later
//! MobaXterm `.mobaconf` / `.mxtsession` importer.
//!
//! Bookmarks live in `[Bookmarks]` and `[Bookmarks_N]` INI sections. `SubRep=` holds the
//! folder path (`Production\EU`, nested with `\`), which becomes nested groups. Every other
//! entry is a session: `name=#<type>#<icon>%<field>%<field>...#<terminal settings>#...`.
//!
//! Supported session types: SSH (`109`, fields: host, port, user, ..., private key at index
//! 14) and Telnet (`98`, fields: host, port). Other types (RDP, VNC, FTP, serial, ...) are
//! reported as unsupported. MobaXterm's SSH gateway (jump host) settings are not imported.
//! Stored passwords are encrypted with the MobaXterm master password and cannot be imported
//! (a warning reports how many were found).

use crate::bundle::{ImportBundle, KeyCache, KeyFileFormat, WarningKind};
use crate::common::{GroupTree, HostDraft, Protocol, parse_port};
use crate::error::ImportError;
use crate::finalize::finalize;
use crate::text::check_size;

const TYPE_SSH: u32 = 109;
const TYPE_TELNET: u32 = 98;
const FIELD_HOST: usize = 1;
const FIELD_PORT: usize = 2;
const FIELD_USER: usize = 3;
const FIELD_SSH_KEY: usize = 14;

#[derive(Debug, PartialEq, Eq)]
enum Section {
    Bookmarks,
    Secrets,
    Other,
}

/// Parses a MobaXterm configuration or session export.
pub fn parse(input: &str) -> Result<ImportBundle, ImportError> {
    check_size(input.len())?;
    let mut b = ImportBundle::default();
    let mut groups = GroupTree::default();
    let mut keys = KeyCache::default();
    let mut section = Section::Other;
    let mut folder: Option<core_model::GroupId> = None;
    let mut secrets = 0usize;
    let mut saw_bookmarks = false;
    for (idx, raw) in input.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let lower = name.trim().to_ascii_lowercase();
            section = if lower == "bookmarks" || lower.starts_with("bookmarks_") {
                saw_bookmarks = true;
                Section::Bookmarks
            } else if lower == "passwords" || lower == "credentials" {
                Section::Secrets
            } else {
                Section::Other
            };
            folder = None;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match section {
            Section::Other => {}
            Section::Secrets => secrets += 1,
            Section::Bookmarks => {
                if key.eq_ignore_ascii_case("SubRep") {
                    folder = groups.ensure(&mut b, value.split('\\'), Some(line_no));
                } else if key.eq_ignore_ascii_case("ImgNum") {
                } else if value.starts_with('#') {
                    session(&mut b, &mut keys, key, value, folder, line_no);
                }
            }
        }
    }
    if !saw_bookmarks && secrets == 0 {
        return Err(ImportError::Malformed {
            format: "MobaXterm",
            reason: "no [Bookmarks] section".into(),
        });
    }
    if secrets > 0 {
        b.warn(
            WarningKind::SecretNotImported,
            None,
            None,
            format!(
                "{secrets} stored password(s) are encrypted with the MobaXterm master \
                 password and were not imported"
            ),
        );
    }
    Ok(finalize(b))
}

fn session(
    b: &mut ImportBundle,
    keys: &mut KeyCache,
    name: &str,
    value: &str,
    group: Option<core_model::GroupId>,
    line: usize,
) {
    let mut parts = value.split('#').skip(1);
    let kind = parts.next().and_then(|t| t.trim().parse::<u32>().ok());
    let fields: Vec<&str> = parts.next().unwrap_or_default().split('%').collect();
    let field = |i: usize| fields.get(i).map_or("", |f| f.trim());
    let protocol = match kind {
        Some(TYPE_SSH) => Protocol::Ssh,
        Some(TYPE_TELNET) => Protocol::Telnet,
        Some(other) => {
            b.warn(
                WarningKind::Unsupported,
                Some(line),
                Some(name),
                format!(
                    "session type {other} ({}) is not supported; skipped",
                    type_name(other)
                ),
            );
            return;
        }
        None => {
            b.warn(
                WarningKind::Invalid,
                Some(line),
                Some(name),
                "malformed session skipped",
            );
            return;
        }
    };
    let host = field(FIELD_HOST);
    if host.is_empty() {
        b.warn(
            WarningKind::Invalid,
            Some(line),
            Some(name),
            "session without host skipped",
        );
        return;
    }
    let mut d = HostDraft::new(name, host, protocol);
    d.group = group;
    let port = field(FIELD_PORT);
    if !port.is_empty() {
        d.port = parse_port(port);
        if d.port.is_none() {
            b.warn(
                WarningKind::Invalid,
                Some(line),
                Some(name),
                "invalid port ignored",
            );
        }
    }
    if protocol == Protocol::Ssh {
        d.username(field(FIELD_USER));
        let key = field(FIELD_SSH_KEY);
        if !key.is_empty() && key != "-1" {
            let format = if key.to_ascii_lowercase().ends_with(".ppk") {
                KeyFileFormat::Ppk
            } else {
                KeyFileFormat::Unknown
            };
            d.keys.push(keys.key(b, key, format));
        }
    }
    let host = d.build(b);
    b.hosts.push(host);
}

fn type_name(t: u32) -> &'static str {
    match t {
        91 => "VNC",
        128 => "RDP",
        129 => "Xdmcp",
        130 => "FTP",
        131 => "serial",
        132 => "file",
        133 => "shell",
        139 => "browser",
        140 => "SFTP",
        141 => "mosh",
        151 => "AWS S3",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use core_model::Credential;

    const TAIL: &str = "#MobaFont%10%0%0%-1%15%236,236,236%30,30,30%180,180,192%0%-1%0%%xterm%-1%-1%_Std_Colors_0_%80%24%0%1%-1%<none>%%0%0%-1#0# #-1";

    fn fixture() -> String {
        format!(
            "[Misc]\r\nPasswordsInRegistry=0\r\n\r\n\
             [Bookmarks]\r\nSubRep=\r\nImgNum=42\r\n\
             root server=#109#0%192.168.1.10%22%root%%-1%-1%%%%%0%0%0%_ProfileDir_\\.ssh\\id_rsa%%-1%0%0%0%%1080%%0%0%1{TAIL}\r\n\
             desktop=#91#4%192.168.1.20%5900%%-1%0%0{TAIL}\r\n\
             \r\n\
             [Bookmarks_1]\r\nSubRep=Production\\EU\r\nImgNum=41\r\n\
             web01=#109#0%10.0.0.5%2222%deploy%%-1%-1%%%%%0%0%0%C:\\keys\\deploy.ppk%%-1%0%0%0%%1080%%0%0%1{TAIL}\r\n\
             switch=#98#1%10.0.0.254%23%%%2%%%%%0%0%%{TAIL}\r\n\
             broken=#109#0%%22%x{TAIL}\r\n\
             garbage=#abc#\r\n\
             \r\n\
             [Bookmarks_2]\r\nSubRep=Production\\US\r\nImgNum=41\r\n\
             web02=#109#0%10.1.0.5%%%%-1{TAIL}\r\n\
             \r\n\
             [Passwords]\r\nssh22:root@192.168.1.10=_encrypted1_\r\nssh22:deploy@10.0.0.5=_encrypted2_\r\n"
        )
    }

    #[test]
    fn realistic_config() {
        let b = parse(&fixture()).unwrap();
        let labels: Vec<&str> = b.hosts.iter().map(|h| h.label.as_str()).collect();
        assert_eq!(labels, ["root server", "web01", "switch", "web02"]);
        let groups: Vec<&str> = b.groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(groups, ["Production", "EU", "US"]);

        let root = &b.hosts[0];
        assert_eq!(root.address, "192.168.1.10");
        assert_eq!(root.group, None);
        assert_eq!(root.config.ssh.port, Some(22));
        let Some(Credential::Inline { username, keys, .. }) = &root.config.ssh.credential else {
            panic!()
        };
        assert_eq!(username.as_deref(), Some("root"));
        assert_eq!(keys.len(), 1);
        assert_eq!(
            b.keys[0].source_path.as_deref(),
            Some(r"_ProfileDir_\.ssh\id_rsa")
        );
        assert_eq!(b.keys[0].format, KeyFileFormat::Unknown);
        assert_eq!(b.keys[1].format, KeyFileFormat::Ppk);

        let web = &b.hosts[1];
        assert_eq!(web.group, Some(b.groups[1].id));
        assert_eq!(web.config.ssh.port, Some(2222));
        let sw = &b.hosts[2];
        assert_eq!(sw.config.telnet.enabled, Some(true));
        assert_eq!(sw.config.telnet.port, Some(23));
        let web02 = &b.hosts[3];
        assert_eq!(web02.group, Some(b.groups[2].id));
        assert_eq!(web02.config.ssh.port, None);
        assert_eq!(web02.config.ssh.credential, None);

        let msgs: Vec<String> = b.warnings.iter().map(ToString::to_string).collect();
        let has = |s: &str| msgs.iter().any(|m| m.contains(s));
        assert!(has("type 91 (VNC)"), "{msgs:#?}");
        assert!(has("broken: session without host"));
        assert!(has("garbage: malformed"));
        assert!(has("2 stored password(s)"));
        assert!(!has("_encrypted"));
    }

    #[test]
    fn rejects_unrelated_ini() {
        assert!(parse("[Section]\nkey=value\n").is_err());
        assert!(parse("[Bookmarks]\nSubRep=\n").unwrap().is_empty());
    }
}
