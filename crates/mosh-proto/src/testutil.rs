// SPDX-License-Identifier: GPL-3.0-or-later
//! A minimal fake `mosh-server` built from the crate's own primitives (tests only).

#![allow(clippy::unwrap_used, dead_code)]

use prost::Message as _;

use crate::crypto::Cipher;
use crate::fragment::{Fragment, FragmentAssembly, Fragmenter, decode_instruction};
use crate::key::MoshKey;
use crate::packet::{Direction, Packet, TIMESTAMP_NONE};
use crate::proto::client_buffers::UserMessage;
use crate::proto::host_buffers::{
    HostBytes, HostMessage, Instruction as HostInstruction, ResizeMessage,
};
use crate::proto::transport_buffers::Instruction;
use crate::transport::PROTOCOL_VERSION;

pub const TEST_KEY: &str = "AAECAwQFBgcICQoLDA0ODw";

pub fn key() -> MoshKey {
    MoshKey::parse(TEST_KEY).unwrap()
}

/// Decoded user input of one client instruction.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct UserInput {
    pub keys: Vec<u8>,
    pub resizes: Vec<(i32, i32)>,
}

pub fn user_input(diff: &[u8]) -> UserInput {
    let msg = UserMessage::decode(diff).unwrap();
    let mut out = UserInput::default();
    for i in msg.instruction {
        if let Some(k) = i.keystroke {
            out.keys.extend(k.keys.unwrap_or_default());
        }
        if let Some(r) = i.resize {
            out.resizes.push((r.width.unwrap(), r.height.unwrap()));
        }
    }
    out
}

pub fn host_bytes(bytes: &[u8]) -> Vec<u8> {
    HostMessage {
        instruction: vec![HostInstruction {
            hostbytes: Some(HostBytes {
                hoststring: Some(bytes.to_vec()),
            }),
            resize: None,
            echoack: None,
        }],
    }
    .encode_to_vec()
}

pub fn host_resize(width: i32, height: i32) -> Vec<u8> {
    HostMessage {
        instruction: vec![HostInstruction {
            hostbytes: None,
            resize: Some(ResizeMessage {
                width: Some(width),
                height: Some(height),
            }),
            echoack: None,
        }],
    }
    .encode_to_vec()
}

pub fn inst(old: u64, new: u64, ack: u64, diff: Vec<u8>) -> Instruction {
    Instruction {
        protocol_version: Some(PROTOCOL_VERSION),
        old_num: Some(old),
        new_num: Some(new),
        ack_num: Some(ack),
        throwaway_num: Some(0),
        diff: Some(diff),
        chaff: Some(vec![]),
    }
}

/// Server half of the protocol: decrypts client datagrams, reassembles instructions and seals
/// server → client datagrams.
pub struct FakeServer {
    cipher: Cipher,
    pub seq: u64,
    fragmenter: Fragmenter,
    assembly: FragmentAssembly,
    pub last_client_timestamp: u16,
}

impl FakeServer {
    pub fn new() -> Self {
        Self {
            cipher: Cipher::new(&key()),
            seq: 0,
            fragmenter: Fragmenter::new(),
            assembly: FragmentAssembly::new(),
            last_client_timestamp: TIMESTAMP_NONE,
        }
    }

    /// Opens a client datagram; returns the completed instruction, if any.
    pub fn receive(&mut self, wire: &[u8]) -> Option<Instruction> {
        let pkt = Packet::open(&self.cipher, wire).unwrap();
        assert_eq!(pkt.direction, Direction::ToServer);
        self.last_client_timestamp = pkt.timestamp;
        let frag = Fragment::decode(&pkt.payload).unwrap();
        if frag.num == 0 && frag.is_final {
            // Single-fragment instruction: decode directly so that retransmissions that reuse
            // the fragment id (identical instruction) are still observable by tests.
            return Some(decode_instruction(&frag.contents).unwrap());
        }
        self.assembly
            .add(frag)
            .map(|c| decode_instruction(&c).unwrap())
    }

    /// Seals an instruction into server → client datagrams.
    pub fn send(&mut self, inst: &Instruction, timestamp_reply: u16) -> Vec<Vec<u8>> {
        self.send_mtu(inst, timestamp_reply, 472)
    }

    pub fn send_mtu(
        &mut self,
        inst: &Instruction,
        timestamp_reply: u16,
        mtu: usize,
    ) -> Vec<Vec<u8>> {
        self.fragmenter
            .make_fragments(inst, mtu)
            .into_iter()
            .map(|f| {
                let p = Packet {
                    direction: Direction::ToClient,
                    seq: self.seq,
                    timestamp: 1000,
                    timestamp_reply,
                    payload: f.encode(),
                };
                self.seq += 1;
                p.seal(&self.cipher)
            })
            .collect()
    }

    /// Seals raw payload bytes with an arbitrary direction and sequence number.
    pub fn seal_raw(&self, direction: Direction, seq: u64, payload: Vec<u8>) -> Vec<u8> {
        Packet {
            direction,
            seq,
            timestamp: TIMESTAMP_NONE,
            timestamp_reply: TIMESTAMP_NONE,
            payload,
        }
        .seal(&self.cipher)
    }
}
