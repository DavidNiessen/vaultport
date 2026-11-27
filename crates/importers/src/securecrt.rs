// SPDX-License-Identifier: GPL-3.0-or-later
//! SecureCRT XML export importer (`File → Export Settings`).
//!
//! The export is a tree of `<key name="...">` elements under
//! `<VanDyke><key name="Sessions">`. Keys that contain session values (`Hostname`,
//! `Protocol Name`) are sessions; all other keys are folders and become nested groups.
//!
//! Imported values: `Hostname`, `Username`, `Protocol Name` (`SSH2` or `Telnet`),
//! `[SSH2] Port` / `Port`, and `Identity Filename V2` (key path). Saved passwords are
//! encrypted by SecureCRT and are not imported (warning). SSH1 and other protocols are
//! reported as unsupported.
//!
//! The XML is parsed without DTD or external entity processing, with bounded nesting.

use crate::bundle::{ImportBundle, KeyCache, KeyFileFormat, WarningKind};
use crate::common::{GroupTree, HostDraft, Protocol};
use crate::error::ImportError;
use crate::finalize::finalize;
use crate::text::check_size;
use core_model::GroupId;
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

/// Maximum XML element nesting.
const MAX_XML_DEPTH: usize = 64;

#[derive(Debug, Default)]
struct XKey {
    name: String,
    strings: Vec<(String, String)>,
    dwords: Vec<(String, String)>,
    children: Vec<XKey>,
}

impl XKey {
    fn string(&self, name: &str) -> Option<&str> {
        self.strings
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.trim())
            .filter(|v| !v.is_empty())
    }

    fn dword(&self, name: &str) -> Option<u32> {
        self.dwords
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .and_then(|(_, v)| v.trim().parse().ok())
    }

    fn is_session(&self) -> bool {
        self.strings.iter().any(|(n, _)| {
            n.eq_ignore_ascii_case("Hostname") || n.eq_ignore_ascii_case("Protocol Name")
        })
    }
}

#[derive(Debug)]
enum Frame {
    Key(XKey),
    Value {
        dword: bool,
        name: String,
        text: String,
    },
    Ignored,
}

/// Parses a SecureCRT XML export.
pub fn parse(input: &str) -> Result<ImportBundle, ImportError> {
    check_size(input.len())?;
    let root = read_tree(input)?;
    let mut b = ImportBundle::default();
    let sessions = find_sessions(&root, 0).unwrap_or(&root);
    let mut groups = GroupTree::default();
    let mut keys = KeyCache::default();
    let mut path = Vec::new();
    walk(&mut b, &mut groups, &mut keys, sessions, &mut path, 0);
    Ok(finalize(b))
}

fn malformed(reason: impl Into<String>) -> ImportError {
    ImportError::Malformed {
        format: "SecureCRT XML",
        reason: reason.into(),
    }
}

fn name_attr(e: &BytesStart<'_>) -> String {
    e.try_get_attribute("name")
        .ok()
        .flatten()
        .and_then(|a| {
            a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .ok()
                .map(std::borrow::Cow::into_owned)
        })
        .unwrap_or_default()
}

fn open_frame(parent: Option<&Frame>, e: &BytesStart<'_>) -> Frame {
    let in_key = matches!(parent, Some(Frame::Key(_)));
    match e.local_name().as_ref() {
        "key" | "VanDyke" if in_key => Frame::Key(XKey {
            name: name_attr(e),
            ..XKey::default()
        }),
        "string" if in_key => Frame::Value {
            dword: false,
            name: name_attr(e),
            text: String::new(),
        },
        "dword" if in_key => Frame::Value {
            dword: true,
            name: name_attr(e),
            text: String::new(),
        },
        _ => Frame::Ignored,
    }
}

fn close_frame(stack: &mut [Frame], frame: Frame) {
    let Some(Frame::Key(parent)) = stack.last_mut() else {
        return;
    };
    match frame {
        Frame::Key(k) => parent.children.push(k),
        Frame::Value { dword, name, text } => {
            if dword {
                parent.dwords.push((name, text));
            } else {
                parent.strings.push((name, text));
            }
        }
        Frame::Ignored => {}
    }
}

