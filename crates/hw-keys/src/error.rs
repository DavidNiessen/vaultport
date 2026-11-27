// SPDX-License-Identifier: GPL-3.0-or-later
//! Error type shared by all hardware key backends.

/// Errors produced by hardware key operations.
///
/// Messages never contain PINs, key handles or other secret material.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The operation is not available on this platform or with this build.
    #[error("not supported: {0}")]
    Unsupported(&'static str),
    /// No FIDO2 authenticator is connected.
    #[error("no FIDO2 security key found")]
    NoDevice,
    /// Several authenticators are connected and none was selected.
    #[error("more than one FIDO2 security key is connected; select one")]
    MultipleDevices,
    /// The authenticator requires a PIN and the [`PinProvider`](crate::PinProvider) gave none.
    #[error("the security key requires a PIN")]
    PinRequired,
    /// The PIN was wrong. The authenticator decrements its retry counter on every failure.
    #[error("incorrect PIN")]
    PinInvalid,
    /// The authenticator is locked after too many wrong PINs (or needs a power cycle).
    #[error("the security key PIN is blocked")]
    PinBlocked,
    /// User verification is required but the authenticator has neither a PIN nor built-in
    /// user verification configured.
    #[error("the security key has no PIN or biometric configured")]
    VerificationUnavailable,
    /// The user cancelled or did not confirm presence in time.
    #[error("cancelled or timed out waiting for the user")]
    Cancelled,
    /// The key (credential, key handle or platform reference) does not exist on the device.
    #[error("key not found on this device")]
    KeyNotFound,
    /// A credential for this application already exists (exclude list or name clash).
    #[error("a key with this name already exists")]
    AlreadyExists,
    /// An authenticator or transport level failure.
    #[error("security key error: {0}")]
    Authenticator(String),
    /// A failure reported by the platform key store (Secure Enclave, Windows Hello, Android).
    #[error("platform key store error: {0}")]
    Platform(String),
    /// Input could not be decoded.
    #[error("malformed {what}: {reason}")]
    Malformed {
        /// What was being decoded.
        what: &'static str,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// A signature did not verify against the key it claims to come from.
    #[error("signature verification failed")]
    InvalidSignature,
    /// Error from the `ssh-key` crate while building key or signature objects.
    #[error("SSH key encoding error: {0}")]
    SshKey(#[from] ssh_key::Error),
    /// The operating system random number generator failed.
    #[error("random number generator failure")]
    Rng,
    /// A blocking worker task panicked or was cancelled.
    #[error("background task failed")]
    Task,
}

impl Error {
    pub(crate) const fn malformed(what: &'static str, reason: &'static str) -> Self {
        Self::Malformed { what, reason }
    }
}

/// Result alias for this crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Fills `buf` from the OS random number generator.
pub(crate) fn fill_random(buf: &mut [u8]) -> Result<()> {
    getrandom::fill(buf).map_err(|_| Error::Rng)
}
