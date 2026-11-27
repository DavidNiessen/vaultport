// SPDX-License-Identifier: GPL-3.0-or-later
//! Helpers shared by the session-oriented importers (CSV, PuTTY, MobaXterm, SecureCRT).

use crate::bundle::{ImportBundle, SecretKind, WarningKind, truncate_for_display};
use core_model::{Credential, Group, GroupId, Host, HostId, KeyId, ProxyConfig, validate_label};
use std::collections::HashMap;
use zeroize::Zeroizing;

/// Maximum group nesting accepted from any source.
pub(crate) const MAX_GROUP_DEPTH: usize = 32;

/// Builds a group hierarchy from folder paths, creating each group once.
#[derive(Debug, Default)]
pub(crate) struct GroupTree {
    by_path: HashMap<Vec<String>, GroupId>,
}

impl GroupTree {
    /// Returns the group for the folder path `segments` (outermost first), creating missing
    /// groups. Empty segments are ignored. Returns `None` for the root.
    pub(crate) fn ensure<'s>(
        &mut self,
        bundle: &mut ImportBundle,
        segments: impl IntoIterator<Item = &'s str>,
        line: Option<usize>,
    ) -> Option<GroupId> {
        let mut path: Vec<String> = Vec::new();
        let mut parent: Option<GroupId> = None;
        for seg in segments {
            let seg = seg.trim();
            if seg.is_empty() {
                continue;
            }
            if path.len() >= MAX_GROUP_DEPTH {
                bundle.warn(
                    WarningKind::Limit,
                    line,
                    Some(seg),
                    format!("groups nested deeper than {MAX_GROUP_DEPTH} levels were flattened"),
                );
                break;
            }
            if let Err(e) = validate_label(seg) {
                bundle.warn(
                    WarningKind::Invalid,
                    line,
                    Some(seg),
                    format!("invalid group name ({e}); placed in the parent group instead"),
                );
                break;
            }
            path.push(seg.to_owned());
            let id = if let Some(id) = self.by_path.get(&path) {
                *id
            } else {
                let mut g = Group::new(seg);
                g.parent = parent;
                let id = g.id;
                bundle.groups.push(g);
                self.by_path.insert(path.clone(), id);
                id
            };
            parent = Some(id);
        }
        parent
    }
}

/// Connection protocol of an imported session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Protocol {
    Ssh,
    Telnet,
    Mosh,
}

/// A session description that is turned into a [`Host`].
#[derive(Debug)]
pub(crate) struct HostDraft {
    pub label: String,
    pub address: String,
    pub protocol: Protocol,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<Zeroizing<Vec<u8>>>,
    pub keys: Vec<KeyId>,
    pub group: Option<GroupId>,
    pub tags: Vec<String>,
    pub proxy: Option<ProxyConfig>,
    pub jump_chain: Option<Vec<HostId>>,
}

impl HostDraft {
    pub(crate) fn new(label: &str, address: &str, protocol: Protocol) -> Self {
        let address = address.trim();
        let label = if label.trim().is_empty() {
            address
        } else {
            label.trim()
        };
        Self {
            label: label.to_owned(),
            address: address.to_owned(),
            protocol,
            port: None,
            username: None,
            password: None,
            keys: Vec::new(),
            group: None,
            tags: Vec::new(),
            proxy: None,
            jump_chain: None,
        }
    }

    /// Sets the username, treating empty strings as unset.
    pub(crate) fn username(&mut self, user: &str) {
        let u = user.trim();
        self.username = (!u.is_empty()).then(|| u.to_owned());
    }

    /// Converts the draft into a host, moving any password into a secret slot of `bundle`.
    pub(crate) fn build(self, bundle: &mut ImportBundle) -> Host {
        let mut host = Host::new(self.label, self.address);
        host.group = self.group;
        host.tags = self.tags;
        let password = self.password.filter(|p| !p.is_empty()).map(|p| {
            bundle.add_secret(
                SecretKind::Password,
                format!("password for {}", truncate_for_display(&host.label, 120)),
                p,
            )
        });
        match self.protocol {
            Protocol::Telnet => {
                host.config.ssh.enabled = Some(false);
                host.config.telnet.enabled = Some(true);
                host.config.telnet.port = self.port;
                host.config.telnet.username = self.username;
                host.config.telnet.password = password;
            }
            Protocol::Ssh | Protocol::Mosh => {
                if self.protocol == Protocol::Mosh {
                    host.config.mosh.enabled = Some(true);
                }
                host.config.ssh.port = self.port;
                host.config.ssh.proxy = self.proxy;
                host.config.ssh.jump_chain = self.jump_chain;
                if self.username.is_some() || password.is_some() || !self.keys.is_empty() {
                    host.config.ssh.credential = Some(Credential::Inline {
                        username: self.username,
                        password,
                        keys: self.keys,
                    });
                }
            }
        }
        host
    }
}

