// SPDX-License-Identifier: GPL-3.0-or-later
//! The result of an import: entities, placeholders for secrets and keys, and warnings.

use core_model::{Group, Host, Identity, KeyId, KnownHost, PortForwardRule, SecretRef};
use serde::{Deserialize, Serialize};
use std::fmt;
use zeroize::Zeroizing;

/// Everything produced by one import.
///
/// Entities reference each other by id exactly as they will once persisted:
///
/// * hosts reference [`ImportBundle::groups`] through [`Host::group`],
/// * hosts reference [`ImportBundle::keys`] through the [`KeyId`]s in their
///   [`core_model::Credential::Inline`] credential ([`ImportedKey::id`]),
/// * hosts (and proxies) reference [`ImportBundle::secrets`] through [`SecretRef`]s that are
///   already set in their configuration ([`ImportedSecret::slot`]).
///
/// The caller stores each secret in the vault under its slot id (or under a fresh id and then
/// patches the references), loads key files named by [`ImportedKey::source_path`], and
/// persists the rest. Nothing in a bundle has been written anywhere yet.
#[derive(Debug, Default)]
pub struct ImportBundle {
    pub groups: Vec<Group>,
    pub hosts: Vec<Host>,
    pub identities: Vec<Identity>,
    pub keys: Vec<ImportedKey>,
    pub known_hosts: Vec<KnownHost>,
    pub forwards: Vec<PortForwardRule>,
    pub secrets: Vec<ImportedSecret>,
    pub warnings: Vec<ImportWarning>,
}

impl ImportBundle {
    /// Returns `true` if the bundle contains no entities (warnings are not counted).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
            && self.hosts.is_empty()
            && self.identities.is_empty()
            && self.keys.is_empty()
            && self.known_hosts.is_empty()
            && self.forwards.is_empty()
            && self.secrets.is_empty()
    }

    /// Records a warning.
    pub(crate) fn warn(
        &mut self,
        kind: WarningKind,
        line: Option<usize>,
        item: Option<&str>,
        message: impl Into<String>,
    ) {
        if self.warnings.len() >= MAX_WARNINGS {
            return;
        }
        self.warnings.push(ImportWarning {
            kind,
            line,
            item: item.map(|s| truncate_for_display(s, 120)),
            message: message.into(),
        });
    }

    /// Returns the key placeholder for `path`, creating it on first use.
    pub(crate) fn key_for_path(&mut self, path: &str, format: KeyFileFormat) -> KeyId {
        if let Some(k) = self
            .keys
            .iter()
            .find(|k| k.source_path.as_deref() == Some(path))
        {
            return k.id;
        }
        let label = path
            .rsplit(['/', '\\'])
            .find(|s| !s.is_empty())
            .unwrap_or(path)
            .to_owned();
        let key = ImportedKey {
            id: KeyId::new(),
            label,
            source_path: Some(path.to_owned()),
            format,
            material: KeyMaterial::Unloaded,
        };
        let id = key.id;
        self.keys.push(key);
        id
    }

    /// Stores a secret value in a new placeholder slot and returns the slot reference.
    pub(crate) fn add_secret(
        &mut self,
        kind: SecretKind,
        description: impl Into<String>,
        value: Zeroizing<Vec<u8>>,
    ) -> SecretRef {
        let slot = SecretRef::new();
        self.secrets.push(ImportedSecret {
            slot,
            kind,
            description: description.into(),
            value,
        });
        slot
    }
}

/// Upper bound on the number of warnings kept per import, so hostile input cannot make the
/// warning list itself unbounded in practice.
pub(crate) const MAX_WARNINGS: usize = 10_000;

/// Truncates user-controlled text that is echoed into warnings.
pub(crate) fn truncate_for_display(s: &str, max_chars: usize) -> String {
    let mut out: String = s
        .chars()
        .filter(|c| !c.is_control())
        .take(max_chars)
        .collect();
    if s.chars().count() > max_chars {
        out.push('…');
    }
    out
}

/// File format of a referenced key file, as far as the source told us.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyFileFormat {
    /// OpenSSH (or PEM/PKCS#8) private key, as referenced by `IdentityFile`.
    OpenSsh,
    /// PuTTY `.ppk` private key; must be converted by the caller.
    Ppk,
    /// Unknown format; the caller should sniff the file contents.
    Unknown,
}

/// The key material of an [`ImportedKey`].
#[derive(Clone)]
pub enum KeyMaterial {
    /// Only a path is known; the caller must read [`ImportedKey::source_path`].
    Unloaded,
    /// A public key in OpenSSH `authorized_keys` format (no private half available).
    Public { openssh: String },
    /// A private key in PEM / OpenSSH format.
    Private { pem: Zeroizing<String> },
}

