// SPDX-License-Identifier: GPL-3.0-or-later
//! User interaction callbacks for FIDO2 operations.

use zeroize::Zeroizing;

/// Supplies PINs and user prompts for FIDO2 operations. Implemented by the app (a native
/// dialog, never the web UI, so the PIN does not cross the IPC boundary).
#[async_trait::async_trait]
pub trait PinProvider: Send + Sync {
    /// Asks the user for the authenticator PIN. Return `None` if the user cancelled.
    ///
    /// Called at most once per operation; a wrong PIN is reported as
    /// [`Error::PinInvalid`](crate::Error::PinInvalid) rather than retried automatically,
    /// because every failure consumes one of the authenticator's limited PIN retries.
    async fn pin(&self) -> Option<Zeroizing<String>>;

    /// Called right before the authenticator waits for a touch ("Touch your security key").
    fn touch_prompt(&self);

    /// Called when the authenticator operation finished (successfully or not), so the
    /// touch prompt can be dismissed.
    fn touch_done(&self) {}
}

/// A [`PinProvider`] that never supplies a PIN and ignores prompts. Useful for keys
/// enrolled without user verification and for tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoPin;

#[async_trait::async_trait]
impl PinProvider for NoPin {
    async fn pin(&self) -> Option<Zeroizing<String>> {
        None
    }

    fn touch_prompt(&self) {}
}
