// SPDX-License-Identifier: GPL-3.0-or-later
//! FIDO2 security keys over USB HID (CTAP2), using the pure-Rust `ctap-hid-fido2` crate on
//! top of `hidapi` (Linux: hidraw + libudev, macOS: IOKit, Windows: HID API).
//!
//! The protocol mapping follows OpenSSH's `sk-usbhid.c`:
//!
//! * **Enrollment** is `authenticatorMakeCredential` with relying party id = the
//!   application (`ssh:` by default), a random 32-byte challenge and a random 32-byte user
//!   id (so resident credentials never overwrite each other), requesting ES256 or EdDSA.
//! * **Signing** is `authenticatorGetAssertion` with the stored key handle in the allow
//!   list and `clientDataHash = SHA256(data)`; the authenticator data flags and counter are
//!   copied into the `sk-*` signature ([`SkSignature`]).
//! * A PIN is only requested when needed: for keys enrolled with
//!   `verify-required`, for resident credentials, when the authenticator demands it
//!   (`CTAP2_ERR_PIN_REQUIRED`, e.g. `alwaysUv`), and for enrollment on authenticators
//!   that require it. Keys without a PIN but with built-in user verification (biometric
//!   keys) use the `uv` option instead.
//!
//! All device I/O is blocking and runs on tokio's blocking pool; operations are serialized
//! process-wide because authenticators handle one CTAP transaction at a time.
//!
//! # Limitations
//!
//! * `ctap-hid-fido2` copies the PIN into non-zeroized buffers internally; our copies are
//!   zeroized.
//! * There is no cancellation: an operation waiting for a touch ends when the
//!   authenticator times out (typically 30 s).
//! * Loading resident keys from the authenticator (`ssh-keygen -K`) is not implemented.

use std::sync::{Arc, Mutex, MutexGuard};

use ctap_hid_fido2::fidokey::get_info::InfoOption;
use ctap_hid_fido2::fidokey::{
    CredentialSupportedKeyType, GetAssertionArgsBuilder, MakeCredentialArgsBuilder,
};
use ctap_hid_fido2::public_key::PublicKeyType;
use ctap_hid_fido2::public_key_credential_user_entity::PublicKeyCredentialUserEntity;
use ctap_hid_fido2::{FidoKeyHid, FidoKeyHidFactory, HidParam, LibCfg};
use sha2::{Digest as _, Sha256};
use zeroize::Zeroizing;

use crate::error::{Error, Result, fill_random};
use crate::pin::PinProvider;
use crate::signer::HardwareSigner;
use crate::sk::{DEFAULT_APPLICATION, SkAlgorithm, SkKey, SkSignature, auth_data_flags, key_flags};
use crate::task::blocking;

/// Serializes all CTAP transactions in this process.
static DEVICE_LOCK: Mutex<()> = Mutex::new(());

fn device_lock() -> MutexGuard<'static, ()> {
    // A poisoned lock only means a previous operation panicked; the guarded state is `()`.
    DEVICE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A connected FIDO2 HID authenticator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Platform HID path; pass it back in [`DeviceSelector::Path`].
    pub path: String,
    /// USB vendor id.
    pub vendor_id: u16,
    /// USB product id.
    pub product_id: u16,
    /// Product string reported by the device.
    pub product: String,
}

/// Which authenticator to use.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DeviceSelector {
    /// The only connected authenticator. When signing with several connected, the one
    /// holding the credential is found with a silent (no-touch) probe.
    #[default]
    Auto,
    /// A specific device from [`list_devices`].
    Path(String),
}

/// Lists connected FIDO2 HID authenticators.
pub async fn list_devices() -> Result<Vec<DeviceInfo>> {
    blocking(|| {
        let _guard = device_lock();
        Ok(enumerate())
    })
    .await
}

fn enumerate() -> Vec<DeviceInfo> {
    ctap_hid_fido2::get_fidokey_devices()
        .into_iter()
        .filter_map(|d| match d.param {
            HidParam::Path(path) => Some(DeviceInfo {
                path,
                vendor_id: d.vid,
                product_id: d.pid,
                product: d.product_string,
            }),
            HidParam::VidPid { .. } => None,
        })
        .collect()
}

