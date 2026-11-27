// SPDX-License-Identifier: GPL-3.0-or-later
//! Vault entities.

use crate::config::{Credential, HostConfig};
use crate::id::{
    GroupId, HostId, IdentityId, IntegrationId, KeyId, KnownHostId, PackageId, RuleId, SecretRef,
    SnippetId, WorkspaceId,
};
use serde::{Deserialize, Serialize};

/// Unix timestamp in milliseconds.
pub type Millis = i64;

/// Detected (or user-chosen) operating system, used for the host icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum OsKind {
    #[default]
    Unknown,
    Linux,
    Ubuntu,
    Debian,
    Fedora,
    Rhel,
    Centos,
    Rocky,
    Alma,
    Arch,
    Alpine,
    Amazon,
    Suse,
    Nixos,
    Raspbian,
    Freebsd,
    Openbsd,
    Netbsd,
    Macos,
    Windows,
    Cisco,
    Mikrotik,
    Synology,
    Truenas,
    Proxmox,
    Openwrt,
}

impl OsKind {
    /// Maps an `/etc/os-release` `ID` (or `uname -s`) value to an [`OsKind`].
    #[must_use]
    pub fn from_os_release_id(id: &str) -> Self {
        match id.trim().trim_matches('"').to_ascii_lowercase().as_str() {
            "ubuntu" | "pop" | "linuxmint" | "elementary" => Self::Ubuntu,
            "debian" => Self::Debian,
            "raspbian" => Self::Raspbian,
            "fedora" => Self::Fedora,
            "rhel" | "redhat" => Self::Rhel,
            "centos" => Self::Centos,
            "rocky" => Self::Rocky,
            "almalinux" | "alma" => Self::Alma,
            "arch" | "manjaro" | "endeavouros" => Self::Arch,
            "alpine" => Self::Alpine,
            "amzn" | "amazon" => Self::Amazon,
            "opensuse" | "opensuse-leap" | "opensuse-tumbleweed" | "sles" | "suse" => Self::Suse,
            "nixos" => Self::Nixos,
            "freebsd" => Self::Freebsd,
            "openbsd" => Self::Openbsd,
            "netbsd" => Self::Netbsd,
            "darwin" | "macos" => Self::Macos,
            "windows" | "windows_nt" => Self::Windows,
            "openwrt" => Self::Openwrt,
            "proxmox" | "pve" => Self::Proxmox,
            "truenas" => Self::Truenas,
            "linux" => Self::Linux,
            _ => Self::Unknown,
        }
    }
}

/// A nestable collection of hosts. Groups are *not* folders: a group view shows hosts of all
/// its subgroups too, and every setting cascades down.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: GroupId,
    pub label: String,
    #[serde(default)]
    pub parent: Option<GroupId>,
    #[serde(default)]
    pub config: HostConfig,
    /// When set, the group is mirrored from a cloud integration.
    #[serde(default)]
    pub integration: Option<IntegrationId>,
    #[cfg_attr(feature = "specta", specta(type = f64))]
    #[serde(default)]
    pub created_at: Millis,
    #[cfg_attr(feature = "specta", specta(type = f64))]
    #[serde(default)]
    pub updated_at: Millis,
}

impl Group {
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: GroupId::new(),
            label: label.into(),
            parent: None,
            config: HostConfig::default(),
            integration: None,
            created_at: 0,
            updated_at: 0,
        }
    }
}

/// A remote machine reachable through one or more protocols.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Host {
    pub id: HostId,
    pub label: String,
    /// Hostname, IPv4 or IPv6 address.
    pub address: String,
    #[serde(default)]
    pub group: Option<GroupId>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub config: HostConfig,
    #[serde(default)]
    pub os: OsKind,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub notes: Option<String>,
    /// Stable id of the instance in a cloud provider, when imported by an integration.
    #[serde(default)]
    pub cloud_id: Option<String>,
    #[cfg_attr(feature = "specta", specta(type = f64))]
    #[serde(default)]
    pub created_at: Millis,
    #[cfg_attr(feature = "specta", specta(type = f64))]
    #[serde(default)]
    pub updated_at: Millis,
    #[cfg_attr(feature = "specta", specta(type = Option<f64>))]
    #[serde(default)]
    pub last_connected_at: Option<Millis>,
}