fn read_tree(input: &str) -> Result<XKey, ImportError> {
    let mut reader = Reader::from_str(input);
    let mut stack: Vec<Frame> = vec![Frame::Key(XKey::default())];
    let mut saw_vandyke = false;
    loop {
        let event = reader.read_event().map_err(|e| {
            malformed(format!(
                "XML error at byte {}: {e}",
                reader.error_position()
            ))
        })?;
        match event {
            Event::Start(e) => {
                if stack.len() > MAX_XML_DEPTH {
                    return Err(malformed("elements nested too deeply"));
                }
                saw_vandyke |= e.local_name().as_ref() == "VanDyke";
                let f = open_frame(stack.last(), &e);
                stack.push(f);
            }
            Event::Empty(e) => {
                saw_vandyke |= e.local_name().as_ref() == "VanDyke";
                let f = open_frame(stack.last(), &e);
                close_frame(&mut stack, f);
            }
            Event::End(_) => {
                if stack.len() > 1
                    && let Some(f) = stack.pop()
                {
                    close_frame(&mut stack, f);
                }
            }
            Event::Text(t) => {
                if let Some(Frame::Value { text, .. }) = stack.last_mut() {
                    text.push_str(&t.xml10_content());
                }
            }
            Event::CData(t) => {
                if let Some(Frame::Value { text, .. }) = stack.last_mut() {
                    text.push_str(t.as_ref());
                }
            }
            Event::GeneralRef(r) => {
                if let Some(Frame::Value { text, .. }) = stack.last_mut() {
                    let c = match r.resolve_char_ref() {
                        Ok(Some(c)) => Some(c),
                        _ => match r.as_ref() {
                            "lt" => Some('<'),
                            "gt" => Some('>'),
                            "amp" => Some('&'),
                            "apos" => Some('\''),
                            "quot" => Some('"'),
                            _ => None,
                        },
                    };
                    text.extend(c);
                }
            }
            Event::Eof => break,
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
        }
    }
    while stack.len() > 1 {
        if let Some(f) = stack.pop() {
            close_frame(&mut stack, f);
        }
    }
    if !saw_vandyke {
        return Err(malformed("missing <VanDyke> root element"));
    }
    match stack.pop() {
        Some(Frame::Key(root)) => Ok(root),
        _ => Err(malformed("empty document")),
    }
}

fn find_sessions(k: &XKey, depth: usize) -> Option<&XKey> {
    if depth > MAX_XML_DEPTH {
        return None;
    }
    if k.name.eq_ignore_ascii_case("Sessions") {
        return Some(k);
    }
    k.children.iter().find_map(|c| find_sessions(c, depth + 1))
}

fn walk<'a>(
    b: &mut ImportBundle,
    groups: &mut GroupTree,
    keys: &mut KeyCache,
    folder: &'a XKey,
    path: &mut Vec<&'a str>,
    depth: usize,
) {
    if depth > MAX_XML_DEPTH {
        return;
    }
    for child in &folder.children {
        if child.is_session() {
            if path.is_empty() && child.name == "Default" {
                continue;
            }
            let group = groups.ensure(b, path.iter().copied(), None);
            session(b, keys, child, group);
        } else if !child.children.is_empty() {
            path.push(&child.name);
            walk(b, groups, keys, child, path, depth + 1);
            path.pop();
        }
    }
}

