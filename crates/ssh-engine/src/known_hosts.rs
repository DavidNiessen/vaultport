// SPDX-License-Identifier: GPL-3.0-or-later
//! Strict known-hosts verification.
//!
//! The app owns storage (the vault); this module only decides. Rules, in order:
//!
//! 1. Entries are selected with [`KnownHost::matches`] (plus OpenSSH hashed `|1|salt|hash`
//!    patterns, see [`entry_matches`]).
//! 2. If the presented key — or, for a host certificate, the certified key or the CA key — is
//!    listed under `@revoked`, the connection fails **without** asking the user.
//! 3. A host certificate signed by a matching `@cert-authority` key is accepted when it is a
//!    *host* certificate, currently valid, its signature verifies, it names the host among its
//!    principals and carries no critical options.
//! 4. A plain entry with the same key data → trusted.
//! 5. Plain entries exist but none matches → [`HostKeyStatus::Changed`] (any key type: the
//!    algorithm negotiation prefers already-known key types, see
//!    [`crate::algorithms::preferred`], so a server presenting another type is suspicious).
//! 6. Otherwise → [`HostKeyStatus::Unknown`].

use crate::fingerprint::{randomart, sha256_fingerprint};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use core_model::{KnownHost, KnownHostId, KnownHostMarker};
use hmac::{Hmac, KeyInit, Mac};
use russh::keys::ssh_key::certificate::CertType;
use russh::keys::ssh_key::public::KeyData;
use russh::keys::ssh_key::{Algorithm, Certificate, HashAlg, PublicKey};
use sha1::Sha1;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Storage of trusted host keys, implemented by the app (backed by the vault).
pub trait KnownHosts: Send + Sync {
    /// All entries that *might* apply to `host:port`. Returning more entries than necessary is
    /// fine (they are filtered again with [`entry_matches`]); returning all entries is valid.
    fn lookup(&self, host: &str, port: u16) -> Vec<KnownHost>;
    /// Persist a newly trusted key (user chose [`crate::HostKeyDecision::AcceptAndSave`]).
    fn save(&self, entry: KnownHost);
    /// Called before [`Self::save`] when the user accepted a *changed* key: the previously
    /// trusted entries that no longer match. Default: keep them.
    fn forget(&self, _previous: &[KnownHost]) {}
}

/// Simple in-memory [`KnownHosts`] (tests, ephemeral sessions).
#[derive(Debug, Default)]
pub struct MemoryKnownHosts {
    entries: Mutex<Vec<KnownHost>>,
}

impl MemoryKnownHosts {
    #[must_use]
    pub fn new(entries: Vec<KnownHost>) -> Self {
        Self {
            entries: Mutex::new(entries),
        }
    }

    /// Snapshot of all entries.
    #[must_use]
    pub fn entries(&self) -> Vec<KnownHost> {
        self.entries.lock().map(|e| e.clone()).unwrap_or_default()
    }
}

impl KnownHosts for MemoryKnownHosts {
    fn lookup(&self, _host: &str, _port: u16) -> Vec<KnownHost> {
        self.entries()
    }

    fn save(&self, entry: KnownHost) {
        if let Ok(mut e) = self.entries.lock() {
            e.push(entry);
        }
    }

    fn forget(&self, previous: &[KnownHost]) {
        if let Ok(mut e) = self.entries.lock() {
            e.retain(|x| !previous.iter().any(|p| p.id == x.id));
        }
    }
}

/// Status of a presented host key that is not (yet) trusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyStatus {
    /// No entry for this host.
    Unknown,
    /// The host is known with different key(s). **Possible man-in-the-middle attack** — the UI
    /// must show a prominent warning and should default to rejecting.
    Changed {
        /// `SHA256:` fingerprints of the previously trusted keys.
        previous_fingerprints: Vec<String>,
    },
    /// The key is revoked. Informational only: the connection is refused regardless of the
    /// decision returned by the interaction callback.
    Revoked,
}

/// Summary of a host certificate, for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostCertificateInfo {
    pub key_id: String,
    pub principals: Vec<String>,
    pub ca_fingerprint: String,
    pub valid_after: u64,
    pub valid_before: u64,
    /// Why it was not accepted by any `@cert-authority` entry (if it was checked at all).
    pub problem: Option<String>,
}

