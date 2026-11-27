// SPDX-License-Identifier: GPL-3.0-or-later
//! OpenSSH security key (`sk-*`) formats as specified in [PROTOCOL.u2f].
//!
//! # Public keys
//!
//! ```text
//! sk-ecdsa-sha2-nistp256@openssh.com      sk-ssh-ed25519@openssh.com
//!   string  key type                        string  key type
//!   string  "nistp256"                      string  public key (32 bytes)
//!   string  Q (uncompressed point)          string  application
//!   string  application
//! ```
//!
//! # Private keys (`openssh-key-v1` private section)
//!
//! The public fields above, followed by `uint8 flags`, `string key_handle` and
//! `string reserved`. The key handle only works together with the authenticator that
//! created it; it is nevertheless treated as sensitive (zeroized, redacted in `Debug`).
//!
//! # Signatures
//!
//! ```text
//! string  key type
//! string  signature   ; ecdsa: mpint r || mpint s, ed25519: 64 bytes
//! uint8   flags       ; authenticator data flags (UP = 0x01, UV = 0x04)
//! uint32  counter     ; authenticator signature counter
//! ```
//!
//! The authenticator signs `SHA256(application) || flags || counter || SHA256(message)`
//! (its authenticator data followed by the client data hash). For ECDSA that byte string
//! is hashed once more with SHA-256 (ES256); Ed25519 signs it directly.
//!
//! [PROTOCOL.u2f]: https://cvsweb.openbsd.org/src/usr.bin/ssh/PROTOCOL.u2f

use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use sha2::{Digest as _, Sha256};
use ssh_key::PublicKey;
use zeroize::Zeroizing;

use crate::ecdsa::EcdsaP256Signature;
use crate::error::{Error, Result, fill_random};
use crate::pubkey::{fingerprint_sha256, openssh_line, p256_uncompressed_point};
use crate::wire::{Reader, Writer};

/// SSH name of FIDO2 Ed25519 keys.
pub const SK_ED25519_ALGORITHM: &str = "sk-ssh-ed25519@openssh.com";
/// SSH name of FIDO2 ECDSA P-256 keys.
pub const SK_ECDSA_P256_ALGORITHM: &str = "sk-ecdsa-sha2-nistp256@openssh.com";
/// The application (FIDO relying party id) OpenSSH uses by default.
pub const DEFAULT_APPLICATION: &str = "ssh:";

/// Key flags stored in OpenSSH security key files (`SSH_SK_*` in `sk-api.h`).
pub mod key_flags {
    /// `SSH_SK_USER_PRESENCE_REQD`: every signature needs a touch.
    pub const USER_PRESENCE_REQUIRED: u8 = 0x01;
    /// `SSH_SK_USER_VERIFICATION_REQD`: every signature needs PIN / biometric verification.
    pub const USER_VERIFICATION_REQUIRED: u8 = 0x04;
    /// `SSH_SK_FORCE_OPERATION`: overwrite an existing resident credential on enrollment.
    pub const FORCE_OPERATION: u8 = 0x10;
    /// `SSH_SK_RESIDENT_KEY`: the credential is discoverable (stored on the authenticator).
    pub const RESIDENT_KEY: u8 = 0x20;

    /// Flags for a non-resident key with the given requirements (as stored in
    /// `KeyStorage::Fido2 { require_presence, require_pin }`).
    pub const fn from_requirements(require_presence: bool, require_verification: bool) -> u8 {
        let mut flags = 0;
        if require_presence {
            flags |= USER_PRESENCE_REQUIRED;
        }
        if require_verification {
            flags |= USER_VERIFICATION_REQUIRED;
        }
        flags
    }
}

/// Flags of the CTAP2 authenticator data (W3C WebAuthn §6.1).
pub mod auth_data_flags {
    /// User present.
    pub const USER_PRESENT: u8 = 0x01;
    /// User verified.
    pub const USER_VERIFIED: u8 = 0x04;
    /// Attested credential data included (only in `makeCredential` responses).
    pub const ATTESTED_CREDENTIAL_DATA: u8 = 0x40;
    /// Extension data included.
    pub const EXTENSION_DATA: u8 = 0x80;
}

/// Maximum key handle length accepted (CTAP2 credential ids are at most 1023 bytes).
const MAX_KEY_HANDLE_LEN: usize = 1023;
/// Maximum application string length (matches CTAP2 rpId limits in practice).
const MAX_APPLICATION_LEN: usize = 255;