fn session(b: &mut ImportBundle, keys: &mut KeyCache, k: &XKey, group: Option<GroupId>) {
    let name = k.name.as_str();
    let protocol_name = k.string("Protocol Name").unwrap_or("SSH2");
    let protocol = match protocol_name.to_ascii_lowercase().as_str() {
        "ssh2" => Protocol::Ssh,
        "telnet" => Protocol::Telnet,
        _ => {
            let shown: String = protocol_name.chars().take(32).collect();
            b.warn(
                WarningKind::Unsupported,
                None,
                Some(name),
                format!("protocol {shown:?} is not supported; skipped"),
            );
            return;
        }
    };
    let Some(host) = k.string("Hostname") else {
        b.warn(
            WarningKind::Invalid,
            None,
            Some(name),
            "session without host name skipped",
        );
        return;
    };
    let mut d = HostDraft::new(name, host, protocol);
    d.group = group;
    let port_key = if protocol == Protocol::Ssh {
        "[SSH2] Port"
    } else {
        "Port"
    };
    d.port = k
        .dword(port_key)
        .and_then(|p| u16::try_from(p).ok())
        .filter(|p| *p != 0);
    d.username(k.string("Username").unwrap_or_default());
    if protocol == Protocol::Ssh
        && let Some(id) = k
            .string("Identity Filename V2")
            .or_else(|| k.string("Identity Filename"))
    {
        let path = id.split_once("::").map_or(id, |(p, _)| p).trim();
        if !path.is_empty() {
            let format = if path.to_ascii_lowercase().ends_with(".ppk") {
                KeyFileFormat::Ppk
            } else {
                KeyFileFormat::Unknown
            };
            d.keys.push(keys.key(b, path, format));
        }
    }
    if k.string("Password V2").is_some() || k.string("Password").is_some() {
        b.warn(
            WarningKind::SecretNotImported,
            None,
            Some(name),
            "saved password is encrypted by SecureCRT and was not imported",
        );
    }
    if k.string("Firewall Name")
        .is_some_and(|f| !f.eq_ignore_ascii_case("None"))
    {
        b.warn(
            WarningKind::Unsupported,
            None,
            Some(name),
            "SecureCRT firewall/proxy settings were not imported",
        );
    }
    let host = d.build(b);
    b.hosts.push(host);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use core_model::Credential;

    const FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE VanDyke>
<VanDyke version="3.0">
  <key name="Sessions">
    <key name="Default">
      <string name="Hostname"></string>
      <string name="Protocol Name">SSH2</string>
    </key>
    <key name="Production">
      <key name="EU">
        <key name="web01">
          <string name="Hostname">web01.eu.example.com</string>
          <dword name="[SSH2] Port">2222</dword>
          <string name="Username">deploy</string>
          <string name="Protocol Name">SSH2</string>
          <string name="Identity Filename V2">C:\Users\me\keys\id_ed25519::rawkey</string>
          <string name="Password V2">02:4f2a...</string>
          <array name="Port Forward Table V2">
            <string>ignored</string>
          </array>
        </key>
        <key name="db &amp; cache">
          <string name="Hostname">10.0.0.7</string>
          <string name="Firewall Name">corp-socks</string>
        </key>
      </key>
      <key name="switch">
        <string name="Hostname">192.0.2.1</string>
        <dword name="Port">23</dword>
        <string name="Protocol Name">Telnet</string>
      </key>
    </key>
    <key name="legacy">
      <string name="Hostname">old.example.com</string>
      <string name="Protocol Name">SSH1</string>
    </key>
    <key name="serial">
      <string name="Protocol Name">Serial</string>
    </key>
    <key name="nohost">
      <string name="Protocol Name">SSH2</string>
      <string name="Hostname"/>
    </key>
    <key name="Empty Folder"/>
  </key>
  <key name="Global Options">
    <string name="Hostname">not-a-session.example.com</string>
  </key>
</VanDyke>
"#;

    #[test]
    fn realistic_export() {
        let b = parse(FIXTURE).unwrap();
        let labels: Vec<&str> = b.hosts.iter().map(|h| h.label.as_str()).collect();
        assert_eq!(labels, ["web01", "db & cache", "switch"]);
        let groups: Vec<&str> = b.groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(groups, ["Production", "EU"]);
        let web = &b.hosts[0];
        assert_eq!(web.group, Some(b.groups[1].id));
        assert_eq!(web.address, "web01.eu.example.com");
        assert_eq!(web.config.ssh.port, Some(2222));
        let Some(Credential::Inline { username, keys, .. }) = &web.config.ssh.credential else {
            panic!()
        };
        assert_eq!(username.as_deref(), Some("deploy"));
        assert_eq!(keys.len(), 1);
        assert_eq!(
            b.keys[0].source_path.as_deref(),
            Some(r"C:\Users\me\keys\id_ed25519")
        );
        let sw = &b.hosts[2];
        assert_eq!(sw.group, Some(b.groups[0].id));
        assert_eq!(sw.config.telnet.port, Some(23));

        let msgs: Vec<String> = b.warnings.iter().map(ToString::to_string).collect();
        let has = |s: &str| msgs.iter().any(|m| m.contains(s));
        assert!(has("web01: saved password"), "{msgs:#?}");
        assert!(has("db & cache: SecureCRT firewall"));
        assert!(has("legacy: protocol \"SSH1\""));
        assert!(has("serial: protocol \"Serial\""));
        assert!(has("nohost: session without host"));
        assert!(!has("02:4f2a"));
    }

    #[test]
    fn rejects_non_securecrt_and_malformed_xml() {
        assert!(parse("<foo/>").is_err());
        assert!(parse("<VanDyke><key name=\"Sessions\"></VanDyke>").is_err());
        assert!(parse("not xml at all <<<").is_err());
        let deep = format!(
            "<VanDyke>{}{}</VanDyke>",
            "<key name=\"a\">".repeat(100),
            "</key>".repeat(100)
        );
        assert!(parse(&deep).is_err());
    }

    #[test]
    fn entity_bombs_are_not_expanded() {
        let xml = r#"<?xml version="1.0"?>
<!DOCTYPE VanDyke [<!ENTITY a "aaaaaaaaaaaaaaaa"><!ENTITY b "&a;&a;&a;&a;&a;&a;">]>
<VanDyke><key name="Sessions"><key name="x">
<string name="Hostname">h&b;.example.com</string></key></key></VanDyke>"#;
        let b = parse(xml).unwrap();
        // Unknown (DTD-declared) entities are dropped, never expanded.
        assert_eq!(b.hosts[0].address, "h.example.com");
    }

    #[test]
    fn sessions_at_root_without_sessions_key() {
        let xml = r#"<VanDyke version="3.0"><key name="box"><string name="Hostname">box.example</string></key></VanDyke>"#;
        let b = parse(xml).unwrap();
        assert_eq!(b.hosts.len(), 1);
        assert_eq!(b.hosts[0].label, "box");
    }
}
