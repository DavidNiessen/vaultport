// SPDX-License-Identifier: GPL-3.0-or-later
//! Key generation, import/inspection, certificates and `authorized_keys` export.
//!
//! * [`generate`] creates Ed25519, ECDSA (P-256/384/521) and RSA (3072/4096) keys in the
//!   OpenSSH format, optionally encrypted exactly like `ssh-keygen` (`aes256-ctr` +
//!   `bcrypt-pbkdf`, 16 rounds).
//! * [`inspect_private_key`] / [`decode_private_key`] understand OpenSSH, PKCS#8 (plain and
//!   encrypted), PKCS#1 RSA and PuTTY PPK v2/v3 keys.
//! * [`parse_certificate`] summarizes an OpenSSH certificate for display.
//! * [`export_to_host`] appends a public key to a remote `authorized_keys` idempotently.

use crate::fingerprint::{key_type_and_bits, randomart, sha256_fingerprint};
use core_model::KeyAlgorithm;
use russh::keys::ssh_encoding::Decode;
use russh::keys::ssh_key::certificate::CertType;
use russh::keys::ssh_key::private::{KeypairData, RsaKeypair};
use russh::keys::ssh_key::public::KeyData;
use russh::keys::ssh_key::{Algorithm, Certificate, EcdsaCurve, LineEnding, PrivateKey, PublicKey};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

/// Errors of key handling.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    /// The key is encrypted and no passphrase was supplied.
    #[error("the private key is encrypted; a passphrase is required")]
    NeedsPassphrase,
    /// The passphrase did not decrypt the key.
    #[error("wrong passphrase")]
    WrongPassphrase,
    /// The input is not a key in any supported format.
    #[error("unrecognized key format: {0}")]
    UnsupportedFormat(String),
    /// The key uses an algorithm or option that is not supported.
    #[error("unsupported key: {0}")]
    Unsupported(String),
    /// The key data is malformed.
    #[error("invalid key: {0}")]
    Invalid(String),
}

/// Container format of a private key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyFormat {
    /// `-----BEGIN OPENSSH PRIVATE KEY-----`
    OpenSsh,
    /// `-----BEGIN PRIVATE KEY-----` / `-----BEGIN ENCRYPTED PRIVATE KEY-----`
    Pkcs8,
    /// `-----BEGIN RSA PRIVATE KEY-----`
    Pkcs1,
    /// `-----BEGIN EC PRIVATE KEY-----`
    Sec1,
    /// `PuTTY-User-Key-File-2/3`
    PuttyPpk { version: u8 },
}

/// Detects the container format of `text`.
#[must_use]
pub fn detect_format(text: &str) -> Option<KeyFormat> {
    let t = text.trim_start();
    if t.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----") {
        Some(KeyFormat::OpenSsh)
    } else if t.starts_with("-----BEGIN PRIVATE KEY-----")
        || t.starts_with("-----BEGIN ENCRYPTED PRIVATE KEY-----")
    {
        Some(KeyFormat::Pkcs8)
    } else if t.starts_with("-----BEGIN RSA PRIVATE KEY-----") {
        Some(KeyFormat::Pkcs1)
    } else if t.starts_with("-----BEGIN EC PRIVATE KEY-----") {
        Some(KeyFormat::Sec1)
    } else if let Some(rest) = t.strip_prefix("PuTTY-User-Key-File-") {
        rest.chars()
            .next()
            .and_then(|c| c.to_digit(10))
            .map(|v| KeyFormat::PuttyPpk { version: v as u8 })
    } else {
        None
    }
}

/// Whether the key needs a passphrase (best effort, without decrypting).
#[must_use]
pub fn is_encrypted(text: &str) -> bool {
    match detect_format(text) {
        Some(KeyFormat::OpenSsh) => PrivateKey::from_openssh(text).is_ok_and(|k| k.is_encrypted()),
        Some(KeyFormat::Pkcs8) => text.contains("BEGIN ENCRYPTED PRIVATE KEY"),
        Some(KeyFormat::Pkcs1 | KeyFormat::Sec1) => text.contains("Proc-Type: 4,ENCRYPTED"),
        Some(KeyFormat::PuttyPpk { .. }) => {
            ppk_header(text, "Encryption").is_some_and(|e| e != "none")
        }
        None => false,
    }
}

