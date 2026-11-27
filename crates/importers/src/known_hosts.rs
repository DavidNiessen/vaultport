// SPDX-License-Identifier: GPL-3.0-or-later
//! OpenSSH `known_hosts` importer.
//!
//! Each valid line becomes a [`KnownHost`]: `[@marker] patterns keytype base64-key [comment]`.
//!
//! * Patterns are kept verbatim (comma-separated names, `[host]:port`, wildcards and `!`
//!   negations), matching the format of [`KnownHost::pattern`].
//! * **Hashed** entries (`|1|base64-salt|base64-hmac`, written by `HashKnownHosts yes`) cannot
//!   be reversed. They are imported as-is: [`KnownHost::matches`] does not understand them,
//!   but [`entry_matches`] / [`hashed_pattern_matches`] check them by recomputing
//!   `HMAC-SHA1(salt, hostname)` exactly like OpenSSH does.
//! * `@cert-authority` and `@revoked` markers are preserved.
//! * The key blob is validated (base64, embedded key type equals the declared one) and its
//!   OpenSSH `SHA256:` fingerprint is computed. Invalid lines are skipped with a warning.

use crate::bundle::{ImportBundle, WarningKind};
use crate::error::ImportError;
use crate::finalize::finalize;
use crate::text::check_size;
use crate::wire::{blob_key_type, decode_blob, encode_blob, sha256_fingerprint};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use core_model::{KnownHost, KnownHostId, KnownHostMarker};
use hmac::{Hmac, Mac};
use sha1::Sha1;

/// Parses a `known_hosts` file.
pub fn parse(input: &str) -> Result<ImportBundle, ImportError> {
    check_size(input.len())?;
    let mut b = ImportBundle::default();
    for (idx, raw) in input.lines().enumerate() {
        let line = idx + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        match parse_line(trimmed) {
            Ok(kh) => b.known_hosts.push(kh),
            Err(e) => b.warn(
                WarningKind::Invalid,
                Some(line),
                None,
                format!("line skipped: {e}"),
            ),
        }
    }
    Ok(finalize(b))
}

/// Splits off the first whitespace-delimited token.
fn token(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }
    Some(s.split_once(char::is_whitespace).unwrap_or((s, "")))
}

fn parse_line(line: &str) -> Result<KnownHost, &'static str> {
    let (mut first, mut rest) = token(line).ok_or("empty line")?;
    let marker = if first.starts_with('@') {
        let m = match first.to_ascii_lowercase().as_str() {
            "@cert-authority" => KnownHostMarker::CertAuthority,
            "@revoked" => KnownHostMarker::Revoked,
            _ => return Err("unknown marker"),
        };
        (first, rest) = token(rest).ok_or("missing host patterns")?;
        Some(m)
    } else {
        None
    };
    let patterns = first;
    let (key_type, rest) = token(rest).ok_or("missing key type")?;
    if key_type.bytes().all(|c| c.is_ascii_digit()) {
        return Err("SSH protocol 1 keys are not supported");
    }
    let (key_b64, comment) = token(rest).ok_or("missing key")?;
    validate_patterns(patterns)?;
    let blob = decode_blob(key_b64).ok_or("key is not valid base64")?;
    if blob_key_type(&blob) != Some(key_type) {
        return Err("key type does not match the key data");
    }
    let comment = comment.trim();
    Ok(KnownHost {
        id: KnownHostId::new(),
        pattern: patterns.to_owned(),
        key_type: key_type.to_owned(),
        public_key: encode_blob(&blob),
        fingerprint: sha256_fingerprint(&blob),
        marker,
        comment: (!comment.is_empty()).then(|| comment.to_owned()),
        added_at: 0,
    })
}

fn validate_patterns(patterns: &str) -> Result<(), &'static str> {
    for p in patterns.split(',') {
        if p.is_empty() {
            return Err("empty host pattern");
        }
        if p.starts_with('|') {
            decode_hashed(p).ok_or("malformed hashed host name")?;
        } else if p.chars().any(char::is_control) {
            return Err("control characters in host pattern");
        }
    }
    Ok(())
}

/// Decodes `|1|salt|hash` into `(salt, hash)`.
fn decode_hashed(p: &str) -> Option<(Vec<u8>, Vec<u8>)> {
    let rest = p.strip_prefix("|1|")?;
    let (salt, hash) = rest.split_once('|')?;
    let salt = STANDARD.decode(salt).ok()?;
    let hash = STANDARD.decode(hash).ok()?;
    (salt.len() == 20 && hash.len() == 20).then_some((salt, hash))
}