const PEM_BEGIN: &str = "-----BEGIN OPENSSH PRIVATE KEY-----";
const PEM_END: &str = "-----END OPENSSH PRIVATE KEY-----";
const AUTH_MAGIC: &[u8] = b"openssh-key-v1\0";
const PEM_LINE_WIDTH: usize = 70;
const PRIVATE_BLOCK_SIZE: usize = 8;

/// Algorithm of a FIDO2 security key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkAlgorithm {
    /// `sk-ssh-ed25519@openssh.com` (COSE EdDSA, -8).
    Ed25519,
    /// `sk-ecdsa-sha2-nistp256@openssh.com` (COSE ES256, -7).
    EcdsaP256,
}

impl SkAlgorithm {
    /// The SSH key / signature algorithm name.
    pub const fn ssh_name(self) -> &'static str {
        match self {
            Self::Ed25519 => SK_ED25519_ALGORITHM,
            Self::EcdsaP256 => SK_ECDSA_P256_ALGORITHM,
        }
    }

    /// The COSE algorithm identifier requested from the authenticator.
    pub const fn cose_algorithm(self) -> i32 {
        match self {
            Self::Ed25519 => -8,
            Self::EcdsaP256 => -7,
        }
    }

    fn from_ssh_name(name: &[u8]) -> Option<Self> {
        match name {
            n if n == SK_ED25519_ALGORITHM.as_bytes() => Some(Self::Ed25519),
            n if n == SK_ECDSA_P256_ALGORITHM.as_bytes() => Some(Self::EcdsaP256),
            _ => None,
        }
    }
}

impl From<SkAlgorithm> for core_model::KeyAlgorithm {
    fn from(value: SkAlgorithm) -> Self {
        match value {
            SkAlgorithm::Ed25519 => Self::SkEd25519,
            SkAlgorithm::EcdsaP256 => Self::SkEcdsaP256,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
enum SkPublic {
    Ed25519([u8; 32]),
    EcdsaP256([u8; 65]),
}

/// A FIDO2 security key credential in OpenSSH form: public key, application, flags and
/// the authenticator's key handle (credential id).
#[derive(Clone, PartialEq, Eq)]
pub struct SkKey {
    public: SkPublic,
    application: String,
    flags: u8,
    key_handle: Zeroizing<Vec<u8>>,
    reserved: Vec<u8>,
    /// Cached typed form of the public key (validated by `ssh-key` at construction).
    ssh_public: PublicKey,
}

impl fmt::Debug for SkKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkKey")
            .field("algorithm", &self.algorithm())
            .field("application", &self.application)
            .field("flags", &format_args!("{:#04x}", self.flags))
            .field(
                "key_handle",
                &format_args!("<{} bytes>", self.key_handle.len()),
            )
            .finish_non_exhaustive()
    }
}

impl SkKey {
    /// Builds a key from its parts, validating the public key and the field sizes.
    ///
    /// `public_key` is the 32-byte Ed25519 key, or a P-256 point (SEC1, either form).
    pub fn new(
        algorithm: SkAlgorithm,
        public_key: &[u8],
        application: impl Into<String>,
        flags: u8,
        key_handle: Vec<u8>,
    ) -> Result<Self> {
        let key_handle = Zeroizing::new(key_handle);
        let public = match algorithm {
            SkAlgorithm::Ed25519 => {
                let bytes = <[u8; 32]>::try_from(public_key)
                    .map_err(|_| Error::malformed("Ed25519 public key", "must be 32 bytes"))?;
                ed25519_dalek::VerifyingKey::from_bytes(&bytes)
                    .map_err(|_| Error::malformed("Ed25519 public key", "invalid point"))?;
                SkPublic::Ed25519(bytes)
            }
            SkAlgorithm::EcdsaP256 => SkPublic::EcdsaP256(p256_uncompressed_point(public_key)?),
        };
        let application = application.into();
        if application.is_empty() || application.len() > MAX_APPLICATION_LEN {
            return Err(Error::malformed(
                "security key",
                "invalid application length",
            ));
        }
        if key_handle.is_empty() || key_handle.len() > MAX_KEY_HANDLE_LEN {
            return Err(Error::malformed(
                "security key",
                "invalid key handle length",
            ));
        }
        let mut w = Writer::new();
        write_public_fields(&mut w, &public, &application);
        let ssh_public = PublicKey::from_bytes(&w.into_vec())?;
        Ok(Self {
            public,
            application,
            flags,
            key_handle,
            reserved: Vec::new(),
            ssh_public,
        })
    }