fn lib_cfg() -> LibCfg {
    let mut cfg = LibCfg::init();
    // Never print to stdout/stderr; prompts go through `PinProvider::touch_prompt`.
    cfg.enable_log = false;
    cfg.enable_keep_alive_msg = false;
    cfg.keep_alive_msg.clear();
    cfg
}

fn open(path: &str) -> Result<FidoKeyHid> {
    FidoKeyHidFactory::create_by_params(&[HidParam::Path(path.to_owned())], &lib_cfg())
        .map_err(|e| map_ctap_error(&e.to_string()))
}

/// Maps a `ctap-hid-fido2` error message (which embeds the CTAP status name) to [`Error`].
fn map_ctap_error(message: &str) -> Error {
    const CANCELLED: [&str; 4] = [
        "CTAP2_ERR_OPERATION_DENIED",
        "CTAP2_ERR_USER_ACTION_TIMEOUT",
        "CTAP2_ERR_ACTION_TIMEOUT",
        "CTAP2_ERR_KEEPALIVE_CANCEL",
    ];
    let has = |code: &str| message.contains(code);
    if has("CTAP2_ERR_PIN_INVALID") {
        Error::PinInvalid
    } else if has("CTAP2_ERR_PIN_BLOCKED") || has("CTAP2_ERR_PIN_AUTH_BLOCKED") {
        Error::PinBlocked
    } else if has("CTAP2_ERR_PIN_REQUIRED") {
        Error::PinRequired
    } else if has("CTAP2_ERR_PIN_NOT_SET") {
        Error::VerificationUnavailable
    } else if has("CTAP2_ERR_NO_CREDENTIALS") || has("CTAP2_ERR_INVALID_CREDENTIAL") {
        Error::KeyNotFound
    } else if has("CTAP2_ERR_CREDENTIAL_EXCLUDED") {
        Error::AlreadyExists
    } else if has("CTAP2_ERR_UNSUPPORTED_ALGORITHM") {
        Error::Unsupported("the security key does not support this key type")
    } else if CANCELLED.iter().any(|c| has(c)) {
        Error::Cancelled
    } else {
        // Bounded, and free of secrets: the crate's messages only describe status codes.
        Error::Authenticator(message.chars().take(200).collect())
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Capabilities {
    client_pin: bool,
    built_in_uv: bool,
    make_cred_uv_not_required: bool,
}

fn capabilities(device: &FidoKeyHid) -> Result<Capabilities> {
    let info = device
        .get_info()
        .map_err(|e| map_ctap_error(&e.to_string()))?;
    let option = |name: &str| {
        info.options
            .iter()
            .any(|(key, value)| key == name && *value)
    };
    Ok(Capabilities {
        client_pin: option(InfoOption::ClientPin.as_ref()),
        built_in_uv: option(InfoOption::Uv.as_ref()),
        make_cred_uv_not_required: option(InfoOption::MakeCredUvNotRqd.as_ref()),
    })
}

/// How the user is verified for one CTAP request.
enum Verification {
    None,
    Pin(Zeroizing<String>),
    BuiltIn,
}

async fn ask_pin(provider: &dyn PinProvider) -> Result<Verification> {
    provider
        .pin()
        .await
        .map(Verification::Pin)
        .ok_or(Error::PinRequired)
}

/// Picks the verification method when user verification is mandatory.
async fn required_verification(
    caps: Capabilities,
    provider: &dyn PinProvider,
) -> Result<Verification> {
    if caps.client_pin {
        ask_pin(provider).await
    } else if caps.built_in_uv {
        Ok(Verification::BuiltIn)
    } else {
        Err(Error::VerificationUnavailable)
    }
}

/// Runs `op` once, and once more with a PIN if the authenticator demanded one.
async fn with_pin_retry<T, F>(
    mut verification: Verification,
    caps: Capabilities,
    provider: &dyn PinProvider,
    touch: bool,
    op: F,
) -> Result<T>
where
    T: Send + 'static,
    F: Fn(Verification) -> Result<T> + Clone + Send + Sync + 'static,
{
    let mut retried = false;
    loop {
        if touch {
            provider.touch_prompt();
        }
        let attempt = op.clone();
        let result = blocking(move || {
            let _guard = device_lock();
            attempt(verification)
        })
        .await;
        if touch {
            provider.touch_done();
        }
        match result {
            Err(Error::PinRequired) if !retried && caps.client_pin => {
                retried = true;
                verification = ask_pin(provider).await?;
            }
            other => return other,
        }
    }
}

/// Options for [`enroll`]. The defaults match `ssh-keygen -t ed25519-sk`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollOptions {
    /// Key type. Ed25519 needs a reasonably recent authenticator (e.g. YubiKey ≥ 5.2.3);
    /// ECDSA P-256 works everywhere.
    pub algorithm: SkAlgorithm,
    /// FIDO application; must start with `ssh:` (OpenSSH rule).
    pub application: String,
    /// User name stored with resident credentials.
    pub user_name: String,
    /// Create a resident (discoverable) credential (`-O resident`). Requires a PIN.
    pub resident: bool,
    /// Require a touch for every signature (cleared by `-O no-touch-required`).
    pub require_user_presence: bool,
    /// Require PIN / biometric verification for every signature (`-O verify-required`).
    pub require_user_verification: bool,
    /// Which authenticator to enroll on.
    pub device: DeviceSelector,
}

impl Default for EnrollOptions {
    fn default() -> Self {
        Self {
            algorithm: SkAlgorithm::Ed25519,
            application: DEFAULT_APPLICATION.to_owned(),
            user_name: "openssh".to_owned(),
            resident: false,
            require_user_presence: true,
            require_user_verification: false,
            device: DeviceSelector::Auto,
        }
    }
}

impl EnrollOptions {
    fn key_flags(&self) -> u8 {
        let mut flags = 0;
        if self.require_user_presence {
            flags |= key_flags::USER_PRESENCE_REQUIRED;
        }
        if self.require_user_verification {
            flags |= key_flags::USER_VERIFICATION_REQUIRED;
        }
        if self.resident {
            flags |= key_flags::RESIDENT_KEY;
        }
        flags
    }
}

/// Attestation returned by the authenticator at enrollment (for optional verification
/// against vendor roots; OpenSSH's `-O write-attestation`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollmentAttestation {
    /// Attestation statement format (`packed`, `fido-u2f`, `none`, ...).
    pub format: String,
    /// Raw authenticator data from `makeCredential`.
    pub authenticator_data: Vec<u8>,
    /// Attestation signature.
    pub signature: Vec<u8>,
    /// Attestation certificate chain (DER), leaf first. May be empty.
    pub certificates: Vec<Vec<u8>>,
    /// The challenge whose SHA-256 was used as client data hash.
    pub challenge: [u8; 32],
    /// Authenticator model id.
    pub aaguid: Vec<u8>,
}

