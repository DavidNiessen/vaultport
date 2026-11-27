// SPDX-License-Identifier: GPL-3.0-or-later
//! The byte transport underneath a [`Session`](crate::Session).
//!
//! Native serial ports implement it in this crate. On Android the app implements it on top
//! of its USB serial Kotlin plugin (e.g. `usb-serial-for-android`): incoming bytes are
//! pushed from the plugin into a channel that `read` drains, and the other methods invoke
//! plugin commands.

use std::io;
use std::time::Duration;

/// A full-duplex serial byte stream with modem control.
///
/// All methods take `&self`: [`read`](Self::read) runs concurrently with the others.
#[async_trait::async_trait]
pub trait SerialTransport: Send + Sync + std::fmt::Debug + 'static {
    /// Waits for the next chunk of received bytes. `Ok(None)` means the device went away.
    ///
    /// Must be cancel-safe: if the returned future is dropped before completion, no data
    /// may be lost (receiving from a `tokio::sync::mpsc` channel satisfies this).
    async fn read(&self) -> io::Result<Option<Vec<u8>>>;

    /// Writes all of `data`.
    async fn write(&self, data: &[u8]) -> io::Result<()>;

    /// Holds the line in the break condition for `duration`.
    async fn send_break(&self, duration: Duration) -> io::Result<()>;

    /// Sets the DTR modem line.
    async fn set_dtr(&self, level: bool) -> io::Result<()>;

    /// Sets the RTS modem line.
    async fn set_rts(&self, level: bool) -> io::Result<()>;

    /// Releases the device. Called by the session when it ends; must be idempotent.
    async fn close(&self) -> io::Result<()>;
}