impl Host {
    #[must_use]
    pub fn new(label: impl Into<String>, address: impl Into<String>) -> Self {
        Self {
            id: HostId::new(),
            label: label.into(),
            address: address.into(),
            group: None,
            tags: Vec::new(),
            config: HostConfig::default(),
            os: OsKind::Unknown,
            favorite: false,
            notes: None,
            cloud_id: None,
            created_at: 0,
            updated_at: 0,
            last_connected_at: None,
        }
    }

    /// Returns `true` if the host has *all* of the given tags (case-insensitive AND filter).
    #[must_use]
    pub fn has_all_tags<S: AsRef<str>>(&self, tags: &[S]) -> bool {
        tags.iter().all(|wanted| {
            self.tags
                .iter()
                .any(|t| t.eq_ignore_ascii_case(wanted.as_ref()))
        })
    }

    /// Case-insensitive search across label, address and tags.
    #[must_use]
    pub fn matches_query(&self, query: &str) -> bool {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        q.split_whitespace().all(|term| {
            self.label.to_lowercase().contains(term)
                || self.address.to_lowercase().contains(term)
                || self.tags.iter().any(|t| t.to_lowercase().contains(term))
        })
    }
}

/// A reusable bundle of username + password + keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub id: IdentityId,
    pub label: String,
    pub username: String,
    #[serde(default)]
    pub password: Option<SecretRef>,
    #[serde(default)]
    pub keys: Vec<KeyId>,
    #[cfg_attr(feature = "specta", specta(type = f64))]
    #[serde(default)]
    pub created_at: Millis,
    #[cfg_attr(feature = "specta", specta(type = f64))]
    #[serde(default)]
    pub updated_at: Millis,
}

/// Algorithm / storage kind of a keychain key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum KeyAlgorithm {
    Ed25519,
    EcdsaP256,
    EcdsaP384,
    EcdsaP521,
    Rsa,
    /// FIDO2 security key, `sk-ssh-ed25519@openssh.com`.
    SkEd25519,
    /// FIDO2 security key, `sk-ecdsa-sha2-nistp256@openssh.com`.
    SkEcdsaP256,
}

impl KeyAlgorithm {
    /// The SSH public key algorithm name.
    #[must_use]
    pub const fn ssh_name(self) -> &'static str {
        match self {
            Self::Ed25519 => "ssh-ed25519",
            Self::EcdsaP256 => "ecdsa-sha2-nistp256",
            Self::EcdsaP384 => "ecdsa-sha2-nistp384",
            Self::EcdsaP521 => "ecdsa-sha2-nistp521",
            Self::Rsa => "rsa-sha2-512",
            Self::SkEd25519 => "sk-ssh-ed25519@openssh.com",
            Self::SkEcdsaP256 => "sk-ecdsa-sha2-nistp256@openssh.com",
        }
    }
}

/// Where the private half of a key lives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum KeyStorage {
    /// OpenSSH private key encrypted inside the vault. Synced.
    Vault { secret: SecretRef },
    /// Key handle for a FIDO2 authenticator (non-resident). The handle is synced; the
    /// authenticator is required to sign.
    Fido2 {
        secret: SecretRef,
        application: String,
        require_pin: bool,
        require_presence: bool,
    },
    /// Non-exportable key inside the platform secure hardware (Secure Enclave, TPM via
    /// Windows Hello, Android Keystore/StrongBox). Never synced.
    Hardware {
        platform_ref: String,
        device: String,
    },
}

/// Public metadata about a key. Private material is never part of this struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct KeyMeta {
    pub id: KeyId,
    pub label: String,
    pub algorithm: KeyAlgorithm,
    /// Bits for RSA keys.
    #[serde(default)]
    pub bits: Option<u32>,
    /// Public key in OpenSSH `authorized_keys` format.
    pub public_key: String,
    /// `SHA256:...` fingerprint.
    pub fingerprint: String,
    pub storage: KeyStorage,
    /// Whether the private key is protected by a passphrase.
    #[serde(default)]
    pub encrypted: bool,
    /// Stored passphrase (when the user chose "save passphrase").
    #[serde(default)]
    pub passphrase: Option<SecretRef>,
    /// OpenSSH certificate (`*-cert.pub` contents) paired with this key.
    #[serde(default)]
    pub certificate: Option<String>,
    #[cfg_attr(feature = "specta", specta(type = f64))]
    #[serde(default)]
    pub created_at: Millis,
    #[cfg_attr(feature = "specta", specta(type = f64))]
    #[serde(default)]
    pub updated_at: Millis,
}

