// SPDX-License-Identifier: GPL-3.0-or-later
//! Async serial console sessions.
//!
//! * [`list_ports`] enumerates serial ports with USB vendor/product details.
//! * [`SerialConfig`] describes the line settings (baud rate, data/stop bits, parity, flow
//!   control) with validation and serde (camelCase) support for the UI.
//! * [`Session`] is an open console: received bytes arrive as [`Event`]s on a bounded
//!   channel, and the session writes, sends breaks and drives DTR/RTS.
//!
//! Sessions run over a [`SerialTransport`]. The `native` feature (default; desktop targets
//! only) provides one for OS serial ports via the `serialport` crate
//! ([`Session::open`]); other transports, such as Android USB serial implemented by the
//! app's Kotlin plugin, plug in through [`Session::from_transport`].

#![forbid(unsafe_code)]

mod config;
mod error;
#[cfg(all(feature = "native", not(any(target_os = "android", target_os = "ios"))))]
mod native;
mod port;
mod session;
mod transport;

pub use config::{
    COMMON_BAUD_RATES, DataBits, FlowControl, MAX_BAUD_RATE, MIN_BAUD_RATE, Parity, SerialConfig,
    StopBits,
};
pub use error::{Error, Result};
pub use port::{PortInfo, PortKind, list_ports};
pub use session::{CloseReason, EVENT_CHANNEL_CAPACITY, Event, MAX_BREAK, Session};
pub use transport::SerialTransport;
