// SPDX-License-Identifier: GPL-3.0-or-later
//! Telnet client (RFC 854) for the Vaultport terminal.
//!
//! # Protocol overview
//!
//! Telnet is a byte stream over TCP in which `IAC` (0xFF) introduces commands:
//!
//! * `IAC IAC` is a literal 0xFF data byte (applied in both directions).
//! * `IAC WILL|WONT|DO|DONT <opt>` negotiate options. `WILL`/`WONT` concern the sender's own
//!   behaviour, `DO`/`DONT` ask the receiver. Negotiation follows the RFC 1143 Q method
//!   ([`qmethod`]), which cannot loop.
//! * `IAC SB <opt> … IAC SE` carries option parameters (bounded in size here).
//! * Outside BINARY mode the stream is NVT ASCII: a CR is always followed by LF (end of line)
//!   or NUL (bare carriage return). Received `CR NUL` is folded to `CR`; a bare CR typed by the
//!   user is sent as `CR LF` (configurable).
//!
//! Supported options: ECHO (RFC 857, server side), SUPPRESS-GO-AHEAD (858), BINARY (856),
//! TERMINAL-TYPE (1091, default `xterm-256color`), NAWS (1073, sent on every resize) and
//! NEW-ENVIRON (1572, `USER` only). Everything else is refused.
//!
//! The protocol is implemented as a sans-IO [`Codec`]; [`Session`] is a thin tokio driver
//! over TCP or any other `AsyncRead + AsyncWrite` stream, shaped like an SSH shell channel
//! (`write`, `resize`, `close` plus an event channel).

#![forbid(unsafe_code)]

pub mod codec;
pub mod consts;
mod error;
pub mod qmethod;
mod session;

pub use codec::{Codec, CodecEvent, Config, Newline, Side, escape_iac};
pub use consts::TelnetOption;
pub use error::{Error, Result};
pub use session::{CloseReason, Event, Session, SessionConfig, SessionEvents, Status};