    /// Rebuilds a key from its public key (e.g. `KeyMeta::public_key` parsed with
    /// [`PublicKey::from_openssh`]), flags and the key handle stored in the vault.
    pub fn from_public_key(public_key: &PublicKey, flags: u8, key_handle: Vec<u8>) -> Result<Self> {
        const WHAT: &str = "security key public key";
        let blob = public_key.to_bytes()?;
        let mut r = Reader::new(&blob, WHAT);
        let algorithm = SkAlgorithm::from_ssh_name(r.string()?)
            .ok_or(Error::malformed(WHAT, "not a security key (sk-*) key"))?;
        let public = match algorithm {
            SkAlgorithm::Ed25519 => r.string()?,
            SkAlgorithm::EcdsaP256 => {
                r.expect_string(b"nistp256", "unexpected curve")?;
                r.string()?
            }
        };
        let application = r.utf8()?.to_owned();
        r.finish()?;
        Self::new(algorithm, public, application, flags, key_handle)
    }

    /// The `KeyStorage` entry for this key, once its key handle has been stored as the
    /// vault secret `secret`.
    pub fn key_storage(&self, secret: core_model::SecretRef) -> core_model::KeyStorage {
        core_model::KeyStorage::Fido2 {
            secret,
            application: self.application.clone(),
            require_pin: self.requires_user_verification(),
            require_presence: self.requires_user_presence(),
        }
    }

    /// The key algorithm.
    pub const fn algorithm(&self) -> SkAlgorithm {
        match self.public {
            SkPublic::Ed25519(_) => SkAlgorithm::Ed25519,
            SkPublic::EcdsaP256(_) => SkAlgorithm::EcdsaP256,
        }
    }

    /// The FIDO application / relying party id (normally `ssh:`).
    pub fn application(&self) -> &str {
        &self.application
    }

    /// OpenSSH key flags (see [`key_flags`]).
    pub const fn flags(&self) -> u8 {
        self.flags
    }

    /// The authenticator key handle (credential id). Store it encrypted (vault secret).
    pub fn key_handle(&self) -> &[u8] {
        &self.key_handle
    }

    /// Whether each signature requires touching the key.
    pub const fn requires_user_presence(&self) -> bool {
        self.flags & key_flags::USER_PRESENCE_REQUIRED != 0
    }

    /// Whether each signature requires PIN or on-device biometric verification.
    pub const fn requires_user_verification(&self) -> bool {
        self.flags & key_flags::USER_VERIFICATION_REQUIRED != 0
    }

    /// Whether the credential is resident (discoverable) on the authenticator.
    pub const fn is_resident(&self) -> bool {
        self.flags & key_flags::RESIDENT_KEY != 0
    }

    fn write_public_fields(&self, w: &mut Writer) {
        write_public_fields(w, &self.public, &self.application);
    }

    /// The SSH wire encoding of the public key.
    pub fn public_key_blob(&self) -> Vec<u8> {
        let mut w = Writer::new();
        self.write_public_fields(&mut w);
        w.into_vec()
    }

    /// The public key as an [`ssh_key::PublicKey`] (empty comment).
    pub fn public_key(&self) -> PublicKey {
        self.ssh_public.clone()
    }

    /// The OpenSSH `authorized_keys` line.
    pub fn public_key_openssh(&self, comment: &str) -> String {
        openssh_line(&self.public_key(), comment)
    }

    /// The `SHA256:` fingerprint.
    pub fn fingerprint(&self) -> String {
        fingerprint_sha256(&self.public_key())
    }