/// Everything the UI needs to ask "do you trust this host?".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKeyCheck {
    /// 0-based index of the hop in the chain (jump hosts first).
    pub hop_index: usize,
    pub host: String,
    pub port: u16,
    /// SSH key type, e.g. `ssh-ed25519`.
    pub key_type: String,
    /// `SHA256:...`
    pub fingerprint: String,
    /// OpenSSH randomart (multi-line).
    pub randomart: String,
    /// OpenSSH public key line (`type base64`), suitable for a known-hosts entry.
    pub public_key: String,
    /// Present when the server offered a host certificate.
    pub certificate: Option<HostCertificateInfo>,
    pub status: HostKeyStatus,
}

/// How a key was trusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustedBy {
    /// A plain known-hosts entry.
    Key(KnownHostId),
    /// A `@cert-authority` entry that signed the presented host certificate.
    CertificateAuthority(KnownHostId),
}

/// Result of [`evaluate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Trusted(TrustedBy),
    Untrusted {
        status: HostKeyStatus,
        /// Plain entries that would be replaced if the user accepts a changed key.
        mismatched: Vec<KnownHost>,
        certificate_problem: Option<String>,
    },
}

/// A host key or host certificate presented during key exchange.
#[derive(Debug, Clone)]
pub enum Presented<'a> {
    Key(&'a PublicKey),
    Certificate(&'a Certificate),
}

impl Presented<'_> {
    /// The key that signed the key exchange.
    #[must_use]
    pub fn key_data(&self) -> &KeyData {
        match self {
            Self::Key(k) => k.key_data(),
            Self::Certificate(c) => c.public_key(),
        }
    }
}

/// Whether `entry` applies to `host:port`, supporting OpenSSH hashed hostnames
/// (`|1|base64(salt)|base64(HMAC-SHA1(salt, host))`).
#[must_use]
pub fn entry_matches(entry: &KnownHost, host: &str, port: u16) -> bool {
    if entry.pattern.starts_with("|1|") {
        return hashed_matches(&entry.pattern, host, port);
    }
    entry.matches(host, port)
}

fn hashed_matches(pattern: &str, host: &str, port: u16) -> bool {
    let mut parts = pattern.trim_start_matches("|1|").splitn(2, '|');
    let (Some(salt), Some(hash)) = (parts.next(), parts.next()) else {
        return false;
    };
    let (Ok(salt), Ok(hash)) = (B64.decode(salt), B64.decode(hash.trim())) else {
        return false;
    };
    let name = known_hosts_name(host, port);
    let Ok(mut mac) = <Hmac<Sha1> as KeyInit>::new_from_slice(&salt) else {
        return false;
    };
    mac.update(name.as_bytes());
    mac.verify_slice(&hash).is_ok()
}

/// `host` for port 22, `[host]:port` otherwise (lower-cased), as written to `known_hosts`.
#[must_use]
pub fn known_hosts_name(host: &str, port: u16) -> String {
    let host = host.to_ascii_lowercase();
    if port == 22 {
        host
    } else {
        format!("[{host}]:{port}")
    }
}

fn entry_key(entry: &KnownHost) -> Option<KeyData> {
    let line = format!("{} {}", entry.key_type, entry.public_key);
    PublicKey::from_openssh(&line)
        .ok()
        .map(|k| k.key_data().clone())
}

/// Key algorithms of the plain entries matching the host (for algorithm ordering).
#[must_use]
pub fn known_algorithms(entries: &[KnownHost], host: &str, port: u16) -> Vec<Algorithm> {
    entries
        .iter()
        .filter(|e| e.marker.is_none() && entry_matches(e, host, port))
        .filter_map(entry_key)
        .map(|k| k.algorithm())
        .collect()
}