/// Result of [`enroll`].
#[derive(Debug, Clone)]
pub struct Enrollment {
    /// The new key. Persist [`SkKey::key_handle`] in the vault (encrypted) and the public
    /// key in `KeyMeta`; [`SkKey::to_openssh_private_key`] exports it for OpenSSH.
    pub key: SkKey,
    /// Attestation data.
    pub attestation: EnrollmentAttestation,
}

/// Creates a new credential on a security key (`ssh-keygen -t *-sk`).
///
/// The user must touch the key; a PIN is requested through `pin` when required.
pub async fn enroll(options: &EnrollOptions, pin: &dyn PinProvider) -> Result<Enrollment> {
    if !options.application.starts_with(DEFAULT_APPLICATION)
        || options.application.len() > 255
        || options.user_name.is_empty()
        || options.user_name.len() > 64
    {
        return Err(Error::malformed(
            "enrollment options",
            "application must start with \"ssh:\" and names must be 1-64 bytes",
        ));
    }

    let selector = options.device.clone();
    let (path, caps) = blocking(move || {
        let _guard = device_lock();
        let path = resolve_single(&selector)?;
        let caps = capabilities(&open(&path)?)?;
        Ok((path, caps))
    })
    .await?;

    let pin_needed = caps.client_pin
        && (options.resident
            || options.require_user_verification
            || !caps.make_cred_uv_not_required);
    let verification = if pin_needed {
        ask_pin(pin).await?
    } else if options.require_user_verification {
        required_verification(caps, pin).await?
    } else {
        Verification::None
    };

    let mut challenge = [0u8; 32];
    fill_random(&mut challenge)?;
    let mut user_id = [0u8; 32];
    fill_random(&mut user_id)?;

    let request = Arc::new(MakeCredentialRequest {
        path,
        application: options.application.clone(),
        user_name: options.user_name.clone(),
        user_id,
        challenge,
        algorithm: options.algorithm,
        resident: options.resident,
    });
    let attestation = with_pin_retry(verification, caps, pin, true, move |v| {
        make_credential(&request, &v)
    })
    .await?;

    let key = key_from_attestation(options, &attestation)?;
    Ok(Enrollment {
        key,
        attestation: EnrollmentAttestation {
            format: attestation.fmt,
            authenticator_data: attestation.auth_data,
            signature: attestation.attstmt_sig,
            certificates: attestation.attstmt_x5c,
            challenge,
            aaguid: attestation.aaguid,
        },
    })
}