fn ppk_header<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.lines().find_map(|l| {
        l.strip_prefix(name)
            .and_then(|r| r.strip_prefix(':'))
            .map(str::trim)
    })
}

/// Decodes (and decrypts) a private key in any supported format.
pub fn decode_private_key(text: &str, passphrase: Option<&str>) -> Result<PrivateKey, KeyError> {
    let passphrase = passphrase.filter(|p| !p.is_empty());
    let format = detect_format(text)
        .ok_or_else(|| KeyError::UnsupportedFormat("no private key header found".into()))?;
    let encrypted = is_encrypted(text);
    if encrypted && passphrase.is_none() {
        return Err(KeyError::NeedsPassphrase);
    }
    match format {
        KeyFormat::OpenSsh => {
            let key =
                PrivateKey::from_openssh(text).map_err(|e| KeyError::Invalid(e.to_string()))?;
            if key.is_encrypted() {
                let pass = passphrase.ok_or(KeyError::NeedsPassphrase)?;
                key.decrypt(pass).map_err(|_| KeyError::WrongPassphrase)
            } else {
                Ok(key)
            }
        }
        KeyFormat::PuttyPpk { version } => {
            if !(2..=3).contains(&version) {
                return Err(KeyError::Unsupported(format!(
                    "PuTTY key format version {version}"
                )));
            }
            PrivateKey::from_ppk(text, passphrase.map(ToOwned::to_owned)).map_err(|e| {
                if encrypted {
                    KeyError::WrongPassphrase
                } else {
                    KeyError::Invalid(e.to_string())
                }
            })
        }
        KeyFormat::Pkcs8 | KeyFormat::Pkcs1 | KeyFormat::Sec1 => {
            russh::keys::decode_secret_key(text, passphrase).map_err(|e| match e {
                russh::keys::Error::KeyIsEncrypted => KeyError::NeedsPassphrase,
                _ if encrypted => KeyError::WrongPassphrase,
                russh::keys::Error::UnsupportedKeyType {
                    key_type_string, ..
                } => KeyError::Unsupported(key_type_string),
                russh::keys::Error::UnknownAlgorithm(oid) => {
                    KeyError::Unsupported(format!("algorithm {oid}"))
                }
                other => KeyError::Invalid(other.to_string()),
            })
        }
    }
}

/// Public information about a private key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInfo {
    pub format: KeyFormat,
    pub encrypted: bool,
    /// `None` when the key is encrypted in a format that hides the public part (PKCS#8, PKCS#1).
    pub algorithm: Option<KeyAlgorithm>,
    pub bits: Option<u32>,
    /// `authorized_keys` line (with comment).
    pub public_key: Option<String>,
    pub fingerprint: Option<String>,
    pub randomart: Option<String>,
    pub comment: Option<String>,
}

/// Inspects a private key. With a passphrase, the key is fully decrypted and validated;
/// without, as much as possible is derived from the unencrypted parts (OpenSSH and PPK keys
/// store the public key in clear).
pub fn inspect_private_key(text: &str, passphrase: Option<&str>) -> Result<KeyInfo, KeyError> {
    let format = detect_format(text)
        .ok_or_else(|| KeyError::UnsupportedFormat("no private key header found".into()))?;
    let encrypted = is_encrypted(text);
    let public = match decode_private_key(text, passphrase) {
        Ok(k) => Some(k.public_key().clone()),
        Err(KeyError::NeedsPassphrase) => match format {
            KeyFormat::OpenSsh => PrivateKey::from_openssh(text)
                .ok()
                .map(|k| k.public_key().clone()),
            KeyFormat::PuttyPpk { .. } => ppk_public_key(text),
            _ => None,
        },
        Err(e) => return Err(e),
    };
    Ok(match public {
        Some(pk) => {
            let info = public_key_info(&pk);
            KeyInfo {
                format,
                encrypted,
                algorithm: info.algorithm,
                bits: Some(info.bits),
                public_key: Some(info.line),
                fingerprint: Some(info.fingerprint),
                randomart: Some(info.randomart),
                comment: Some(pk.comment().to_string()).filter(|c| !c.is_empty()),
            }
        }
        None => KeyInfo {
            format,
            encrypted,
            algorithm: None,
            bits: None,
            public_key: None,
            fingerprint: None,
            randomart: None,
            comment: None,
        },
    })
}