/// Whether a `@cert-authority` entry exists for the host.
#[must_use]
pub fn has_cert_authority(entries: &[KnownHost], host: &str, port: u16) -> bool {
    entries
        .iter()
        .any(|e| e.marker == Some(KnownHostMarker::CertAuthority) && entry_matches(e, host, port))
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Decides whether `presented` is trusted for `host:port` given `entries`.
#[must_use]
pub fn evaluate(
    presented: &Presented<'_>,
    entries: &[KnownHost],
    host: &str,
    port: u16,
) -> Verdict {
    evaluate_at(presented, entries, host, port, now_unix())
}

/// [`evaluate`] at a fixed Unix time (for tests).
#[must_use]
pub fn evaluate_at(
    presented: &Presented<'_>,
    entries: &[KnownHost],
    host: &str,
    port: u16,
    now: u64,
) -> Verdict {
    let relevant: Vec<&KnownHost> = entries
        .iter()
        .filter(|e| entry_matches(e, host, port))
        .collect();
    let key = presented.key_data();

    // 2. Revocations: never overridable.
    let mut candidates = vec![key.clone()];
    if let Presented::Certificate(c) = presented {
        candidates.push(c.signature_key().clone());
    }
    let revoked = relevant
        .iter()
        .filter(|e| e.marker == Some(KnownHostMarker::Revoked))
        .filter_map(|e| entry_key(e))
        .any(|k| candidates.contains(&k));
    if revoked {
        return Verdict::Untrusted {
            status: HostKeyStatus::Revoked,
            mismatched: Vec::new(),
            certificate_problem: None,
        };
    }

    // 3. Certificate authorities.
    let mut certificate_problem = None;
    if let Presented::Certificate(cert) = presented {
        let cas: Vec<(&KnownHost, KeyData)> = relevant
            .iter()
            .filter(|e| e.marker == Some(KnownHostMarker::CertAuthority))
            .filter_map(|e| entry_key(e).map(|k| (*e, k)))
            .collect();
        if cas.is_empty() {
            certificate_problem = Some("no @cert-authority entry for this host".to_owned());
        }
        for (entry, ca) in cas {
            if cert.signature_key() != &ca {
                certificate_problem = Some("certificate signed by an unknown CA".to_owned());
                continue;
            }
            match check_host_certificate(cert, &ca, host, now) {
                Ok(()) => return Verdict::Trusted(TrustedBy::CertificateAuthority(entry.id)),
                Err(problem) => certificate_problem = Some(problem),
            }
        }
    }

    // 4./5. Plain keys (for a certificate: the certified key, like OpenSSH).
    let plain: Vec<&KnownHost> = relevant
        .iter()
        .copied()
        .filter(|e| e.marker.is_none())
        .collect();
    if let Some(hit) = plain.iter().find(|e| entry_key(e).as_ref() == Some(key)) {
        return Verdict::Trusted(TrustedBy::Key(hit.id));
    }
    if plain.is_empty() {
        Verdict::Untrusted {
            status: HostKeyStatus::Unknown,
            mismatched: Vec::new(),
            certificate_problem,
        }
    } else {
        Verdict::Untrusted {
            status: HostKeyStatus::Changed {
                previous_fingerprints: plain.iter().map(|e| e.fingerprint.clone()).collect(),
            },
            mismatched: plain.into_iter().cloned().collect(),
            certificate_problem,
        }
    }
}

fn check_host_certificate(
    cert: &Certificate,
    ca: &KeyData,
    host: &str,
    now: u64,
) -> Result<(), String> {
    if cert.cert_type() != CertType::Host {
        return Err("not a host certificate".to_owned());
    }
    let ca_fp = ca.fingerprint(HashAlg::Sha256);
    cert.validate_at(now, [&ca_fp])
        .map_err(|_| "certificate expired, not yet valid or badly signed".to_owned())?;
    if !cert.critical_options().is_empty() {
        return Err("certificate has unsupported critical options".to_owned());
    }
    let host_lc = host.to_ascii_lowercase();
    let principal_ok = cert
        .valid_principals()
        .iter()
        .any(|p| core_model::glob_match(&p.to_ascii_lowercase(), &host_lc));
    if !principal_ok {
        return Err(format!("certificate is not valid for host {host}"));
    }
    Ok(())
}

/// Builds the [`HostKeyCheck`] shown to the user.
#[must_use]
pub fn host_key_check(
    hop_index: usize,
    host: &str,
    port: u16,
    presented: &Presented<'_>,
    status: HostKeyStatus,
    certificate_problem: Option<String>,
) -> HostKeyCheck {
    let key = presented.key_data();
    let certificate = match presented {
        Presented::Key(_) => None,
        Presented::Certificate(c) => Some(HostCertificateInfo {
            key_id: c.key_id().to_owned(),
            principals: c.valid_principals().to_vec(),
            ca_fingerprint: sha256_fingerprint(c.signature_key()),
            valid_after: c.valid_after(),
            valid_before: c.valid_before(),
            problem: certificate_problem,
        }),
    };
    HostKeyCheck {
        hop_index,
        host: host.to_owned(),
        port,
        key_type: key.algorithm().as_str().to_owned(),
        fingerprint: sha256_fingerprint(key),
        randomart: randomart(key),
        public_key: PublicKey::new(key.clone(), "")
            .to_openssh()
            .unwrap_or_default()
            .trim()
            .to_owned(),
        certificate,
        status,
    }
}

/// A new known-hosts entry for `host:port` and `key`.
#[must_use]
pub fn new_entry(host: &str, port: u16, key: &KeyData, comment: Option<String>) -> KnownHost {
    let line = PublicKey::new(key.clone(), "")
        .to_openssh()
        .unwrap_or_default();
    let mut parts = line.split_whitespace();
    let key_type = parts.next().unwrap_or_default().to_owned();
    let blob = parts.next().unwrap_or_default().to_owned();
    KnownHost {
        id: KnownHostId::new(),
        pattern: known_hosts_name(host, port),
        key_type,
        public_key: blob,
        fingerprint: sha256_fingerprint(key),
        marker: None,
        comment,
        added_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64),
    }
}

