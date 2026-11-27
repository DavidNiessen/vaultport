// SPDX-License-Identifier: GPL-3.0-or-later
//! Telnet client (RFC 854) for the ssh-client terminal.
//!
//! Options are negotiated with `IAC WILL|WONT|DO|DONT <opt>`: `WILL`/`WONT` concern the
//! sender's own behaviour, `DO`/`DONT` ask the receiver. Negotiation follows the RFC 1143
//! Q method ([`qmethod`]), which cannot loop.

#![forbid(unsafe_code)]

pub mod consts;
pub mod qmethod;

pub use consts::TelnetOption;