impl KeyMeta {
    /// Hardware-bound keys cannot leave the device and are excluded from sync.
    #[must_use]
    pub const fn is_device_bound(&self) -> bool {
        matches!(self.storage, KeyStorage::Hardware { .. })
    }
}

/// A folder-like collection of snippets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SnippetPackage {
    pub id: PackageId,
    pub label: String,
    #[cfg_attr(feature = "specta", specta(type = f64))]
    #[serde(default)]
    pub created_at: Millis,
}

/// A named shell script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: SnippetId,
    pub label: String,
    pub script: String,
    #[serde(default)]
    pub package: Option<PackageId>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Hosts remembered by "save targets" for multi-host execution.
    #[serde(default)]
    pub targets: Vec<HostId>,
    #[serde(default)]
    pub run_count: u32,
    #[cfg_attr(feature = "specta", specta(type = f64))]
    #[serde(default)]
    pub created_at: Millis,
    #[cfg_attr(feature = "specta", specta(type = f64))]
    #[serde(default)]
    pub updated_at: Millis,
}

impl Snippet {
    /// Distinct `{{variable}}` placeholders in order of first appearance.
    ///
    /// A variable name is `[A-Za-z_][A-Za-z0-9_]*`, optionally followed by `:default`.
    #[must_use]
    pub fn variables(&self) -> Vec<SnippetVariable> {
        let mut out: Vec<SnippetVariable> = Vec::new();
        let s = self.script.as_str();
        let mut rest = s;
        while let Some(start) = rest.find("{{") {
            let after = &rest[start + 2..];
            let Some(end) = after.find("}}") else { break };
            let inner = after[..end].trim();
            let (name, default) = match inner.split_once(':') {
                Some((n, d)) => (n.trim(), Some(d.trim().to_owned())),
                None => (inner, None),
            };
            let valid = name
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if valid && !out.iter().any(|v| v.name == name) {
                out.push(SnippetVariable {
                    name: name.to_owned(),
                    default,
                });
            }
            rest = &after[end + 2..];
        }
        out
    }

    /// Substitutes `{{variables}}` with the given values (falling back to defaults).
    /// Unknown placeholders are left untouched.
    #[must_use]
    pub fn render(&self, values: &[(String, String)]) -> String {
        let mut out = String::with_capacity(self.script.len());
        let mut rest = self.script.as_str();
        let vars = self.variables();
        while let Some(start) = rest.find("{{") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let Some(end) = after.find("}}") else {
                out.push_str(&rest[start..]);
                return out;
            };
            let inner = after[..end].trim();
            let name = inner.split_once(':').map_or(inner, |(n, _)| n.trim());
            let value = values
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
                .or_else(|| {
                    vars.iter()
                        .find(|v| v.name == name)
                        .and_then(|v| v.default.clone())
                });
            match value {
                Some(v) if vars.iter().any(|x| x.name == name) => out.push_str(&v),
                _ => {
                    out.push_str("{{");
                    out.push_str(&after[..end]);
                    out.push_str("}}");
                }
            }
            rest = &after[end + 2..];
        }
        out.push_str(rest);
        out
    }
}

/// A `{{name:default}}` placeholder inside a snippet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SnippetVariable {
    pub name: String,
    pub default: Option<String>,
}

/// Kind of port forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum ForwardKind {
    /// `-L`: reach a remote service from this device.
    Local,
    /// `-R`: expose a local service on the remote host.
    Remote,
    /// `-D`: SOCKS5 proxy through the remote host.
    Dynamic,
}