fn ppk_public_key(text: &str) -> Option<PublicKey> {
    use base64::Engine as _;
    let mut lines = text.lines();
    let count: usize = lines
        .by_ref()
        .find_map(|l| l.strip_prefix("Public-Lines:"))?
        .trim()
        .parse()
        .ok()?;
    let b64: String = lines.take(count).map(str::trim).collect();
    let blob = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    let data = KeyData::decode(&mut blob.as_slice()).ok()?;
    let comment = ppk_header(text, "Comment").unwrap_or_default();
    Some(PublicKey::new(data, comment))
}

/// Maps an SSH algorithm to the model's [`KeyAlgorithm`].
#[must_use]
pub fn key_algorithm(alg: &Algorithm) -> Option<KeyAlgorithm> {
    Some(match alg {
        Algorithm::Ed25519 => KeyAlgorithm::Ed25519,
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP256,
        } => KeyAlgorithm::EcdsaP256,
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP384,
        } => KeyAlgorithm::EcdsaP384,
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP521,
        } => KeyAlgorithm::EcdsaP521,
        Algorithm::Rsa { .. } => KeyAlgorithm::Rsa,
        Algorithm::SkEd25519 => KeyAlgorithm::SkEd25519,
        Algorithm::SkEcdsaSha2NistP256 => KeyAlgorithm::SkEcdsaP256,
        _ => return None,
    })
}

/// Display information about a public key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKeyInfo {
    pub algorithm: Option<KeyAlgorithm>,
    /// SSH algorithm name, e.g. `ssh-ed25519`.
    pub key_type: String,
    pub bits: u32,
    /// `authorized_keys` line.
    pub line: String,
    pub fingerprint: String,
    pub randomart: String,
    pub comment: String,
}

/// Summarizes a public key.
#[must_use]
pub fn public_key_info(pk: &PublicKey) -> PublicKeyInfo {
    let (_, bits) = key_type_and_bits(pk.key_data());
    PublicKeyInfo {
        algorithm: key_algorithm(&pk.algorithm()),
        key_type: pk.algorithm().as_str().to_owned(),
        bits,
        line: pk.to_openssh().unwrap_or_default().trim().to_owned(),
        fingerprint: sha256_fingerprint(pk.key_data()),
        randomart: randomart(pk.key_data()),
        comment: pk.comment().to_string(),
    }
}

/// Parses an `authorized_keys`-style public key line (`type base64 [comment]`).
pub fn parse_public_key(line: &str) -> Result<PublicKeyInfo, KeyError> {
    let pk = PublicKey::from_openssh(line.trim()).map_err(|e| KeyError::Invalid(e.to_string()))?;
    Ok(public_key_info(&pk))
}

/// Parameters of [`generate`].
#[derive(Clone)]
pub struct GenerateOptions {
    pub algorithm: KeyAlgorithm,
    /// RSA modulus size: 3072 or 4096 (ignored for other algorithms).
    pub rsa_bits: u32,
    pub comment: String,
    /// Encrypt the private key (aes256-ctr + bcrypt-pbkdf, like `ssh-keygen`).
    pub passphrase: Option<Zeroizing<String>>,
}