#[derive(Debug)]
struct MakeCredentialRequest {
    path: String,
    application: String,
    user_name: String,
    user_id: [u8; 32],
    challenge: [u8; 32],
    algorithm: SkAlgorithm,
    resident: bool,
}

fn make_credential(
    req: &MakeCredentialRequest,
    verification: &Verification,
) -> Result<ctap_hid_fido2::fidokey::make_credential::Attestation> {
    let device = open(&req.path)?;
    let user = PublicKeyCredentialUserEntity::new(
        Some(&req.user_id),
        Some(&req.user_name),
        Some(&req.user_name),
    );
    let key_type = match req.algorithm {
        SkAlgorithm::Ed25519 => CredentialSupportedKeyType::Ed25519,
        SkAlgorithm::EcdsaP256 => CredentialSupportedKeyType::Ecdsa256,
    };
    let mut builder = MakeCredentialArgsBuilder::new(&req.application, &req.challenge)
        .key_type(key_type)
        .user_entity(&user);
    if req.resident {
        builder = builder.resident_key();
    }
    builder = match verification {
        Verification::None => builder.without_pin_and_uv(),
        Verification::Pin(pin) => builder.pin(pin.as_str()),
        // `new` requests `uv: true` by default.
        Verification::BuiltIn => builder,
    };
    device
        .make_credential_with_args(&builder.build())
        .map_err(|e| map_ctap_error(&e.to_string()))
}

fn key_from_attestation(
    options: &EnrollOptions,
    attestation: &ctap_hid_fido2::fidokey::make_credential::Attestation,
) -> Result<SkKey> {
    const WHAT: &str = "makeCredential response";
    let auth_data = &attestation.auth_data;
    if auth_data.len() < 37 || auth_data[..32] != Sha256::digest(options.application.as_bytes())[..]
    {
        return Err(Error::malformed(
            WHAT,
            "rpIdHash does not match the application",
        ));
    }
    if auth_data[32] & auth_data_flags::ATTESTED_CREDENTIAL_DATA == 0 {
        return Err(Error::malformed(WHAT, "no attested credential data"));
    }
    let type_ok = matches!(
        (
            &attestation.credential_publickey.key_type,
            options.algorithm
        ),
        (PublicKeyType::Ed25519, SkAlgorithm::Ed25519)
            | (PublicKeyType::Ecdsa256, SkAlgorithm::EcdsaP256)
    );
    if !type_ok {
        return Err(Error::malformed(WHAT, "unexpected credential key type"));
    }
    SkKey::new(
        options.algorithm,
        &attestation.credential_publickey.der,
        options.application.clone(),
        options.key_flags(),
        attestation.credential_descriptor.id.clone(),
    )
}

fn resolve_single(selector: &DeviceSelector) -> Result<String> {
    match selector {
        DeviceSelector::Path(path) => Ok(path.clone()),
        DeviceSelector::Auto => {
            let mut devices = enumerate();
            match devices.len() {
                0 => Err(Error::NoDevice),
                1 => Ok(devices.remove(0).path),
                _ => Err(Error::MultipleDevices),
            }
        }
    }
}

/// Finds the authenticator that holds `key_handle` with a silent assertion (`up: false`),
/// as OpenSSH does when several keys are plugged in.
fn resolve_for_credential(
    selector: &DeviceSelector,
    application: &str,
    key_handle: &[u8],
) -> Result<String> {
    if let DeviceSelector::Path(path) = selector {
        return Ok(path.clone());
    }
    let devices = enumerate();
    match devices.len() {
        0 => Err(Error::NoDevice),
        1 => Ok(devices[0].path.clone()),
        _ => devices
            .into_iter()
            .find(|d| {
                let probe = || -> Result<()> {
                    let device = open(&d.path)?;
                    let args = GetAssertionArgsBuilder::new(application, b"probe")
                        .credential_id(key_handle)
                        .without_pin_and_uv()
                        .without_up()
                        .build();
                    device
                        .get_assertion_with_args(&args)
                        .map(|_| ())
                        .map_err(|e| map_ctap_error(&e.to_string()))
                };
                probe().is_ok()
            })
            .map(|d| d.path)
            .ok_or(Error::KeyNotFound),
    }
}