    /// Serializes the key as an unencrypted OpenSSH private key file (`openssh-key-v1`),
    /// as written by `ssh-keygen -t ed25519-sk -N ''`. OpenSSH ≥ 8.2 can use it directly
    /// with the same authenticator.
    pub fn to_openssh_private_key(&self, comment: &str) -> Result<Zeroizing<String>> {
        let mut check = [0u8; 4];
        fill_random(&mut check)?;
        let checkint = u32::from_be_bytes(check);

        let mut private = Writer::new();
        private.u32(checkint).u32(checkint);
        self.write_public_fields(&mut private);
        private
            .u8(self.flags)
            .string(&self.key_handle)
            .string(&self.reserved)
            .string(comment.as_bytes());
        let mut pad = 1u8;
        while !private.len().is_multiple_of(PRIVATE_BLOCK_SIZE) {
            private.u8(pad);
            pad += 1;
        }
        let private = Zeroizing::new(private.into_vec());

        let mut file = Writer::new();
        file.raw(AUTH_MAGIC)
            .string(b"none")
            .string(b"none")
            .string(b"")
            .u32(1)
            .string(&self.public_key_blob())
            .string(&private);
        let file = Zeroizing::new(file.into_vec());

        let encoded = Zeroizing::new(BASE64.encode(file.as_slice()));
        let mut out = String::with_capacity(encoded.len() + encoded.len() / PEM_LINE_WIDTH + 80);
        out.push_str(PEM_BEGIN);
        out.push('\n');
        for chunk in encoded.as_bytes().chunks(PEM_LINE_WIDTH) {
            // Base64 output is ASCII, so every chunk is valid UTF-8.
            out.push_str(std::str::from_utf8(chunk).unwrap_or_default());
            out.push('\n');
        }
        out.push_str(PEM_END);
        out.push('\n');
        Ok(Zeroizing::new(out))
    }

    /// Parses an unencrypted OpenSSH private key file holding a security key (e.g. one
    /// created with `ssh-keygen -t ecdsa-sk`). Returns the key and its comment.
    ///
    /// Passphrase-protected files are rejected with [`Error::Unsupported`].
    pub fn from_openssh_private_key(text: &str) -> Result<(Self, String)> {
        const WHAT: &str = "OpenSSH private key";
        let text = text.trim();
        let body = text
            .strip_prefix(PEM_BEGIN)
            .and_then(|t| t.strip_suffix(PEM_END))
            .ok_or(Error::malformed(WHAT, "missing PEM armor"))?;
        let b64: Zeroizing<String> =
            Zeroizing::new(body.chars().filter(|c| !c.is_ascii_whitespace()).collect());
        let raw = Zeroizing::new(
            BASE64
                .decode(b64.as_bytes())
                .map_err(|_| Error::malformed(WHAT, "invalid base64"))?,
        );

        let magic_ok = raw.get(..AUTH_MAGIC.len()) == Some(AUTH_MAGIC);
        if !magic_ok {
            return Err(Error::malformed(WHAT, "not an openssh-key-v1 file"));
        }
        let mut r = Reader::new(&raw[AUTH_MAGIC.len()..], WHAT);
        let cipher = r.string()?;
        let kdf = r.string()?;
        let _kdf_options = r.string()?;
        if cipher != b"none" || kdf != b"none" {
            return Err(Error::Unsupported("passphrase-protected private key files"));
        }
        if r.u32()? != 1 {
            return Err(Error::malformed(WHAT, "expected exactly one key"));
        }
        let public_blob = r.string()?;
        let private = r.string()?;
        r.finish()?;

        if !private.len().is_multiple_of(PRIVATE_BLOCK_SIZE) {
            return Err(Error::malformed(WHAT, "private section not block aligned"));
        }
        let mut p = Reader::new(private, WHAT);
        if p.u32()? != p.u32()? {
            return Err(Error::malformed(WHAT, "check bytes mismatch"));
        }
        let key = Self::read_private_fields(&mut p)?;
        let comment = p.utf8()?.to_owned();
        let padding = p.raw(p.remaining())?;
        let padding_ok = padding.len() < PRIVATE_BLOCK_SIZE
            && padding
                .iter()
                .zip(1u8..)
                .all(|(&byte, expected)| byte == expected);
        if !padding_ok {
            return Err(Error::malformed(WHAT, "invalid padding"));
        }
        if key.public_key_blob() != public_blob {
            return Err(Error::malformed(WHAT, "public and private key differ"));
        }
        Ok((key, comment))
    }

