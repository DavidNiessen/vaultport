// SPDX-License-Identifier: GPL-3.0-or-later
//! CSV importer.
//!
//! The first row must be a header. Columns are matched by name (case-insensitive, any order);
//! only `address` is required. See [`TEMPLATE_HEADER`]:
//!
//! | Column | Meaning |
//! |---|---|
//! | `label` | Display name (defaults to the address) |
//! | `address` | Hostname or IP address |
//! | `port` | Port (default depends on the protocol) |
//! | `username` | Login name |
//! | `password` | Password; returned as an [`ImportedSecret`](crate::ImportedSecret) |
//! | `group` | Group path, nested with `/` (e.g. `Prod/EU`) |
//! | `tags` | Tags separated by `;` |
//! | `protocol` | `ssh` (default), `telnet` or `mosh` |
//!
//! Note: the CSV reader's internal buffers holding passwords cannot be zeroized; only the
//! returned secrets are.

use crate::bundle::{ImportBundle, WarningKind};
use crate::common::{GroupTree, HostDraft, Protocol, parse_port};
use crate::error::ImportError;
use crate::finalize::finalize;
use crate::text::check_size;
use zeroize::Zeroizing;

/// Header row of the CSV template offered to users.
pub const TEMPLATE_HEADER: &str = "label,address,port,username,password,group,tags,protocol";

#[derive(Debug, Default)]
struct Columns {
    label: Option<usize>,
    address: Option<usize>,
    port: Option<usize>,
    username: Option<usize>,
    password: Option<usize>,
    group: Option<usize>,
    tags: Option<usize>,
    protocol: Option<usize>,
}