#[derive(Debug)]
struct AssertionRequest {
    path: String,
    application: String,
    key_handle: Zeroizing<Vec<u8>>,
    data: Vec<u8>,
    user_presence: bool,
}

fn get_assertion(
    req: &AssertionRequest,
    verification: &Verification,
) -> Result<(Vec<u8>, Vec<u8>)> {
    let device = open(&req.path)?;
    let mut builder =
        GetAssertionArgsBuilder::new(&req.application, &req.data).credential_id(&req.key_handle);
    builder = match verification {
        Verification::None => builder.without_pin_and_uv(),
        Verification::Pin(pin) => builder.pin(pin.as_str()),
        // `uv: true` is the builder default.
        Verification::BuiltIn => builder,
    };
    if !req.user_presence {
        builder = builder.without_up();
    }
    let assertions = device
        .get_assertion_with_args(&builder.build())
        .map_err(|e| map_ctap_error(&e.to_string()))?;
    let assertion = assertions
        .into_iter()
        .next()
        .ok_or(Error::malformed("getAssertion response", "no assertion"))?;
    if !assertion.credential_id.is_empty() && assertion.credential_id != *req.key_handle {
        return Err(Error::malformed(
            "getAssertion response",
            "wrong credential",
        ));
    }
    Ok((assertion.auth_data, assertion.signature))
}

/// Signs `data` with a security key and returns the complete `sk-*` SSH signature.
///
/// The signature is verified against the key before it is returned.
pub async fn sign(
    key: &SkKey,
    device: &DeviceSelector,
    pin: &dyn PinProvider,
    data: &[u8],
) -> Result<Vec<u8>> {
    let selector = device.clone();
    let application = key.application().to_owned();
    let handle = Zeroizing::new(key.key_handle().to_vec());
    let (path, caps) = {
        let application = application.clone();
        let handle = handle.clone();
        blocking(move || {
            let _guard = device_lock();
            let path = resolve_for_credential(&selector, &application, &handle)?;
            let caps = capabilities(&open(&path)?)?;
            Ok((path, caps))
        })
        .await?
    };

    let verification = if key.requires_user_verification() {
        required_verification(caps, pin).await?
    } else {
        Verification::None
    };

    let request = Arc::new(AssertionRequest {
        path,
        application,
        key_handle: handle,
        data: data.to_vec(),
        user_presence: key.requires_user_presence(),
    });
    let (auth_data, raw_signature) = with_pin_retry(
        verification,
        caps,
        pin,
        key.requires_user_presence(),
        move |v| get_assertion(&request, &v),
    )
    .await?;

    let signature = SkSignature::from_assertion(key, &auth_data, &raw_signature)?;
    let blob = signature.encode();
    key.verify(data, &blob)?;
    Ok(blob)
}

/// A [`HardwareSigner`] for a FIDO2 security key.
#[derive(Clone)]
pub struct Fido2Signer {
    key: SkKey,
    device: DeviceSelector,
    pin: Arc<dyn PinProvider>,
}

impl std::fmt::Debug for Fido2Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fido2Signer")
            .field("key", &self.key)
            .field("device", &self.device)
            .finish_non_exhaustive()
    }
}

impl Fido2Signer {
    /// Creates a signer for `key` using whichever authenticator holds it.
    pub fn new(key: SkKey, pin: Arc<dyn PinProvider>) -> Self {
        Self {
            key,
            device: DeviceSelector::Auto,
            pin,
        }
    }

    /// Restricts signing to one device.
    #[must_use]
    pub fn with_device(mut self, device: DeviceSelector) -> Self {
        self.device = device;
        self
    }

    /// The security key.
    pub const fn key(&self) -> &SkKey {
        &self.key
    }
}

#[async_trait::async_trait]
impl HardwareSigner for Fido2Signer {
    fn public_key(&self) -> ssh_key::PublicKey {
        self.key.public_key()
    }