impl fmt::Debug for GenerateOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GenerateOptions")
            .field("algorithm", &self.algorithm)
            .field("rsa_bits", &self.rsa_bits)
            .field("comment", &self.comment)
            .field(
                "passphrase",
                &self.passphrase.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl GenerateOptions {
    #[must_use]
    pub fn new(algorithm: KeyAlgorithm) -> Self {
        Self {
            algorithm,
            rsa_bits: 4096,
            comment: String::new(),
            passphrase: None,
        }
    }
}

/// A freshly generated key pair.
#[derive(Clone)]
pub struct GeneratedKey {
    /// OpenSSH private key PEM (encrypted if a passphrase was given).
    pub private_key_pem: Zeroizing<String>,
    /// `authorized_keys` line.
    pub public_key: String,
    pub fingerprint: String,
    pub randomart: String,
    pub algorithm: KeyAlgorithm,
    pub bits: u32,
}

impl fmt::Debug for GeneratedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GeneratedKey")
            .field("private_key_pem", &"<redacted>")
            .field("public_key", &self.public_key)
            .field("fingerprint", &self.fingerprint)
            .field("algorithm", &self.algorithm)
            .field("bits", &self.bits)
            .finish_non_exhaustive()
    }
}

/// Generates a new key pair.
pub fn generate(opts: &GenerateOptions) -> Result<GeneratedKey, KeyError> {
    let mut rng = russh::keys::key::safe_rng();
    let mut key = match opts.algorithm {
        KeyAlgorithm::Ed25519 => PrivateKey::random(&mut rng, Algorithm::Ed25519),
        KeyAlgorithm::EcdsaP256 | KeyAlgorithm::EcdsaP384 | KeyAlgorithm::EcdsaP521 => {
            let curve = match opts.algorithm {
                KeyAlgorithm::EcdsaP256 => EcdsaCurve::NistP256,
                KeyAlgorithm::EcdsaP384 => EcdsaCurve::NistP384,
                _ => EcdsaCurve::NistP521,
            };
            PrivateKey::random(&mut rng, Algorithm::Ecdsa { curve })
        }
        KeyAlgorithm::Rsa => {
            if !matches!(opts.rsa_bits, 3072 | 4096) {
                return Err(KeyError::Unsupported(format!(
                    "RSA key size {} (use 3072 or 4096)",
                    opts.rsa_bits
                )));
            }
            RsaKeypair::random(&mut rng, opts.rsa_bits as usize)
                .and_then(|kp| PrivateKey::new(KeypairData::from(kp), ""))
        }
        KeyAlgorithm::SkEd25519 | KeyAlgorithm::SkEcdsaP256 => {
            return Err(KeyError::Unsupported(
                "FIDO2 keys are created on the authenticator, not in software".into(),
            ));
        }
    }
    .map_err(|e| KeyError::Invalid(e.to_string()))?;
    key.set_comment(opts.comment.as_str());
    let info = public_key_info(key.public_key());
    let key = match opts.passphrase.as_ref().filter(|p| !p.is_empty()) {
        Some(p) => key
            .encrypt(&mut rng, p.as_bytes())
            .map_err(|e| KeyError::Invalid(e.to_string()))?,
        None => key,
    };
    let pem = key
        .to_openssh(LineEnding::LF)
        .map_err(|e| KeyError::Invalid(e.to_string()))?;
    Ok(GeneratedKey {
        private_key_pem: Zeroizing::new(pem.as_str().to_owned()),
        public_key: info.line,
        fingerprint: info.fingerprint,
        randomart: info.randomart,
        algorithm: opts.algorithm,
        bits: info.bits,
    })
}

/// Kind of certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertificateKind {
    User,
    Host,
}

/// Display summary of an OpenSSH certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateInfo {
    pub kind: CertificateKind,
    /// Certificate algorithm, e.g. `ssh-ed25519-cert-v01@openssh.com`.
    pub cert_type: String,
    pub key_id: String,
    pub serial: u64,
    /// Empty means "valid for any principal" (rare; ssh-keygen requires `-n`).
    pub principals: Vec<String>,
    /// Unix seconds.
    pub valid_after: u64,
    /// Unix seconds; `u64::MAX` means "forever".
    pub valid_before: u64,
    pub critical_options: Vec<(String, String)>,
    pub extensions: Vec<String>,
    /// Fingerprint of the certified key (must match the private key used with it).
    pub key_fingerprint: String,
    /// Fingerprint of the signing CA.
    pub ca_fingerprint: String,
    pub comment: String,
}

