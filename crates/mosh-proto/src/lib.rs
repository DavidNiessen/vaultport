// SPDX-License-Identifier: GPL-3.0-or-later
//! Native Mosh client, wire-compatible with `mosh-server` 1.3.x / 1.4.
//!
//! * **Crypto** ([`crypto`]): AES-128-OCB3, 96-bit nonce = 4 zero bytes ‖ 64-bit big-endian
//!   value whose top bit is the direction (server → client) and whose low 63 bits are the
//!   packet sequence number. Datagram = last 8 nonce bytes ‖ ciphertext ‖ 16-byte tag.
//!   Datagrams are authenticated before anything is parsed.
//! * **Packets** ([`packet`]): plaintext = 16-bit send timestamp ‖ 16-bit timestamp reply ‖
//!   payload.

#![forbid(unsafe_code)]

pub mod crypto;
mod error;
mod key;
pub mod packet;

pub use error::{Error, Result};
pub use key::{MoshKey, PRINTABLE_KEY_LEN};