    fn read_private_fields(r: &mut Reader<'_>) -> Result<Self> {
        let algorithm = SkAlgorithm::from_ssh_name(r.string()?).ok_or(Error::malformed(
            "OpenSSH private key",
            "not a security key (sk-*) key",
        ))?;
        let public = match algorithm {
            SkAlgorithm::Ed25519 => r.string()?,
            SkAlgorithm::EcdsaP256 => {
                r.expect_string(b"nistp256", "unexpected curve")?;
                r.string()?
            }
        };
        let application = r.utf8()?.to_owned();
        let flags = r.u8()?;
        let key_handle = r.string()?.to_vec();
        let reserved = r.string()?.to_vec();
        let mut key = Self::new(algorithm, public, application, flags, key_handle)?;
        key.reserved = reserved;
        Ok(key)
    }

    /// Verifies an SSH `sk-*` signature over `message` following OpenSSH's
    /// `ssh_ecdsa_sk_verify` / `ssh_ed25519_sk_verify`, returning the decoded signature.
    /// Ed25519 uses strict (RFC 8032 canonical, no small-order points) verification.
    ///
    /// This checks cryptographic validity only. Whether the signature's flags satisfy the
    /// key's policy (touch / verification required) is reported by
    /// [`SkSignature::satisfies_key_policy`].
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> Result<SkSignature> {
        let sig = SkSignature::decode(signature)?;
        if sig.algorithm() != self.algorithm() {
            return Err(Error::InvalidSignature);
        }
        let signed = signed_data(&self.application, sig.flags, sig.counter, message);
        let ok = match (&self.public, &sig.body) {
            (SkPublic::Ed25519(pk), SkSignatureBody::Ed25519(s)) => {
                let key = ed25519_dalek::VerifyingKey::from_bytes(pk)
                    .map_err(|_| Error::InvalidSignature)?;
                let s = ed25519_dalek::Signature::from_bytes(s);
                key.verify_strict(&signed, &s).is_ok()
            }
            (SkPublic::EcdsaP256(q), SkSignatureBody::EcdsaP256(s)) => {
                use p256::ecdsa::signature::Verifier as _;
                let key = p256::ecdsa::VerifyingKey::from_sec1_bytes(q)
                    .map_err(|_| Error::InvalidSignature)?;
                // ES256: ECDSA over SHA-256(signed data).
                key.verify(&signed, &s.to_p256()?).is_ok()
            }
            _ => false,
        };
        if ok {
            Ok(sig)
        } else {
            Err(Error::InvalidSignature)
        }
    }
}

fn write_public_fields(w: &mut Writer, public: &SkPublic, application: &str) {
    match public {
        SkPublic::Ed25519(pk) => {
            w.string(SK_ED25519_ALGORITHM.as_bytes()).string(pk);
        }
        SkPublic::EcdsaP256(q) => {
            w.string(SK_ECDSA_P256_ALGORITHM.as_bytes())
                .string(b"nistp256")
                .string(q);
        }
    }
    w.string(application.as_bytes());
}

/// The byte string a FIDO authenticator signs for OpenSSH: its authenticator data
/// (`SHA256(application) || flags || counter`) followed by the client data hash
/// `SHA256(message)`.
pub fn signed_data(application: &str, flags: u8, counter: u32, message: &[u8]) -> [u8; 69] {
    let mut out = [0u8; 69];
    out[..32].copy_from_slice(&Sha256::digest(application.as_bytes()));
    out[32] = flags;
    out[33..37].copy_from_slice(&counter.to_be_bytes());
    out[37..].copy_from_slice(&Sha256::digest(message));
    out
}

#[derive(Clone, PartialEq, Eq)]
enum SkSignatureBody {
    Ed25519([u8; 64]),
    EcdsaP256(EcdsaP256Signature),
}

/// A decoded OpenSSH security key signature.
#[derive(Clone, PartialEq, Eq)]
pub struct SkSignature {
    body: SkSignatureBody,
    flags: u8,
    counter: u32,
}

impl fmt::Debug for SkSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkSignature")
            .field("algorithm", &self.algorithm())
            .field("flags", &format_args!("{:#04x}", self.flags))
            .field("counter", &self.counter)
            .finish_non_exhaustive()
    }
}

