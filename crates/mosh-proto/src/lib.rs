// SPDX-License-Identifier: GPL-3.0-or-later
//! Native Mosh client, wire-compatible with `mosh-server` 1.3.x / 1.4.
//!
//! * **Crypto** ([`crypto`]): AES-128-OCB3, 96-bit nonce = 4 zero bytes ‖ 64-bit big-endian
//!   value whose top bit is the direction (server → client) and whose low 63 bits are the
//!   packet sequence number. Datagram = last 8 nonce bytes ‖ ciphertext ‖ 16-byte tag.
//!   Datagrams are authenticated before anything is parsed.
//! * **Packets** ([`packet`]): plaintext = 16-bit send timestamp ‖ 16-bit timestamp reply ‖
//!   fragment.
//! * **Fragments** ([`fragment`]): instruction id ‖ fragment number (top bit = final) ‖ bytes,
//!   reassembled with bounded memory, then zlib-inflated (bounded) into a
//!   `TransportBuffers.Instruction` ([`proto`]).
//! * **SSP** ([`transport`]): each side synchronises an object to the other by sending
//!   `Instruction { old_num, new_num, ack_num, throwaway_num, diff }`. The client's object is
//!   the user input stream ([`user`]); the server's is its terminal framebuffer, sent as
//!   `HostBytes` escape sequences. Empty acks double as heartbeats; state `u64::MAX` means
//!   shutdown. The core is sans-IO ([`Transport`]).

#![forbid(unsafe_code)]

pub mod crypto;
mod error;
pub mod fragment;
mod key;
pub mod packet;
pub mod proto;
#[cfg(test)]
mod testutil;
pub mod transport;
pub mod user;

pub use error::{Error, Result};
pub use key::{MoshKey, PRINTABLE_KEY_LEN};
pub use transport::{
    CloseReason, PROTOCOL_VERSION, Stats, Transport, TransportConfig, TransportEvent,
};
