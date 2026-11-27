// SPDX-License-Identifier: GPL-3.0-or-later
//! Algorithm preference lists and the negotiated result.
//!
//! # Defaults (most preferred first)
//!
//! | kind      | algorithms |
//! |-----------|------------|
//! | kex       | `mlkem768x25519-sha256` (post-quantum hybrid, only with `post_quantum`), `curve25519-sha256`, `curve25519-sha256@libssh.org`, `ecdh-sha2-nistp256/384/521`, `diffie-hellman-group-exchange-sha256`, `diffie-hellman-group16-sha512`, `diffie-hellman-group18-sha512`, `diffie-hellman-group14-sha256` |
//! | host key  | `ssh-ed25519`, `ecdsa-sha2-nistp256/384/521`, `rsa-sha2-512`, `rsa-sha2-256` (+ `*-cert-v01@openssh.com` variants when a `@cert-authority` is known for the host) |
//! | cipher    | `chacha20-poly1305@openssh.com`, `aes256-gcm@openssh.com`, `aes128-gcm@openssh.com`, `aes256-ctr`, `aes192-ctr`, `aes128-ctr` |
//! | MAC       | `hmac-sha2-512-etm@openssh.com`, `hmac-sha2-256-etm@openssh.com`, `hmac-sha2-512`, `hmac-sha2-256` (unused with AEAD ciphers) |
//! | compress  | `none` (or `zlib@openssh.com`, `zlib`, `none` with `compression`) |
//!
//! `legacy_algorithms` appends `diffie-hellman-group14-sha1`, `diffie-hellman-group-exchange-sha1`,
//! `ssh-rsa` (SHA-1 signatures), `aes256/192/128-cbc`, `hmac-sha1-etm@openssh.com` and `hmac-sha1`
//! — always *after* the secure choices, so they are only used when the server offers nothing else.
//!
//! `sntrup761x25519-sha512` is not implemented by russh and therefore not offered;
//! `mlkem768x25519-sha256` (the OpenSSH ≥ 9.9 default) is.
//! The strict-kex (`kex-strict-c-v00@openssh.com`, Terrapin mitigation) and `ext-info-c`
//! markers are always sent.

use russh::keys::ssh_key::{Algorithm, EcdsaCurve, HashAlg};
use russh::{Preferred, cipher, compression, kex, mac};
use std::borrow::Cow;

/// Feature switches that influence the offered algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlgorithmOptions {
    pub legacy: bool,
    pub post_quantum: bool,
    pub compression: bool,
}

impl Default for AlgorithmOptions {
    fn default() -> Self {
        Self {
            legacy: false,
            post_quantum: true,
            compression: false,
        }
    }
}

/// The secure host key algorithms, most preferred first.
pub(crate) fn default_host_key_algorithms() -> Vec<Algorithm> {
    vec![
        Algorithm::Ed25519,
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP256,
        },
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP384,
        },
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP521,
        },
        Algorithm::Rsa {
            hash: Some(HashAlg::Sha512),
        },
        Algorithm::Rsa {
            hash: Some(HashAlg::Sha256),
        },
    ]
}

/// Builds the russh preference lists.
///
/// * `known_key_algorithms` — algorithms of host keys already trusted for this host; they are
///   moved to the front (like OpenSSH does) so a server with several host keys presents the one
///   we know instead of triggering a spurious "host key changed" warning.
/// * `want_certificates` — also offer `*-cert-v01@openssh.com` host key algorithms (only useful
///   when a `@cert-authority` entry exists for the host).
#[must_use]
pub fn preferred(
    opts: AlgorithmOptions,
    known_key_algorithms: &[Algorithm],
    want_certificates: bool,
) -> Preferred {
    let mut kex_list: Vec<kex::Name> = Vec::new();
    if opts.post_quantum {
        kex_list.push(kex::MLKEM768X25519_SHA256);
    }
    kex_list.extend([
        kex::CURVE25519,
        kex::CURVE25519_PRE_RFC_8731,
        kex::ECDH_SHA2_NISTP256,
        kex::ECDH_SHA2_NISTP384,
        kex::ECDH_SHA2_NISTP521,
        kex::DH_GEX_SHA256,
        kex::DH_G16_SHA512,
        kex::DH_G18_SHA512,
        kex::DH_G14_SHA256,
    ]);
    if opts.legacy {
        kex_list.extend([kex::DH_G14_SHA1, kex::DH_GEX_SHA1]);
    }
    kex_list.extend([
        kex::EXTENSION_SUPPORT_AS_CLIENT,
        kex::EXTENSION_OPENSSH_STRICT_KEX_AS_CLIENT,
    ]);

    let mut keys = default_host_key_algorithms();
    if opts.legacy {
        keys.push(Algorithm::Rsa { hash: None });
    }
    // Known algorithms first, preserving the secure order within each partition.
    let (mut known, other): (Vec<_>, Vec<_>) = keys.into_iter().partition(|a| {
        known_key_algorithms.iter().any(|k| match (k, a) {
            // A stored RSA key can be verified with any of the RSA signature variants.
            (Algorithm::Rsa { .. }, Algorithm::Rsa { .. }) => true,
            (k, a) => k == a,
        })
    });
    known.extend(other);
    let keys = known;

    let certs = if want_certificates {
        keys.clone()
    } else {
        Vec::new()
    };

    let mut ciphers = vec![
        cipher::CHACHA20_POLY1305,
        cipher::AES_256_GCM,
        cipher::AES_128_GCM,
        cipher::AES_256_CTR,
        cipher::AES_192_CTR,
        cipher::AES_128_CTR,
    ];
    if opts.legacy {
        ciphers.extend([
            cipher::AES_256_CBC,
            cipher::AES_192_CBC,
            cipher::AES_128_CBC,
        ]);
    }

    let mut macs = vec![
        mac::HMAC_SHA512_ETM,
        mac::HMAC_SHA256_ETM,
        mac::HMAC_SHA512,
        mac::HMAC_SHA256,
    ];
    if opts.legacy {
        macs.extend([mac::HMAC_SHA1_ETM, mac::HMAC_SHA1]);
    }

    let compression_list = if opts.compression {
        vec![
            compression::ZLIB_LEGACY,
            compression::ZLIB,
            compression::NONE,
        ]
    } else {
        vec![compression::NONE]
    };

    Preferred {
        kex: Cow::Owned(kex_list),
        key: Cow::Owned(keys),
        host_key_certificates: Cow::Owned(certs),
        cipher: Cow::Owned(ciphers),
        mac: Cow::Owned(macs),
        compression: Cow::Owned(compression_list),
    }
}