    fn signature_algorithm(&self) -> &'static str {
        self.key.algorithm().ssh_name()
    }

    async fn sign(&self, data: &[u8]) -> Result<Vec<u8>> {
        sign(&self.key, &self.device, self.pin.as_ref(), data).await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn maps_ctap_status_messages() {
        let cases = [
            (
                "response_status err = 0x31 CTAP2_ERR_PIN_INVALID   PIN Invalid.",
                "PinInvalid",
            ),
            ("0x32 CTAP2_ERR_PIN_BLOCKED PIN Blocked.", "PinBlocked"),
            (
                "0x36 CTAP2_ERR_PIN_REQUIRED  PIN is required",
                "PinRequired",
            ),
            ("0x2E CTAP2_ERR_NO_CREDENTIALS    No valid", "KeyNotFound"),
            (
                "0x27 CTAP2_ERR_OPERATION_DENIED  Not authorized",
                "Cancelled",
            ),
            ("0x2F CTAP2_ERR_USER_ACTION_TIMEOUT   Timeout", "Cancelled"),
            ("0x19 CTAP2_ERR_CREDENTIAL_EXCLUDED", "AlreadyExists"),
            ("0x26 CTAP2_ERR_UNSUPPORTED_ALGORITHM", "Unsupported"),
            (
                "0x33 CTAP2_ERR_PIN_AUTH_INVALID PIN authentication",
                "Authenticator",
            ),
            ("FIDO device not found.", "Authenticator"),
        ];
        for (message, expected) in cases {
            let got = format!("{:?}", map_ctap_error(message));
            assert!(got.starts_with(expected), "{message} -> {got}");
        }
        let long = "x".repeat(1000);
        assert!(matches!(map_ctap_error(&long), Error::Authenticator(m) if m.len() == 200));
    }

    #[test]
    fn enroll_option_flags() {
        let mut options = EnrollOptions::default();
        assert_eq!(options.key_flags(), key_flags::USER_PRESENCE_REQUIRED);
        options.require_user_presence = false;
        options.require_user_verification = true;
        options.resident = true;
        assert_eq!(
            options.key_flags(),
            key_flags::USER_VERIFICATION_REQUIRED | key_flags::RESIDENT_KEY
        );
    }

    #[tokio::test]
    async fn enroll_rejects_bad_application() {
        let options = EnrollOptions {
            application: "https://example.com".into(),
            ..EnrollOptions::default()
        };
        assert!(matches!(
            enroll(&options, &crate::NoPin).await,
            Err(Error::Malformed { .. })
        ));
    }

    /// A PIN provider for hardware tests reading `HW_KEYS_TEST_PIN`.
    struct EnvPin;

    #[async_trait::async_trait]
    impl PinProvider for EnvPin {
        async fn pin(&self) -> Option<Zeroizing<String>> {
            std::env::var("HW_KEYS_TEST_PIN").ok().map(Zeroizing::new)
        }

        fn touch_prompt(&self) {
            eprintln!("touch the security key");
        }
    }

    /// Needs a FIDO2 key plugged in and touched twice per algorithm.
    /// Run with `cargo test -p hw-keys -- --ignored hardware` (optionally set
    /// `HW_KEYS_TEST_PIN`).
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires a FIDO2 security key"]
    async fn hardware_enroll_and_sign() {
        for algorithm in [SkAlgorithm::EcdsaP256, SkAlgorithm::Ed25519] {
            let options = EnrollOptions {
                algorithm,
                ..EnrollOptions::default()
            };
            let enrollment = enroll(&options, &EnvPin).await.unwrap();
            let key = enrollment.key;
            eprintln!("{}", key.public_key_openssh("hw-keys-test"));
            let signer = Fido2Signer::new(key.clone(), Arc::new(EnvPin));
            let blob = signer.sign(b"hardware test").await.unwrap();
            let sig = key.verify(b"hardware test", &blob).unwrap();
            assert!(sig.satisfies_key_policy(&key));
            // The exported file round-trips.
            let pem = key.to_openssh_private_key("hw-keys-test").unwrap();
            assert_eq!(SkKey::from_openssh_private_key(&pem).unwrap().0, key);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires a FIDO2 security key"]
    async fn hardware_list_devices() {
        let devices = list_devices().await.unwrap();
        assert!(!devices.is_empty());
    }
}
