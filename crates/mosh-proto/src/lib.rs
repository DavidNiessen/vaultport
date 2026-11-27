// SPDX-License-Identifier: GPL-3.0-or-later
//! Native Mosh client, wire-compatible with `mosh-server` 1.3.x / 1.4.
//!
//! # Protocol overview
//!
//! Mosh runs over UDP after an SSH bootstrap: the app executes `mosh-server new` over SSH,
//! which prints `MOSH CONNECT <port> <key>` ([`parse_mosh_connect`]) and detaches. Everything
//! after that is this crate.
//!
//! ```text
//!  user input ─► UserStream ─► TransportSender ─► Instruction ─► zlib ─► Fragments
//!                (SSP state)   (diffs, acks,      (protobuf)              │
//!                               timers)                                    ▼
//!            UDP datagram ◄── AES-128-OCB3 seal ◄── timestamps ◄── Packet (seq, dir)
//! ```
//!
//! * **Crypto** ([`crypto`]): AES-128-OCB3, 96-bit nonce = 4 zero bytes ‖ 64-bit big-endian
//!   value whose top bit is the direction (server → client) and whose low 63 bits are the
//!   packet sequence number. Datagram = last 8 nonce bytes ‖ ciphertext ‖ 16-byte tag.
//!   Datagrams are authenticated before anything is parsed; failures are dropped silently.
//! * **Packets** ([`packet`]): plaintext = 16-bit send timestamp ‖ 16-bit timestamp reply ‖
//!   fragment. Timestamp echoes drive the SRTT/RTTVAR estimator (RTO 50–1000 ms). A sliding
//!   window rejects replayed and wrong-direction packets.
//! * **Fragments** ([`fragment`]): instruction id ‖ fragment number (top bit = final) ‖ bytes,
//!   reassembled with bounded memory, then zlib-inflated (bounded) into a
//!   `TransportBuffers.Instruction` ([`proto`]).
//! * **SSP** ([`transport`]): each side synchronises an object to the other by sending
//!   `Instruction { old_num, new_num, ack_num, throwaway_num, diff }`. The client's object is
//!   the user input stream ([`user`]; `Keystroke` / `ResizeMessage` extensions of
//!   `ClientBuffers.Instruction`); the server's is its terminal framebuffer, sent as
//!   `HostBytes` escape sequences that can be fed straight to a terminal emulator such as
//!   xterm.js. Empty acks double as 3-second heartbeats; state `u64::MAX` means shutdown.
//! * **Roaming**: the server sends to the source address of the newest authentic datagram,
//!   so the client may switch local sockets at any time ([`Session::rebind`]); like
//!   mosh-client it also hops ports automatically after 10 s without a round trip.
//!
//! The protocol core is sans-IO ([`Transport`]); [`Session`] is a small tokio UDP driver with
//! the same shape as the other shell back-ends (`write`, `resize`, `close`, event channel).

#![forbid(unsafe_code)]

mod bootstrap;
pub mod crypto;
mod error;
pub mod fragment;
mod key;
pub mod packet;
pub mod proto;
mod session;
#[cfg(test)]
mod testutil;
pub mod transport;
pub mod user;

pub use bootstrap::{find_mosh_connect, parse_mosh_connect};
pub use error::{Error, Result};
pub use key::{MoshKey, PRINTABLE_KEY_LEN};
pub use session::{Event, Session, SessionConfig, SessionEvents, Status};
pub use transport::{
    CloseReason, PROTOCOL_VERSION, Stats, Transport, TransportConfig, TransportEvent,
};