/// Whether any hashed (`|1|...`) entry of a comma-separated `pattern` matches `host:port`.
///
/// OpenSSH hashes `host` for port 22 and `[host]:port` otherwise, lowercased.
#[must_use]
pub fn hashed_pattern_matches(pattern: &str, host: &str, port: u16) -> bool {
    let target = if port == 22 {
        host.to_ascii_lowercase()
    } else {
        format!("[{}]:{port}", host.to_ascii_lowercase())
    };
    pattern
        .split(',')
        .filter_map(decode_hashed)
        .any(|(salt, hash)| {
            Hmac::<Sha1>::new_from_slice(&salt).is_ok_and(|mut mac| {
                mac.update(target.as_bytes());
                mac.verify_slice(&hash).is_ok()
            })
        })
}

/// Whether a known-hosts entry applies to `host:port`, for both plain and hashed patterns.
#[must_use]
pub fn entry_matches(entry: &KnownHost, host: &str, port: u16) -> bool {
    entry.matches(host, port) || hashed_pattern_matches(&entry.pattern, host, port)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    const GITHUB_ED25519: &str =
        "AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";

    /// Builds a `|1|salt|hash` entry like `ssh-keygen -H` does.
    fn hash_host(salt: &[u8; 20], name: &str) -> String {
        let mut mac = Hmac::<Sha1>::new_from_slice(salt).unwrap();
        mac.update(name.as_bytes());
        let out = mac.finalize().into_bytes();
        format!("|1|{}|{}", STANDARD.encode(salt), STANDARD.encode(out))
    }

    #[test]
    fn realistic_file() {
        let hashed = hash_host(&[7u8; 20], "[git.example.com]:2222");
        let input = format!(
            "# comment\n\
             github.com,140.82.121.4 ssh-ed25519 {GITHUB_ED25519}\n\
             [jump.example.org]:2222 ssh-ed25519 {GITHUB_ED25519} my comment here\n\
             {hashed} ssh-ed25519 {GITHUB_ED25519}\n\
             @cert-authority *.example.com ssh-ed25519 {GITHUB_ED25519} CA\n\
             @revoked old.example.com ssh-ed25519 {GITHUB_ED25519}\n\
             \n\
             broken.example.com ssh-rsa {GITHUB_ED25519}\n\
             bad.example.com ssh-ed25519 !!!notbase64\n\
             @bogus x ssh-ed25519 {GITHUB_ED25519}\n\
             onlyhost\n\
             legacy.example.com 1024 35 1234567\n\
             |1|AAAA|BBBB ssh-ed25519 {GITHUB_ED25519}\n"
        );
        let b = parse(&input).unwrap();
        assert_eq!(b.known_hosts.len(), 5);
        let first = &b.known_hosts[0];
        assert_eq!(first.pattern, "github.com,140.82.121.4");
        assert_eq!(first.key_type, "ssh-ed25519");
        assert_eq!(
            first.fingerprint,
            "SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU"
        );
        assert_eq!(first.public_key, GITHUB_ED25519);
        assert!(first.matches("github.com", 22));
        assert_eq!(b.known_hosts[1].comment.as_deref(), Some("my comment here"));
        assert!(b.known_hosts[1].matches("jump.example.org", 2222));

        let h = &b.known_hosts[2];
        assert_eq!(h.pattern, hashed);
        assert!(
            !h.matches("git.example.com", 2222),
            "glob cannot match hashes"
        );
        assert!(entry_matches(h, "GIT.example.com", 2222));
        assert!(!entry_matches(h, "git.example.com", 22));

        assert_eq!(
            b.known_hosts[3].marker,
            Some(KnownHostMarker::CertAuthority)
        );
        assert_eq!(b.known_hosts[4].marker, Some(KnownHostMarker::Revoked));

        let lines: Vec<Option<usize>> = b.warnings.iter().map(|w| w.line).collect();
        assert_eq!(
            lines,
            [Some(8), Some(9), Some(10), Some(11), Some(12), Some(13)]
        );
    }

    #[test]
    fn duplicates_are_merged_and_crlf_is_fine() {
        let input = format!(
            "a.example ssh-ed25519 {GITHUB_ED25519}\r\na.example ssh-ed25519 {GITHUB_ED25519}\r\n"
        );
        let b = parse(&input).unwrap();
        assert_eq!(b.known_hosts.len(), 1);
        assert!(b.warnings.is_empty());
    }

    #[test]
    fn hashed_port_22_uses_bare_name() {
        let p = hash_host(&[1u8; 20], "host.example");
        assert!(hashed_pattern_matches(&p, "host.example", 22));
        assert!(!hashed_pattern_matches(&p, "host.example", 2222));
        assert!(!hashed_pattern_matches(
            "plain.example",
            "plain.example",
            22
        ));
    }
}