impl fmt::Debug for KeyMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unloaded => f.write_str("Unloaded"),
            Self::Public { openssh } => f.debug_struct("Public").field("openssh", openssh).finish(),
            Self::Private { .. } => f
                .debug_struct("Private")
                .field("pem", &"<redacted>")
                .finish(),
        }
    }
}

/// A key referenced by imported hosts.
///
/// [`ImportedKey::id`] is already used in host credentials; the caller should create the
/// keychain entry ([`core_model::KeyMeta`]) under the same id.
#[derive(Debug, Clone)]
pub struct ImportedKey {
    pub id: KeyId,
    pub label: String,
    /// Path as written in the source configuration (unexpanded: may contain `~`, `%d`,
    /// `_ProfileDir_`, environment variables...).
    pub source_path: Option<String>,
    pub format: KeyFileFormat,
    pub material: KeyMaterial,
}

/// What an [`ImportedSecret`] is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SecretKind {
    /// Login password for a host.
    Password,
    /// Password for an HTTP/SOCKS proxy.
    ProxyPassword,
}

/// A secret value waiting to be stored in the vault.
///
/// [`ImportedSecret::slot`] is already referenced from the bundle's entities. `Debug` never
/// prints the value.
#[derive(Clone)]
pub struct ImportedSecret {
    pub slot: SecretRef,
    pub kind: SecretKind,
    /// Non-secret, human readable description (e.g. `password for web01`).
    pub description: String,
    pub value: Zeroizing<Vec<u8>>,
}

impl fmt::Debug for ImportedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImportedSecret")
            .field("slot", &self.slot)
            .field("kind", &self.kind)
            .field("description", &self.description)
            .field("value", &"<redacted>")
            .finish()
    }
}

/// Category of an [`ImportWarning`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WarningKind {
    /// A feature of the source format that has no equivalent here; the setting was ignored.
    Unsupported,
    /// A line, value or entity was malformed or failed validation and was skipped.
    Invalid,
    /// An entity was a duplicate of an earlier one and was merged into it.
    Duplicate,
    /// A stored secret (usually an encrypted password) could not be imported.
    SecretNotImported,
    /// An `Include` could not be resolved or exceeded the nesting limit.
    Include,
    /// A safety limit (size, depth, count) was reached; some input was ignored.
    Limit,
}

/// A non-fatal problem encountered during import. Never contains secret values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportWarning {
    pub kind: WarningKind,
    /// 1-based line number in the source, when meaningful.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// The session / host / entry the warning is about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    pub message: String,
}

impl fmt::Display for ImportWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(line) = self.line {
            write!(f, "line {line}: ")?;
        }
        if let Some(item) = &self.item {
            write!(f, "{item}: ")?;
        }
        f.write_str(&self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_redacted_in_debug() {
        let mut b = ImportBundle::default();
        b.add_secret(
            SecretKind::Password,
            "password for web",
            Zeroizing::new(b"hunter2".to_vec()),
        );
        b.keys.push(ImportedKey {
            id: KeyId::new(),
            label: "k".into(),
            source_path: None,
            format: KeyFileFormat::OpenSsh,
            material: KeyMaterial::Private {
                pem: Zeroizing::new("-----BEGIN OPENSSH PRIVATE KEY-----secretbits".into()),
            },
        });
        let dbg = format!("{b:?}");
        assert!(!dbg.contains("hunter2"));
        assert!(!dbg.contains("secretbits"));
        assert!(dbg.contains("<redacted>"));
    }

    #[test]
    fn key_placeholders_are_deduplicated_by_path() {
        let mut b = ImportBundle::default();
        let a = b.key_for_path("~/.ssh/id_ed25519", KeyFileFormat::OpenSsh);
        let c = b.key_for_path("~/.ssh/id_ed25519", KeyFileFormat::OpenSsh);
        let d = b.key_for_path(r"C:\keys\work.ppk", KeyFileFormat::Ppk);
        assert_eq!(a, c);
        assert_ne!(a, d);
        assert_eq!(b.keys.len(), 2);
        assert_eq!(b.keys[0].label, "id_ed25519");
        assert_eq!(b.keys[1].label, "work.ppk");
    }

    #[test]
    fn warning_display_and_truncation() {
        let mut b = ImportBundle::default();
        b.warn(
            WarningKind::Invalid,
            Some(3),
            Some(&"x".repeat(500)),
            "bad port",
        );
        let w = &b.warnings[0];
        assert_eq!(w.item.as_ref().map(|s| s.chars().count()), Some(121));
        assert!(w.to_string().starts_with("line 3: xxx"));
        assert!(w.to_string().ends_with(": bad port"));
    }
}