/// A saved tunnel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PortForwardRule {
    pub id: RuleId,
    pub label: String,
    pub kind: ForwardKind,
    pub host: HostId,
    /// Listen address (local for L/D, remote for R).
    pub bind_address: String,
    pub bind_port: u16,
    /// Destination host (not used for dynamic).
    #[serde(default)]
    pub dest_host: Option<String>,
    #[serde(default)]
    pub dest_port: Option<u16>,
    #[serde(default)]
    pub auto_start: bool,
    #[cfg_attr(feature = "specta", specta(type = f64))]
    #[serde(default)]
    pub created_at: Millis,
    #[cfg_attr(feature = "specta", specta(type = f64))]
    #[serde(default)]
    pub updated_at: Millis,
}

impl PortForwardRule {
    /// The equivalent OpenSSH command-line flag, e.g. `-L 127.0.0.1:3306:db:3306`.
    #[must_use]
    pub fn openssh_flag(&self) -> String {
        let bind = format_bind(&self.bind_address, self.bind_port);
        match self.kind {
            ForwardKind::Dynamic => format!("-D {bind}"),
            ForwardKind::Local | ForwardKind::Remote => {
                let flag = if self.kind == ForwardKind::Local {
                    "-L"
                } else {
                    "-R"
                };
                let host = self.dest_host.as_deref().unwrap_or("localhost");
                let host = if host.contains(':') {
                    format!("[{host}]")
                } else {
                    host.to_owned()
                };
                format!("{flag} {bind}:{host}:{}", self.dest_port.unwrap_or(0))
            }
        }
    }
}

fn format_bind(addr: &str, port: u16) -> String {
    if addr.contains(':') {
        format!("[{addr}]:{port}")
    } else {
        format!("{addr}:{port}")
    }
}

/// Marker on a known-hosts line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum KnownHostMarker {
    /// The key is a CA that signs host certificates for matching hosts.
    CertAuthority,
    /// The key is revoked and must never be accepted.
    Revoked,
}

/// A trusted (or revoked) host public key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct KnownHost {
    pub id: KnownHostId,
    /// Host pattern(s) as in `known_hosts`: `example.com`, `[example.com]:2222`, `*.corp`.
    pub pattern: String,
    pub key_type: String,
    /// Base64 public key blob.
    pub public_key: String,
    /// `SHA256:...` fingerprint.
    pub fingerprint: String,
    #[serde(default)]
    pub marker: Option<KnownHostMarker>,
    #[serde(default)]
    pub comment: Option<String>,
    #[cfg_attr(feature = "specta", specta(type = f64))]
    #[serde(default)]
    pub added_at: Millis,
}

impl KnownHost {
    /// Whether this entry applies to `host:port`.
    #[must_use]
    pub fn matches(&self, host: &str, port: u16) -> bool {
        let target = if port == 22 {
            host.to_ascii_lowercase()
        } else {
            format!("[{}]:{port}", host.to_ascii_lowercase())
        };
        let mut matched = false;
        for pat in self.pattern.split(',') {
            let pat = pat.trim();
            if let Some(neg) = pat.strip_prefix('!') {
                if glob_match(&neg.to_ascii_lowercase(), &target) {
                    return false;
                }
            } else if glob_match(&pat.to_ascii_lowercase(), &target) {
                matched = true;
            }
        }
        matched
    }
}

/// `known_hosts`/`ssh_config` style glob matching with `*` and `?`.
#[must_use]
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Workspace layout mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceMode {
    #[default]
    Split,
    Focus,
}

/// One pane in a workspace template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePane {
    pub host: HostId,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
}

/// A saved set of sessions with their layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTemplate {
    pub id: WorkspaceId,
    pub label: String,
    pub mode: WorkspaceMode,
    pub panes: Vec<WorkspacePane>,
    /// Column fractions for the grid layout (sum to 1.0).
    #[serde(default)]
    pub column_sizes: Vec<f32>,
    #[cfg_attr(feature = "specta", specta(type = f64))]
    #[serde(default)]
    pub created_at: Millis,
    #[cfg_attr(feature = "specta", specta(type = f64))]
    #[serde(default)]
    pub updated_at: Millis,
}

/// Supported cloud inventory providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum CloudProvider {
    Aws,
    Digitalocean,
    Azure,
}

