// SPDX-License-Identifier: GPL-3.0-or-later
//! Network-layer packets (`src/network/network.cc`).
//!
//! Plaintext layout: `timestamp: u16 BE || timestamp_reply: u16 BE || payload`, where the
//! timestamps are the low 16 bits of a millisecond clock and `0xFFFF` means "none". The
//! direction and sequence number travel in the nonce (see [`crate::crypto`]).

use crate::crypto::{AuthError, Cipher, SEQUENCE_MASK, TO_CLIENT};

/// "No timestamp" marker.
pub const TIMESTAMP_NONE: u16 = u16::MAX;
/// Plaintext header length (two timestamps).
pub const HEADER_LEN: usize = 4;

/// Which way a packet travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Client → server (direction bit clear).
    ToServer,
    /// Server → client (direction bit set).
    ToClient,
}

/// A decrypted network packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    /// Direction from the nonce.
    pub direction: Direction,
    /// 63-bit sequence number from the nonce.
    pub seq: u64,
    /// Sender's 16-bit millisecond clock, or [`TIMESTAMP_NONE`].
    pub timestamp: u16,
    /// Echo of the peer's last timestamp, corrected for hold time, or [`TIMESTAMP_NONE`].
    pub timestamp_reply: u16,
    /// Fragment bytes.
    pub payload: Vec<u8>,
}

/// Why a datagram was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PacketError {
    /// Bad tag, wrong key or truncated datagram.
    #[error(transparent)]
    Auth(#[from] AuthError),
    /// Authentic but too short to hold the timestamps.
    #[error("packet too short")]
    TooShort,
}

impl Packet {
    /// Encrypts the packet into a wire datagram. `seq` is masked to 63 bits.
    pub fn seal(&self, cipher: &Cipher) -> Vec<u8> {
        let dir = match self.direction {
            Direction::ToServer => 0,
            Direction::ToClient => TO_CLIENT,
        };
        let mut pt = Vec::with_capacity(HEADER_LEN + self.payload.len());
        pt.extend_from_slice(&self.timestamp.to_be_bytes());
        pt.extend_from_slice(&self.timestamp_reply.to_be_bytes());
        pt.extend_from_slice(&self.payload);
        cipher.seal(dir | (self.seq & SEQUENCE_MASK), &pt)
    }

    /// Authenticates and decodes a wire datagram.
    pub fn open(cipher: &Cipher, wire: &[u8]) -> Result<Self, PacketError> {
        let (value, mut pt) = cipher.open(wire)?;
        if pt.len() < HEADER_LEN {
            return Err(PacketError::TooShort);
        }
        let direction = if value & TO_CLIENT == 0 {
            Direction::ToServer
        } else {
            Direction::ToClient
        };
        let timestamp = u16::from_be_bytes([pt[0], pt[1]]);
        let timestamp_reply = u16::from_be_bytes([pt[2], pt[3]]);
        pt.drain(..HEADER_LEN);
        Ok(Self {
            direction,
            seq: value & SEQUENCE_MASK,
            timestamp,
            timestamp_reply,
            payload: pt,
        })
    }
}

/// Wrapping difference of two 16-bit timestamps (`timestamp_diff` in mosh).
pub fn timestamp_diff(newer: u16, older: u16) -> u16 {
    newer.wrapping_sub(older)
}

/// Low 16 bits of a millisecond clock, never [`TIMESTAMP_NONE`] (`timestamp16` in mosh).
pub fn timestamp16(ms: u64) -> u16 {
    let ts = (ms & 0xFFFF) as u16;
    if ts == TIMESTAMP_NONE { 0 } else { ts }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::key::MoshKey;

    #[test]
    fn round_trip_both_directions() {
        let c = Cipher::new(&MoshKey::from_bytes([3; 16]));
        for direction in [Direction::ToServer, Direction::ToClient] {
            let p = Packet {
                direction,
                seq: 0x1234_5678_9abc,
                timestamp: 7,
                timestamp_reply: TIMESTAMP_NONE,
                payload: b"frag".to_vec(),
            };
            let wire = p.seal(&c);
            assert_eq!(wire.len(), 8 + 4 + 4 + 16);
            assert_eq!(wire[0] & 0x80 != 0, direction == Direction::ToClient);
            assert_eq!(Packet::open(&c, &wire).unwrap(), p);
        }
    }

    #[test]
    fn short_authentic_plaintext_rejected() {
        let c = Cipher::new(&MoshKey::from_bytes([3; 16]));
        let wire = c.seal(1, b"abc");
        assert_eq!(Packet::open(&c, &wire), Err(PacketError::TooShort));
    }

    #[test]
    fn timestamps() {
        assert_eq!(timestamp16(0x1_FFFF), 0);
        assert_eq!(timestamp16(0x1_0005), 5);
        assert_eq!(timestamp_diff(3, 0xFFFE), 5);
    }
}
