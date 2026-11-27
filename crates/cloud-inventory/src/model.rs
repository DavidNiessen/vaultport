// SPDX-License-Identifier: GPL-3.0-or-later
//! Provider-neutral inventory types.

use core_model::{OsKind, validate_hostname};
use serde::{Deserialize, Serialize};

/// Which address of an instance becomes the host address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AddressPreference {
    /// Public IP, falling back to the private IP (default).
    #[default]
    PublicThenPrivate,
    /// Public IP only; instances without one are skipped with a warning.
    Public,
    /// Private IP only (VPN / bastion setups).
    Private,
    /// Public DNS name when the provider has one, then public IP, then private IP.
    PublicDns,
}

/// One machine reported by a provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudInstance {
    /// Stable, provider-scoped id, e.g. `aws:ec2:eu-west-1:i-0abc`, stored in `Host.cloud_id`.
    pub cloud_id: String,
    /// Display label (name tag / instance name, else DNS name, else id).
    pub label: String,
    /// Address chosen according to the [`AddressPreference`].
    pub address: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_dns: Option<String>,
    /// Provider tags, as `key:value` strings (or bare values where the provider has no keys).
    #[serde(default)]
    pub tags: Vec<String>,
    /// Operating system hint.
    #[serde(default)]
    pub os: OsKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// Default login user reported by the provider (Lightsail, Azure), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username_hint: Option<String>,
}

/// Result of one inventory run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    pub instances: Vec<CloudInstance>,
    /// Non-fatal problems (e.g. instances skipped because they have no usable address).
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl Inventory {
    /// Adds an instance after choosing its address; records a warning instead if there is no
    /// usable address.
    pub(crate) fn push(&mut self, mut draft: CloudInstance, pref: AddressPreference) {
        let pick = |v: &Option<String>| v.clone().filter(|s| validate_hostname(s).is_ok());
        let public = pick(&draft.public_ip);
        let private = pick(&draft.private_ip);
        let dns = pick(&draft.public_dns);
        let address = match pref {
            AddressPreference::PublicThenPrivate => public.or(private),
            AddressPreference::Public => public,
            AddressPreference::Private => private,
            AddressPreference::PublicDns => dns.or(public).or(private),
        };
        match address {
            Some(a) => {
                draft.address = a;
                self.instances.push(draft);
            }
            None => self.warnings.push(format!(
                "skipped {} ({}): no {} address",
                draft.label,
                draft.cloud_id,
                match pref {
                    AddressPreference::Public => "public",
                    AddressPreference::Private => "private",
                    _ => "usable",
                }
            )),
        }
    }
}

/// Builds a label that passes `core_model` validation (no control characters, ≤ 256 chars).
pub(crate) fn sanitize_label(candidates: &[Option<&str>], fallback: &str) -> String {
    for c in candidates.iter().flatten() {
        let clean: String = c.chars().filter(|ch| !ch.is_control()).take(256).collect();
        if !clean.trim().is_empty() {
            return clean.trim().to_owned();
        }
    }
    fallback
        .chars()
        .filter(|c| !c.is_control())
        .take(256)
        .collect()
}

/// Builds a tag that passes `core_model` validation (no commas or control characters, ≤ 64
/// bytes). Returns `None` for empty tags.
pub(crate) fn sanitize_tag(raw: &str) -> Option<String> {
    let mut out = String::new();
    for ch in raw.trim().chars() {
        let ch = if ch == ',' { ';' } else { ch };
        if ch.is_control() {
            continue;
        }
        if out.len() + ch.len_utf8() > 64 {
            break;
        }
        out.push(ch);
    }
    let out = out.trim().to_owned();
    (!out.is_empty()).then_some(out)
}

/// Formats a `key:value` tag (or just the key when the value is empty).
pub(crate) fn kv_tag(key: &str, value: &str) -> Option<String> {
    if value.is_empty() {
        sanitize_tag(key)
    } else {
        sanitize_tag(&format!("{key}:{value}"))
    }
}

/// Guesses the operating system from free-form provider text (platform details, image
/// names, blueprint ids, distributions).
#[must_use]
pub fn os_hint(text: &str) -> OsKind {
    let t = text.to_ascii_lowercase();
    let table: &[(&str, OsKind)] = &[
        ("windows", OsKind::Windows),
        ("ubuntu", OsKind::Ubuntu),
        ("debian", OsKind::Debian),
        ("red hat", OsKind::Rhel),
        ("rhel", OsKind::Rhel),
        ("centos", OsKind::Centos),
        ("rocky", OsKind::Rocky),
        ("alma", OsKind::Alma),
        ("fedora", OsKind::Fedora),
        ("amazon", OsKind::Amazon),
        ("amzn", OsKind::Amazon),
        ("suse", OsKind::Suse),
        ("sles", OsKind::Suse),
        ("freebsd", OsKind::Freebsd),
        ("openbsd", OsKind::Openbsd),
        ("arch", OsKind::Arch),
        ("alpine", OsKind::Alpine),
        ("nixos", OsKind::Nixos),
        ("linux", OsKind::Linux),
        ("unix", OsKind::Linux),
    ];
    table
        .iter()
        .find(|(needle, _)| t.contains(needle))
        .map_or(OsKind::Unknown, |(_, os)| *os)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_hints() {
        assert_eq!(os_hint("Linux/UNIX"), OsKind::Linux);
        assert_eq!(os_hint("Red Hat Enterprise Linux"), OsKind::Rhel);
        assert_eq!(os_hint("Windows BYOL"), OsKind::Windows);
        assert_eq!(os_hint("ubuntu_22_04"), OsKind::Ubuntu);
        assert_eq!(os_hint("amazon_linux_2023"), OsKind::Amazon);
        assert_eq!(os_hint("0001-com-ubuntu-server-jammy"), OsKind::Ubuntu);
        assert_eq!(os_hint(""), OsKind::Unknown);
    }

    #[test]
    fn tags_and_labels_are_sanitized() {
        assert_eq!(sanitize_tag(" a,b\n "), Some("a;b".into()));
        assert_eq!(sanitize_tag("   "), None);
        let long = "x".repeat(100);
        assert_eq!(sanitize_tag(&long).map(|t| t.len()), Some(64));
        assert_eq!(kv_tag("env", "prod"), Some("env:prod".into()));
        assert_eq!(kv_tag("flag", ""), Some("flag".into()));
        assert_eq!(sanitize_label(&[Some(" \n"), Some("web\t1")], "id"), "web1");
        assert_eq!(sanitize_label(&[None], "i-1"), "i-1");
    }

    #[test]
    fn address_preference() {
        let draft = CloudInstance {
            cloud_id: "x".into(),
            label: "x".into(),
            address: String::new(),
            public_ip: None,
            private_ip: Some("10.0.0.1".into()),
            public_dns: None,
            tags: vec![],
            os: OsKind::Unknown,
            region: None,
            username_hint: None,
        };
        let mut inv = Inventory::default();
        inv.push(draft.clone(), AddressPreference::PublicThenPrivate);
        inv.push(draft.clone(), AddressPreference::Public);
        assert_eq!(inv.instances.len(), 1);
        assert_eq!(inv.instances[0].address, "10.0.0.1");
        assert_eq!(inv.warnings.len(), 1);
    }
}