/// Parses a CSV file with a header row.
pub fn parse(input: &str) -> Result<ImportBundle, ImportError> {
    check_size(input.len())?;
    let mut b = ImportBundle::default();
    let mut reader = ::csv::ReaderBuilder::new()
        .flexible(true)
        .trim(::csv::Trim::All)
        .has_headers(true)
        .from_reader(input.as_bytes());
    let headers = reader
        .headers()
        .map_err(|e| ImportError::Malformed {
            format: "CSV",
            reason: e.to_string(),
        })?
        .clone();
    let mut cols = Columns::default();
    for (i, name) in headers.iter().enumerate() {
        let slot = match name.to_ascii_lowercase().as_str() {
            "label" | "name" => &mut cols.label,
            "address" | "host" | "hostname" => &mut cols.address,
            "port" => &mut cols.port,
            "username" | "user" => &mut cols.username,
            "password" => &mut cols.password,
            "group" => &mut cols.group,
            "tags" => &mut cols.tags,
            "protocol" => &mut cols.protocol,
            other => {
                let shown: String = other.chars().take(64).collect();
                b.warn(
                    WarningKind::Unsupported,
                    Some(1),
                    None,
                    format!("unknown column {shown:?} ignored"),
                );
                continue;
            }
        };
        if slot.is_none() {
            *slot = Some(i);
        }
    }
    let Some(address_col) = cols.address else {
        return Err(ImportError::Malformed {
            format: "CSV",
            reason: format!("missing \"address\" column (expected header: {TEMPLATE_HEADER})"),
        });
    };

    let mut groups = GroupTree::default();
    let mut record = ::csv::StringRecord::new();
    loop {
        match reader.read_record(&mut record) {
            Ok(false) => break,
            Ok(true) => {}
            Err(e) => {
                let line = e.position().map(|p| p.line() as usize);
                b.warn(
                    WarningKind::Invalid,
                    line,
                    None,
                    "malformed CSV row skipped",
                );
                // An I/O-level error cannot recover; a parse error skips the row.
                if matches!(e.kind(), ::csv::ErrorKind::Io(_)) {
                    break;
                }
                continue;
            }
        }
        let line = record.position().map(|p| p.line() as usize);
        if record.iter().all(str::is_empty) {
            continue;
        }
        let get = |c: Option<usize>| c.and_then(|i| record.get(i)).unwrap_or("");
        let address = get(Some(address_col));
        let label = get(cols.label);
        let item = if label.is_empty() { address } else { label };
        if address.is_empty() {
            b.warn(
                WarningKind::Invalid,
                line,
                Some(item),
                "row without address skipped",
            );
            continue;
        }
        let protocol = match get(cols.protocol).to_ascii_lowercase().as_str() {
            "" | "ssh" | "ssh2" => Protocol::Ssh,
            "telnet" => Protocol::Telnet,
            "mosh" => Protocol::Mosh,
            other => {
                let shown: String = other.chars().take(32).collect();
                b.warn(
                    WarningKind::Unsupported,
                    line,
                    Some(item),
                    format!("protocol {shown:?} is not supported; row skipped"),
                );
                continue;
            }
        };
        let mut d = HostDraft::new(label, address, protocol);
        let port = get(cols.port);
        if !port.is_empty() {
            let Some(p) = parse_port(port) else {
                b.warn(
                    WarningKind::Invalid,
                    line,
                    Some(item),
                    "invalid port; row skipped",
                );
                continue;
            };
            d.port = Some(p);
        }
        d.username(get(cols.username));
        let password = get(cols.password);
        if !password.is_empty() {
            d.password = Some(Zeroizing::new(password.as_bytes().to_vec()));
        }
        d.group = groups.ensure(&mut b, get(cols.group).split('/'), line);
        for tag in get(cols.tags)
            .split(';')
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            if tag.len() > 64 || tag.chars().any(|c| c.is_control() || c == ',') {
                b.warn(
                    WarningKind::Invalid,
                    line,
                    Some(item),
                    "invalid tag dropped",
                );
            } else if !d.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                d.tags.push(tag.to_owned());
            }
        }
        let host = d.build(&mut b);
        b.hosts.push(host);
    }
    Ok(finalize(b))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use core_model::Credential;

    #[test]
    fn template_rows() {
        let input = format!(
            "{TEMPLATE_HEADER}\n\
             web01,10.0.0.10,2222,deploy,s3cr3t,Prod/EU,web;nginx,ssh\n\
             web02,10.0.0.11,,deploy,,Prod/EU,web,\n\
             \"db, primary\",db.example.com,5432,,,Prod/US,,ssh\n\
             switch,192.0.2.1,23,admin,pw,Network,,telnet\n\
             ,mosh.example.com,,me,,,,mosh\n"
        );
        let b = parse(&input).unwrap();
        assert!(b.warnings.is_empty(), "{:?}", b.warnings);
        assert_eq!(b.hosts.len(), 5);
        let labels: Vec<&str> = b.groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, ["Prod", "EU", "US", "Network"]);
        let prod = b.groups[0].id;
        assert_eq!(b.groups[1].parent, Some(prod));
        assert_eq!(b.groups[2].parent, Some(prod));

        let web = &b.hosts[0];
        assert_eq!(web.label, "web01");
        assert_eq!(web.group, Some(b.groups[1].id));
        assert_eq!(web.tags, ["web", "nginx"]);
        assert_eq!(web.config.ssh.port, Some(2222));
        let Some(Credential::Inline {
            username, password, ..
        }) = &web.config.ssh.credential
        else {
            panic!("no credential")
        };
        assert_eq!(username.as_deref(), Some("deploy"));
        let secret = b
            .secrets
            .iter()
            .find(|s| Some(s.slot) == *password)
            .unwrap();
        assert_eq!(secret.value.as_slice(), b"s3cr3t");
        assert_eq!(secret.description, "password for web01");

        assert_eq!(b.hosts[2].label, "db, primary");
        let sw = &b.hosts[3];
        assert_eq!(sw.config.telnet.enabled, Some(true));
        assert_eq!(sw.config.telnet.username.as_deref(), Some("admin"));
        assert!(sw.config.telnet.password.is_some());
        assert_eq!(b.hosts[4].label, "mosh.example.com");
        assert_eq!(b.hosts[4].config.mosh.enabled, Some(true));
        assert_eq!(b.secrets.len(), 2);
    }

    #[test]
    fn columns_in_any_order_and_bad_rows() {
        let input = "Address,Label,Extra,Port,Protocol,Tags\n\
                     a.example,A,x,22,ssh,ok;bad\u{1}tag\n\
                     b.example,B,x,99999,ssh,\n\
                     c.example,C,x,22,rdp,\n\
                     ,D,x,22,ssh,\n\
                     a.example,A again,x,22,ssh,\n\
                     not a host,E,x,,,\n\
                     \n\
                     f.example\n";
        let b = parse(input).unwrap();
        let labels: Vec<&str> = b.hosts.iter().map(|h| h.label.as_str()).collect();
        assert_eq!(labels, ["A", "f.example"]);
        assert_eq!(b.hosts[0].tags, ["ok"]);
        let kinds: Vec<WarningKind> = b.warnings.iter().map(|w| w.kind).collect();
        assert!(kinds.contains(&WarningKind::Duplicate));
        assert!(kinds.contains(&WarningKind::Unsupported));
        assert_eq!(
            b.warnings
                .iter()
                .filter(|w| w.kind == WarningKind::Invalid)
                .count(),
            4,
            "{:#?}",
            b.warnings
        );
        assert!(!format!("{:?}", b.warnings).contains("bad\u{1}tag"));
    }

    #[test]
    fn missing_address_column_is_an_error() {
        assert!(matches!(
            parse("label,port\nx,22\n"),
            Err(ImportError::Malformed { .. })
        ));
        assert!(parse("").is_err());
    }

    #[test]
    fn passwords_never_appear_in_warnings() {
        let b = parse("address,password,port\nbad host,topsecret,22\n").unwrap();
        assert!(b.hosts.is_empty());
        assert!(b.secrets.is_empty(), "orphaned secrets are dropped");
        assert!(!format!("{b:?}").contains("topsecret"));
    }
}