/// Algorithms negotiated for one hop, for display ("connection info" panel).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NegotiatedAlgorithms {
    pub kex: String,
    pub host_key: String,
    pub cipher: String,
    /// MAC client→server (`none`/implicit for AEAD ciphers).
    pub mac_client_to_server: String,
    /// MAC server→client.
    pub mac_server_to_client: String,
    pub compression_client_to_server: String,
    pub compression_server_to_client: String,
    /// Whether strict key exchange (Terrapin mitigation) is active.
    pub strict_kex: bool,
    /// Whether the key exchange is post-quantum hybrid.
    pub post_quantum: bool,
}

impl NegotiatedAlgorithms {
    /// Summarizes russh's negotiated names.
    #[must_use]
    pub fn from_names(names: &russh::Names) -> Self {
        let kex: &str = names.kex.as_ref();
        let aead = is_aead(names.cipher.as_ref());
        let mac_name = |m: &mac::Name| {
            if aead {
                "<implicit>".to_owned()
            } else {
                m.as_ref().to_owned()
            }
        };
        Self {
            kex: kex.to_owned(),
            host_key: names.key.to_string(),
            cipher: names.cipher.as_ref().to_owned(),
            mac_client_to_server: mac_name(&names.client_mac),
            mac_server_to_client: mac_name(&names.server_mac),
            compression_client_to_server: compression_name(&names.client_compression),
            compression_server_to_client: compression_name(&names.server_compression),
            strict_kex: names.strict_kex(),
            post_quantum: kex.starts_with("mlkem") || kex.starts_with("sntrup"),
        }
    }
}

fn is_aead(cipher: &str) -> bool {
    cipher.contains("gcm") || cipher.contains("poly1305")
}

fn compression_name(c: &russh::compression::Compression) -> String {
    let s = format!("{c:?}");
    match s.as_str() {
        "None" => "none".to_owned(),
        "Zlib" => "zlib".to_owned(),
        "ZlibOpenSSH" => "zlib@openssh.com".to_owned(),
        _ => s.to_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names<T: AsRef<str>>(v: &[T]) -> Vec<String> {
        v.iter().map(|n| n.as_ref().to_owned()).collect()
    }

    #[test]
    fn secure_defaults() {
        let p = preferred(AlgorithmOptions::default(), &[], false);
        let kex = names(&p.kex);
        assert_eq!(kex[0], "mlkem768x25519-sha256");
        assert!(kex.contains(&"curve25519-sha256".to_owned()));
        assert!(kex.contains(&"kex-strict-c-v00@openssh.com".to_owned()));
        assert!(!kex.iter().any(|k| k.ends_with("sha1")));
        assert!(!p.key.contains(&Algorithm::Rsa { hash: None }));
        assert!(!names(&p.cipher).iter().any(|c| c.ends_with("cbc")));
        assert!(!names(&p.mac).iter().any(|m| m.contains("sha1")));
        assert_eq!(names(&p.compression), vec!["none"]);
        assert!(p.host_key_certificates.is_empty());
    }

    #[test]
    fn no_post_quantum_and_legacy() {
        let p = preferred(
            AlgorithmOptions {
                legacy: true,
                post_quantum: false,
                compression: true,
            },
            &[],
            true,
        );
        let kex = names(&p.kex);
        assert!(!kex.iter().any(|k| k.starts_with("mlkem")));
        assert!(kex.contains(&"diffie-hellman-group14-sha1".to_owned()));
        // Legacy algorithms come after the secure ones.
        let g14 = kex.iter().position(|k| k == "diffie-hellman-group14-sha1");
        let c25519 = kex.iter().position(|k| k == "curve25519-sha256");
        assert!(c25519 < g14);
        assert_eq!(p.key.last(), Some(&Algorithm::Rsa { hash: None }));
        assert!(names(&p.cipher).contains(&"aes128-cbc".to_owned()));
        assert!(names(&p.mac).contains(&"hmac-sha1".to_owned()));
        assert_eq!(names(&p.compression)[0], "zlib@openssh.com");
        assert!(!p.host_key_certificates.is_empty());
    }

    #[test]
    fn known_key_types_are_preferred() {
        let p = preferred(
            AlgorithmOptions::default(),
            &[Algorithm::Rsa { hash: None }],
            false,
        );
        assert!(matches!(p.key[0], Algorithm::Rsa { .. }));
        assert!(matches!(p.key[1], Algorithm::Rsa { .. }));
        assert_eq!(p.key[2], Algorithm::Ed25519);
    }
}