impl CertificateInfo {
    /// Whether `now` (Unix seconds) is inside the validity window.
    #[must_use]
    pub fn is_valid_at(&self, now: u64) -> bool {
        self.valid_after <= now && now < self.valid_before
    }

    /// [`Self::is_valid_at`] with the system clock.
    #[must_use]
    pub fn is_valid_now(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        self.is_valid_at(now)
    }
}

/// Parses an OpenSSH certificate (`*-cert.pub` contents).
pub fn parse_certificate(text: &str) -> Result<CertificateInfo, KeyError> {
    let cert =
        Certificate::from_openssh(text.trim()).map_err(|e| KeyError::Invalid(e.to_string()))?;
    Ok(certificate_info(&cert))
}

/// Summarizes a parsed certificate.
#[must_use]
pub fn certificate_info(cert: &Certificate) -> CertificateInfo {
    CertificateInfo {
        kind: if cert.cert_type() == CertType::Host {
            CertificateKind::Host
        } else {
            CertificateKind::User
        },
        cert_type: cert.algorithm().to_certificate_type(),
        key_id: cert.key_id().to_owned(),
        serial: cert.serial(),
        principals: cert.valid_principals().to_vec(),
        valid_after: cert.valid_after(),
        valid_before: cert.valid_before(),
        critical_options: cert
            .critical_options()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        extensions: cert.extensions().keys().cloned().collect(),
        key_fingerprint: sha256_fingerprint(cert.public_key()),
        ca_fingerprint: sha256_fingerprint(cert.signature_key()),
        comment: cert.comment().to_owned(),
    }
}

/// Parses a certificate and checks it certifies `key`.
pub fn certificate_for_key(text: &str, key: &PublicKey) -> Result<Certificate, KeyError> {
    let cert =
        Certificate::from_openssh(text.trim()).map_err(|e| KeyError::Invalid(e.to_string()))?;
    if cert.public_key() != key.key_data() {
        return Err(KeyError::Invalid(
            "the certificate does not belong to this private key".into(),
        ));
    }
    Ok(cert)
}