/// Configuration of a cloud inventory integration. Credentials are stored as secrets and
/// marked device-local (never synced) unless the user opts in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CloudIntegration {
    pub id: IntegrationId,
    pub label: String,
    pub provider: CloudProvider,
    pub group: GroupId,
    /// Provider-specific, non-secret settings (region, service, ip type, subscription...).
    /// Values are strings by convention.
    #[serde(default)]
    #[cfg_attr(
        feature = "specta",
        specta(type = std::collections::BTreeMap<String, String>)
    )]
    pub settings: serde_json::Map<String, serde_json::Value>,
    pub credentials: SecretRef,
    #[serde(default)]
    pub sync_credentials: bool,
    #[cfg_attr(feature = "specta", specta(type = Option<f64>))]
    #[serde(default)]
    pub last_sync_at: Option<Millis>,
    #[serde(default)]
    pub last_error: Option<String>,
}

/// Convenience accessor for the effective credential of an SSH config.
#[must_use]
pub fn credential_username<'a>(
    cred: Option<&'a Credential>,
    identities: &'a [Identity],
) -> Option<&'a str> {
    match cred? {
        Credential::Identity { id } => identities
            .iter()
            .find(|i| &i.id == id)
            .map(|i| i.username.as_str()),
        Credential::Inline { username, .. } => username.as_deref(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_release_mapping() {
        assert_eq!(OsKind::from_os_release_id("\"ubuntu\""), OsKind::Ubuntu);
        assert_eq!(OsKind::from_os_release_id("amzn"), OsKind::Amazon);
        assert_eq!(OsKind::from_os_release_id("whatever"), OsKind::Unknown);
    }

    #[test]
    fn tag_filter_is_and_and_case_insensitive() {
        let mut h = Host::new("db", "10.0.0.1");
        h.tags = vec!["prod".into(), "DB".into()];
        assert!(h.has_all_tags(&["prod", "db"]));
        assert!(!h.has_all_tags(&["prod", "web"]));
        assert!(h.matches_query("PRO 10.0"));
        assert!(!h.matches_query("staging"));
    }

    #[test]
    fn snippet_variables_and_render() {
        let s = Snippet {
            id: SnippetId::new(),
            label: "restart".into(),
            script:
                "sudo systemctl restart {{service:nginx}} && echo {{ msg }} {{service}} {{1bad}}"
                    .into(),
            package: None,
            tags: vec![],
            targets: vec![],
            run_count: 0,
            created_at: 0,
            updated_at: 0,
        };
        let vars = s.variables();
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].name, "service");
        assert_eq!(vars[0].default.as_deref(), Some("nginx"));
        let out = s.render(&[("msg".into(), "done".into())]);
        assert_eq!(
            out,
            "sudo systemctl restart nginx && echo done nginx {{1bad}}"
        );
    }

    #[test]
    fn forward_rule_flags() {
        let mut r = PortForwardRule {
            id: RuleId::new(),
            label: "db".into(),
            kind: ForwardKind::Local,
            host: HostId::new(),
            bind_address: "127.0.0.1".into(),
            bind_port: 3336,
            dest_host: Some("10.1.30.42".into()),
            dest_port: Some(3306),
            auto_start: false,
            created_at: 0,
            updated_at: 0,
        };
        assert_eq!(r.openssh_flag(), "-L 127.0.0.1:3336:10.1.30.42:3306");
        r.kind = ForwardKind::Dynamic;
        r.bind_address = "::1".into();
        assert_eq!(r.openssh_flag(), "-D [::1]:3336");
    }

    #[test]
    fn known_host_patterns() {
        let kh = KnownHost {
            id: KnownHostId::new(),
            pattern: "*.example.com,!bad.example.com,[jump.example.org]:2222".into(),
            key_type: "ssh-ed25519".into(),
            public_key: String::new(),
            fingerprint: String::new(),
            marker: None,
            comment: None,
            added_at: 0,
        };
        assert!(kh.matches("web.example.com", 22));
        assert!(kh.matches("WEB.Example.com", 22));
        assert!(!kh.matches("bad.example.com", 22));
        assert!(kh.matches("jump.example.org", 2222));
        assert!(!kh.matches("jump.example.org", 22));
        assert!(!kh.matches("web.example.com", 2222));
    }

    #[test]
    fn glob() {
        assert!(glob_match("a*c", "abbbc"));
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("a?c", "abbc"));
        assert!(glob_match("*", ""));
    }
}