/// Parses an OpenSSH `known_hosts` file (for importing). Invalid lines are skipped.
#[must_use]
pub fn parse_openssh_known_hosts(text: &str) -> Vec<KnownHost> {
    text.lines().filter_map(parse_known_hosts_line).collect()
}

/// Parses one `known_hosts` line: `[@marker] patterns keytype base64 [comment]`.
#[must_use]
pub fn parse_known_hosts_line(line: &str) -> Option<KnownHost> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let mut fields = line.split_whitespace();
    let mut first = fields.next()?;
    let marker = match first {
        "@cert-authority" => Some(KnownHostMarker::CertAuthority),
        "@revoked" => Some(KnownHostMarker::Revoked),
        m if m.starts_with('@') => return None,
        _ => None,
    };
    if marker.is_some() {
        first = fields.next()?;
    }
    let key_type = fields.next()?;
    let blob = fields.next()?;
    let comment: Vec<&str> = fields.collect();
    let key = PublicKey::from_openssh(&format!("{key_type} {blob}")).ok()?;
    Some(KnownHost {
        id: KnownHostId::new(),
        pattern: first.to_owned(),
        key_type: key_type.to_owned(),
        public_key: blob.to_owned(),
        fingerprint: sha256_fingerprint(key.key_data()),
        marker,
        comment: (!comment.is_empty()).then(|| comment.join(" ")),
        added_at: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::ssh_key::PrivateKey;
    use russh::keys::ssh_key::certificate::Builder;

    fn rng() -> impl russh::keys::ssh_key::rand_core::CryptoRng {
        russh::keys::key::safe_rng()
    }

    fn ed25519() -> PrivateKey {
        PrivateKey::random(&mut rng(), Algorithm::Ed25519).expect("keygen")
    }

    fn entry(pattern: &str, key: &KeyData, marker: Option<KnownHostMarker>) -> KnownHost {
        let mut e = new_entry("x", 22, key, None);
        e.pattern = pattern.to_owned();
        e.marker = marker;
        e
    }

    fn host_cert(
        ca: &PrivateKey,
        key: &PrivateKey,
        principals: &[&str],
        kind: CertType,
    ) -> Certificate {
        let now = now_unix();
        let mut b =
            Builder::new_with_random_nonce(&mut rng(), key.public_key(), now - 60, now + 3600)
                .expect("builder");
        b.cert_type(kind).expect("type");
        b.key_id("test").expect("id");
        for p in principals {
            b.valid_principal(*p).expect("principal");
        }
        b.sign(ca).expect("sign")
    }

    #[test]
    fn unknown_trusted_changed() {
        let k1 = ed25519();
        let k2 = ed25519();
        let pk1 = k1.public_key();
        let p = Presented::Key(pk1);
        assert!(matches!(
            evaluate(&p, &[], "h", 22),
            Verdict::Untrusted {
                status: HostKeyStatus::Unknown,
                ..
            }
        ));
        let entries = vec![entry("h,other", pk1.key_data(), None)];
        assert!(matches!(
            evaluate(&p, &entries, "h", 22),
            Verdict::Trusted(TrustedBy::Key(_))
        ));
        // Same host on another port is a different host.
        assert!(matches!(
            evaluate(&p, &entries, "h", 2222),
            Verdict::Untrusted {
                status: HostKeyStatus::Unknown,
                ..
            }
        ));
        let changed = evaluate(&Presented::Key(k2.public_key()), &entries, "h", 22);
        match changed {
            Verdict::Untrusted {
                status:
                    HostKeyStatus::Changed {
                        previous_fingerprints,
                    },
                mismatched,
                ..
            } => {
                assert_eq!(
                    previous_fingerprints,
                    vec![sha256_fingerprint(pk1.key_data())]
                );
                assert_eq!(mismatched.len(), 1);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn revoked_wins() {
        let k = ed25519();
        let entries = vec![
            entry("h", k.public_key().key_data(), None),
            entry(
                "*",
                k.public_key().key_data(),
                Some(KnownHostMarker::Revoked),
            ),
        ];
        assert!(matches!(
            evaluate(&Presented::Key(k.public_key()), &entries, "h", 22),
            Verdict::Untrusted {
                status: HostKeyStatus::Revoked,
                ..
            }
        ));
    }

    #[test]
    fn cert_authority() {
        let ca = ed25519();
        let hostkey = ed25519();
        let cert = host_cert(&ca, &hostkey, &["*.example.com"], CertType::Host);
        let entries = vec![entry(
            "*.example.com",
            ca.public_key().key_data(),
            Some(KnownHostMarker::CertAuthority),
        )];
        let p = Presented::Certificate(&cert);
        assert!(matches!(
            evaluate(&p, &entries, "web.example.com", 22),
            Verdict::Trusted(TrustedBy::CertificateAuthority(_))
        ));
        // Host outside the CA pattern: falls back to plain keys → unknown.
        assert!(matches!(
            evaluate(&p, &entries, "web.example.org", 22),
            Verdict::Untrusted {
                status: HostKeyStatus::Unknown,
                ..
            }
        ));
        // Principal mismatch.
        let cert2 = host_cert(&ca, &hostkey, &["db.example.com"], CertType::Host);
        let v = evaluate(
            &Presented::Certificate(&cert2),
            &entries,
            "web.example.com",
            22,
        );
        assert!(matches!(
            v,
            Verdict::Untrusted {
                certificate_problem: Some(_),
                ..
            }
        ));
        // User certificate presented as host certificate.
        let cert3 = host_cert(&ca, &hostkey, &["web.example.com"], CertType::User);
        let v = evaluate(
            &Presented::Certificate(&cert3),
            &entries,
            "web.example.com",
            22,
        );
        assert!(matches!(v, Verdict::Untrusted { .. }));
        // Expired.
        let v = evaluate_at(&p, &entries, "web.example.com", 22, now_unix() + 7200);
        assert!(matches!(v, Verdict::Untrusted { .. }));
        // Revoked CA.
        let mut entries2 = entries.clone();
        entries2.push(entry(
            "*",
            ca.public_key().key_data(),
            Some(KnownHostMarker::Revoked),
        ));
        assert!(matches!(
            evaluate(&p, &entries2, "web.example.com", 22),
            Verdict::Untrusted {
                status: HostKeyStatus::Revoked,
                ..
            }
        ));
    }

    #[test]
    fn hashed_hostnames() {
        // `ssh-keygen -H` style entry for "example.com" built with a fixed salt.
        let salt = [7u8; 20];
        let mut mac = <Hmac<Sha1> as KeyInit>::new_from_slice(&salt).expect("hmac");
        mac.update(b"[example.com]:2222");
        let hash = mac.finalize().into_bytes();
        let k = ed25519();
        let pattern = format!("|1|{}|{}", B64.encode(salt), B64.encode(hash));
        let e = entry(&pattern, k.public_key().key_data(), None);
        assert!(entry_matches(&e, "example.com", 2222));
        assert!(entry_matches(&e, "EXAMPLE.com", 2222));
        assert!(!entry_matches(&e, "example.com", 22));
    }

    #[test]
    fn parse_lines() {
        let text = "\
# comment
github.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl
@cert-authority *.corp ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl corp ca
@weird x ssh-ed25519 AAAA
broken line
";
        let v = parse_openssh_known_hosts(text);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].pattern, "github.com");
        assert_eq!(
            v[0].fingerprint,
            "SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU"
        );
        assert_eq!(v[1].marker, Some(KnownHostMarker::CertAuthority));
        assert_eq!(v[1].comment.as_deref(), Some("corp ca"));
        assert_eq!(
            known_algorithms(&v, "github.com", 22),
            vec![Algorithm::Ed25519]
        );
        assert!(has_cert_authority(&v, "a.corp", 22));
    }
}