impl SkSignature {
    /// Builds a signature from a CTAP2 `authenticatorGetAssertion` response.
    ///
    /// `auth_data` is the raw authenticator data (must be exactly 37 bytes: no attested
    /// credential data and no extensions, and its rpIdHash must match the key's
    /// application). `signature` is DER for ES256 or the raw 64 bytes for EdDSA.
    pub fn from_assertion(key: &SkKey, auth_data: &[u8], signature: &[u8]) -> Result<Self> {
        const WHAT: &str = "authenticator data";
        if auth_data.len() < 37 {
            return Err(Error::malformed(WHAT, "too short"));
        }
        let rp_id_hash = Sha256::digest(key.application.as_bytes());
        if auth_data[..32] != rp_id_hash[..] {
            return Err(Error::malformed(
                WHAT,
                "rpIdHash does not match the application",
            ));
        }
        let flags = auth_data[32];
        if flags & (auth_data_flags::ATTESTED_CREDENTIAL_DATA | auth_data_flags::EXTENSION_DATA)
            != 0
            || auth_data.len() != 37
        {
            // OpenSSH's verifier reconstructs the signed data without extensions, so a
            // signature over extension data could never verify.
            return Err(Error::malformed(
                WHAT,
                "unexpected attested data or extensions",
            ));
        }
        let counter =
            u32::from_be_bytes([auth_data[33], auth_data[34], auth_data[35], auth_data[36]]);
        let body = match key.algorithm() {
            SkAlgorithm::Ed25519 => SkSignatureBody::Ed25519(
                <[u8; 64]>::try_from(signature)
                    .map_err(|_| Error::malformed("Ed25519 signature", "must be 64 bytes"))?,
            ),
            SkAlgorithm::EcdsaP256 => {
                SkSignatureBody::EcdsaP256(EcdsaP256Signature::from_der(signature)?)
            }
        };
        Ok(Self {
            body,
            flags,
            counter,
        })
    }

    /// Decodes an SSH `sk-*` signature blob (strict: no trailing data).
    pub fn decode(blob: &[u8]) -> Result<Self> {
        const WHAT: &str = "security key signature";
        let mut r = Reader::new(blob, WHAT);
        let algorithm = SkAlgorithm::from_ssh_name(r.string()?)
            .ok_or(Error::malformed(WHAT, "unsupported signature algorithm"))?;
        let inner = r.string()?;
        let flags = r.u8()?;
        let counter = r.u32()?;
        r.finish()?;
        let body = match algorithm {
            SkAlgorithm::Ed25519 => SkSignatureBody::Ed25519(
                <[u8; 64]>::try_from(inner)
                    .map_err(|_| Error::malformed(WHAT, "Ed25519 signature must be 64 bytes"))?,
            ),
            SkAlgorithm::EcdsaP256 => {
                SkSignatureBody::EcdsaP256(EcdsaP256Signature::from_ssh_blob(inner)?)
            }
        };
        Ok(Self {
            body,
            flags,
            counter,
        })
    }

    /// Encodes the complete SSH signature blob.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.string(self.algorithm().ssh_name().as_bytes());
        match &self.body {
            SkSignatureBody::Ed25519(s) => w.string(s),
            SkSignatureBody::EcdsaP256(s) => w.string(&s.to_ssh_blob()),
        };
        w.u8(self.flags).u32(self.counter);
        w.into_vec()
    }

    /// The signature algorithm.
    pub const fn algorithm(&self) -> SkAlgorithm {
        match self.body {
            SkSignatureBody::Ed25519(_) => SkAlgorithm::Ed25519,
            SkSignatureBody::EcdsaP256(_) => SkAlgorithm::EcdsaP256,
        }
    }

    /// Authenticator data flags (see [`auth_data_flags`]).
    pub const fn flags(&self) -> u8 {
        self.flags
    }

    /// The authenticator's signature counter.
    pub const fn counter(&self) -> u32 {
        self.counter
    }

    /// Whether the flags satisfy the key's touch / verification requirements, as the
    /// OpenSSH server checks them (unless `no-touch-required` is configured there).
    pub const fn satisfies_key_policy(&self, key: &SkKey) -> bool {
        let presence_ok =
            !key.requires_user_presence() || self.flags & auth_data_flags::USER_PRESENT != 0;
        let verification_ok =
            !key.requires_user_verification() || self.flags & auth_data_flags::USER_VERIFIED != 0;
        presence_ok && verification_ok
    }

    /// Converts into an [`ssh_key::Signature`].
    pub fn to_ssh_key_signature(&self) -> Result<ssh_key::Signature> {
        Ok(ssh_key::Signature::try_from(self.encode().as_slice())?)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