/// Splits `host[:port]`, `[v6]:port` or a bare IPv6 address. Returns `None` for a malformed
/// port or empty host.
pub(crate) fn split_host_port(s: &str) -> Option<(&str, Option<u16>)> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        if host.is_empty() {
            return None;
        }
        return match after {
            "" => Some((host, None)),
            p => Some((host, Some(parse_port(p.strip_prefix(':')?)?))),
        };
    }
    if s.is_empty() {
        return None;
    }
    match s.matches(':').count() {
        1 => {
            let (h, p) = s.split_once(':')?;
            if h.is_empty() {
                return None;
            }
            Some((h, Some(parse_port(p)?)))
        }
        // No colon, or a bare IPv6 address.
        _ => Some((s, None)),
    }
}

/// Parses a non-zero TCP port.
pub(crate) fn parse_port(s: &str) -> Option<u16> {
    s.trim().parse::<u16>().ok().filter(|p| *p != 0)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn host_port_forms() {
        assert_eq!(split_host_port("a.example"), Some(("a.example", None)));
        assert_eq!(
            split_host_port("a.example:2222"),
            Some(("a.example", Some(2222)))
        );
        assert_eq!(split_host_port("[::1]:22"), Some(("::1", Some(22))));
        assert_eq!(split_host_port("[::1]"), Some(("::1", None)));
        assert_eq!(split_host_port("fe80::1"), Some(("fe80::1", None)));
        assert_eq!(split_host_port("a:0"), None);
        assert_eq!(split_host_port("a:x"), None);
        assert_eq!(split_host_port(":22"), None);
        assert_eq!(split_host_port("[]:22"), None);
        assert_eq!(split_host_port("[::1]x"), None);
    }

    #[test]
    fn group_tree_reuses_and_nests() {
        let mut b = ImportBundle::default();
        let mut t = GroupTree::default();
        let eu = t.ensure(&mut b, ["Prod", "EU"], None).unwrap();
        let eu2 = t.ensure(&mut b, ["Prod", "", "EU"], None).unwrap();
        let us = t.ensure(&mut b, ["Prod", "US"], None).unwrap();
        assert_eq!(eu, eu2);
        assert_ne!(eu, us);
        assert_eq!(b.groups.len(), 3);
        let prod = b.groups[0].id;
        assert!(b.groups[1..].iter().all(|g| g.parent == Some(prod)));
        assert_eq!(t.ensure(&mut b, [" ", ""], None), None);
    }

    #[test]
    fn group_tree_limits_depth_and_rejects_bad_labels() {
        let mut b = ImportBundle::default();
        let mut t = GroupTree::default();
        let segs: Vec<String> = (0..50).map(|i| format!("g{i}")).collect();
        t.ensure(&mut b, segs.iter().map(String::as_str), None);
        assert_eq!(b.groups.len(), MAX_GROUP_DEPTH);
        let mut b = ImportBundle::default();
        let g = t.ensure(&mut b, ["ok", "bad\u{1}"], None);
        assert!(g.is_some());
        assert_eq!(b.warnings.len(), 1);
    }

    #[test]
    fn telnet_draft() {
        let mut b = ImportBundle::default();
        let mut d = HostDraft::new("", "switch1", Protocol::Telnet);
        d.port = Some(23);
        d.username("admin");
        d.password = Some(Zeroizing::new(b"pw".to_vec()));
        let h = d.build(&mut b);
        assert_eq!(h.label, "switch1");
        assert_eq!(h.config.ssh.enabled, Some(false));
        assert_eq!(h.config.telnet.port, Some(23));
        assert_eq!(h.config.telnet.password, Some(b.secrets[0].slot));
    }
}
