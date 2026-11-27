// SPDX-License-Identifier: GPL-3.0-or-later
//! Hardware-backed SSH keys.
//!
//! Two families of keys are supported, both exposed through [`HardwareSigner`], which
//! returns complete SSH wire signatures so the SSH engine can use them like any other key:
//!
//! * **FIDO2 security keys** in OpenSSH's `sk-ssh-ed25519@openssh.com` and
//!   `sk-ecdsa-sha2-nistp256@openssh.com` formats ([`sk`]), compatible with OpenSSH ≥ 8.4.
//!   On desktop targets the `fido2-hid` feature talks CTAP2 over USB HID
//!   ([`fido2`]): enrollment, signing, and export to an OpenSSH private key file.
//! * **Platform-bound keys** ([`platform`]): non-exportable keys gated by biometrics in the
//!   Secure Enclave (macOS/iOS), Windows Hello, or Android Keystore (through an app-provided
//!   bridge). Linux has no backend (TPM2 is out of scope) and reports
//!   [`Error::Unsupported`].
//!
//! Format conversions ([`ecdsa`], [`pubkey`], [`sk`]) are pure Rust, strict, never panic on
//! untrusted input and are tested on every platform.
//!
//! # Security notes
//!
//! * PINs are requested through [`PinProvider`] and held in [`zeroize::Zeroizing`] buffers.
//! * Key handles are zeroized on drop and redacted from `Debug` output.
//! * Every signature produced by an authenticator or platform key store is verified
//!   against the public key before it is returned, so a misbehaving device cannot make us
//!   send garbage (or a signature by a different key) to a server.

#![forbid(unsafe_code)]

pub mod ecdsa;
mod error;
#[cfg(all(
    feature = "fido2-hid",
    not(any(target_os = "android", target_os = "ios"))
))]
pub mod fido2;
mod pin;
pub mod platform;
pub mod pubkey;
mod signer;
pub mod sk;
#[cfg(any(
    all(
        feature = "fido2-hid",
        not(any(target_os = "android", target_os = "ios"))
    ),
    target_os = "macos",
    target_os = "ios",
    windows
))]
mod task;
mod wire;

pub use error::{Error, Result};
pub use pin::{NoPin, PinProvider};
pub use signer::HardwareSigner;
pub use sk::{SkAlgorithm, SkKey, SkSignature};
pub use ssh_key;
pub use zeroize;