/// Quotes `s` for a POSIX shell (single quotes, `'` → `'\''`).
#[must_use]
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Result of [`export_to_host`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportOutcome {
    Added,
    AlreadyPresent,
}

const MARK_ADDED: &str = "__SSHC_KEY_ADDED__";
const MARK_PRESENT: &str = "__SSHC_KEY_PRESENT__";

/// Builds the remote command used by [`export_to_host`].
///
/// `path` defaults to `~/.ssh/authorized_keys`; a leading `~/` means the home directory and
/// relative paths are relative to it. For the default path the directory is set to `0700` and
/// the file to `0600` (sshd `StrictModes`); in any case files are created with `umask 077`.
/// The key is matched by `type base64` (ignoring comment and options), so re-running is a
/// no-op.
pub fn authorized_keys_command(public_line: &str, path: Option<&str>) -> Result<String, KeyError> {
    if public_line.chars().any(|c| c.is_control() && c != '\t') {
        return Err(KeyError::Invalid(
            "control characters in public key line".into(),
        ));
    }
    let pk = PublicKey::from_openssh(public_line.trim())
        .map_err(|e| KeyError::Invalid(e.to_string()))?;
    let line = pk
        .to_openssh()
        .map_err(|e| KeyError::Invalid(e.to_string()))?
        .trim()
        .to_owned();
    let mut bare = PublicKey::new(pk.key_data().clone(), "");
    bare.set_comment("");
    let needle = bare
        .to_openssh()
        .map_err(|e| KeyError::Invalid(e.to_string()))?
        .trim()
        .to_owned();

    let (file_expr, is_default) = match path.map(str::trim).filter(|p| !p.is_empty()) {
        None | Some("~/.ssh/authorized_keys" | ".ssh/authorized_keys") => {
            ("\"$HOME\"/.ssh/authorized_keys".to_owned(), true)
        }
        Some(p) => {
            if p.chars().any(char::is_control) {
                return Err(KeyError::Invalid("control characters in path".into()));
            }
            if let Some(rest) = p.strip_prefix("~/") {
                (format!("\"$HOME\"/{}", shell_quote(rest)), false)
            } else if p.starts_with('/') {
                (shell_quote(p), false)
            } else {
                (format!("\"$HOME\"/{}", shell_quote(p)), false)
            }
        }
    };
    let (chmod_dir, chmod_file) = if is_default {
        (
            "chmod 700 \"$d\" || exit 1; ",
            "chmod 600 \"$f\" || exit 1; ",
        )
    } else {
        ("", "")
    };
    let script = format!(
        "umask 077; f={file_expr}; d=$(dirname \"$f\") || exit 1; mkdir -p \"$d\" || exit 1; \
         {chmod_dir}\
         if [ -f \"$f\" ] && grep -qF {needle} \"$f\"; then echo {MARK_PRESENT}; exit 0; fi; \
         if [ -s \"$f\" ] && [ -n \"$(tail -c 1 \"$f\")\" ]; then printf '\\n' >> \"$f\" || exit 1; fi; \
         printf '%s\\n' {line} >> \"$f\" || exit 1; {chmod_file}echo {MARK_ADDED}",
        needle = shell_quote(&needle),
        line = shell_quote(&line),
    );
    Ok(format!("sh -c {}", shell_quote(&script)))
}

/// Appends `public_line` to the remote `authorized_keys` (see [`authorized_keys_command`]).
pub async fn export_to_host(
    conn: &crate::Connection,
    public_line: &str,
    path: Option<&str>,
) -> Result<ExportOutcome, crate::SshError> {
    let cmd = authorized_keys_command(public_line, path)?;
    let out = conn.exec(&cmd).await?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.contains(MARK_ADDED) {
        Ok(ExportOutcome::Added)
    } else if stdout.contains(MARK_PRESENT) {
        Ok(ExportOutcome::AlreadyPresent)
    } else {
        Err(crate::SshError::UnexpectedResponse(format!(
            "authorized_keys update failed (exit {:?}): {}",
            out.exit_status,
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_reimport_all_algorithms() {
        for alg in [
            KeyAlgorithm::Ed25519,
            KeyAlgorithm::EcdsaP256,
            KeyAlgorithm::EcdsaP384,
            KeyAlgorithm::EcdsaP521,
        ] {
            let mut o = GenerateOptions::new(alg);
            o.comment = "me@test".into();
            let g = generate(&o).expect("generate");
            assert!(
                g.private_key_pem
                    .starts_with("-----BEGIN OPENSSH PRIVATE KEY-----")
            );
            assert!(g.public_key.ends_with(" me@test"));
            assert!(g.fingerprint.starts_with("SHA256:"));
            let info = inspect_private_key(&g.private_key_pem, None).expect("inspect");
            assert_eq!(info.algorithm, Some(alg));
            assert!(!info.encrypted);
            assert_eq!(info.public_key.as_deref(), Some(g.public_key.as_str()));
        }
    }

    #[test]
    fn rsa_size_is_validated() {
        let mut o = GenerateOptions::new(KeyAlgorithm::Rsa);
        o.rsa_bits = 1024;
        assert!(matches!(generate(&o), Err(KeyError::Unsupported(_))));
        assert!(generate(&GenerateOptions::new(KeyAlgorithm::SkEd25519)).is_err());
    }

    #[test]
    fn encrypted_openssh_key() {
        let mut o = GenerateOptions::new(KeyAlgorithm::Ed25519);
        o.passphrase = Some(Zeroizing::new("correct horse".into()));
        let g = generate(&o).expect("generate");
        assert!(is_encrypted(&g.private_key_pem));
        assert_eq!(
            decode_private_key(&g.private_key_pem, None).expect_err("needs pass"),
            KeyError::NeedsPassphrase
        );
        assert_eq!(
            decode_private_key(&g.private_key_pem, Some("wrong")).expect_err("wrong"),
            KeyError::WrongPassphrase
        );
        let k = decode_private_key(&g.private_key_pem, Some("correct horse")).expect("decrypt");
        assert_eq!(sha256_fingerprint(k.public_key().key_data()), g.fingerprint);
        // Public part is readable without the passphrase.
        let info = inspect_private_key(&g.private_key_pem, None).expect("inspect");
        assert!(info.encrypted);
        assert_eq!(info.fingerprint.as_deref(), Some(g.fingerprint.as_str()));
    }

    // Generated with: openssl genpkey -algorithm ed25519
    const PKCS8_ED25519: &str = "-----BEGIN PRIVATE KEY-----
MC4CAQAwBQYDK2VwBCIEIHhq0Dz6e1gC2rFdJ9QwH5p7H2h0Zx8bZ3k5v2nJ6m9T
-----END PRIVATE KEY-----
";

    #[test]
    fn pkcs8_import() {
        let info = inspect_private_key(PKCS8_ED25519, None).expect("pkcs8");
        assert_eq!(info.format, KeyFormat::Pkcs8);
        assert_eq!(info.algorithm, Some(KeyAlgorithm::Ed25519));
    }

    #[test]
    fn garbage_is_rejected_clearly() {
        assert!(matches!(
            inspect_private_key("hello", None),
            Err(KeyError::UnsupportedFormat(_))
        ));
        assert_eq!(
            detect_format("PuTTY-User-Key-File-3: ssh-ed25519\n"),
            Some(KeyFormat::PuttyPpk { version: 3 })
        );
    }

    #[test]
    fn shell_quoting() {
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn authorized_keys_command_is_safe() {
        let g = generate(&GenerateOptions::new(KeyAlgorithm::Ed25519)).expect("gen");
        let line = format!("{} evil'; rm -rf / #", g.public_key);
        let cmd = authorized_keys_command(&line, None).expect("cmd");
        assert!(cmd.starts_with("sh -c '"));
        // The injected quote is escaped twice (inner and outer quoting).
        assert!(!cmd.contains("evil'; rm"));
        assert!(authorized_keys_command("ssh-ed25519 AAAA\nrm -rf /", None).is_err());
        assert!(authorized_keys_command("not a key", None).is_err());
        let custom = authorized_keys_command(&g.public_key, Some("~/keys file")).expect("cmd");
        assert!(custom.contains("keys file"));
    }

    #[test]
    fn certificate_summary() {
        use russh::keys::ssh_key::certificate::Builder;
        let mut rng = russh::keys::key::safe_rng();
        let ca = PrivateKey::random(&mut rng, Algorithm::Ed25519).expect("ca");
        let user = PrivateKey::random(&mut rng, Algorithm::Ed25519).expect("user");
        let mut b =
            Builder::new_with_random_nonce(&mut rng, user.public_key(), 10, 1000).expect("b");
        b.cert_type(CertType::User).expect("t");
        b.key_id("alice@corp").expect("id");
        b.valid_principal("alice").expect("p");
        b.extension("permit-pty", "").expect("e");
        let cert = b.sign(&ca).expect("sign");
        let text = cert.to_openssh().expect("openssh");
        let info = parse_certificate(&text).expect("parse");
        assert_eq!(info.kind, CertificateKind::User);
        assert_eq!(info.key_id, "alice@corp");
        assert_eq!(info.principals, vec!["alice".to_owned()]);
        assert_eq!(info.extensions, vec!["permit-pty".to_owned()]);
        assert!(info.is_valid_at(500));
        assert!(!info.is_valid_at(1000));
        assert_eq!(
            info.ca_fingerprint,
            sha256_fingerprint(ca.public_key().key_data())
        );
        assert!(certificate_for_key(&text, user.public_key()).is_ok());
        assert!(certificate_for_key(&text, ca.public_key()).is_err());
    }
}
