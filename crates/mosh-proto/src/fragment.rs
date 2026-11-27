// SPDX-License-Identifier: GPL-3.0-or-later
//! Instruction fragmentation (`src/network/transportfragment.cc`).
//!
//! An [`Instruction`] is serialised, zlib-compressed and split into fragments that each fit
//! one datagram. Fragment wire format: `id: u64 BE || (final << 15 | num): u16 BE || bytes`.
//! Fragment ids increase whenever the instruction changes, so the receiver keeps a single
//! assembly and discards anything older than it.

use std::collections::BTreeMap;
use std::io::{Read as _, Write as _};

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use prost::Message as _;

use crate::proto::transport_buffers::Instruction;

/// Fragment header length.
pub const HEADER_LEN: usize = 10;
/// Largest fragment number (15 bits).
pub const MAX_FRAGMENT_NUM: u16 = 0x7FFF;
/// Upper bound on the compressed bytes held for one instruction being reassembled.
pub const MAX_ASSEMBLY_BYTES: usize = 1 << 20;
/// Upper bound on an instruction after decompression.
pub const MAX_INSTRUCTION_BYTES: usize = 8 << 20;

const FINAL_BIT: u16 = 0x8000;

/// One fragment of a compressed instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    /// Instruction id.
    pub id: u64,
    /// Index within the instruction (0..=0x7FFF).
    pub num: u16,
    /// Whether this is the last fragment.
    pub is_final: bool,
    /// Compressed instruction bytes.
    pub contents: Vec<u8>,
}

impl Fragment {
    /// Serialises the fragment.
    pub fn encode(&self) -> Vec<u8> {
        debug_assert!(self.num <= MAX_FRAGMENT_NUM);
        let mut out = Vec::with_capacity(HEADER_LEN + self.contents.len());
        out.extend_from_slice(&self.id.to_be_bytes());
        let combined = (self.num & MAX_FRAGMENT_NUM) | if self.is_final { FINAL_BIT } else { 0 };
        out.extend_from_slice(&combined.to_be_bytes());
        out.extend_from_slice(&self.contents);
        out
    }

    /// Parses a fragment; `None` if shorter than the header.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < HEADER_LEN {
            return None;
        }
        let mut id = [0u8; 8];
        id.copy_from_slice(&bytes[..8]);
        let combined = u16::from_be_bytes([bytes[8], bytes[9]]);
        Some(Self {
            id: u64::from_be_bytes(id),
            num: combined & MAX_FRAGMENT_NUM,
            is_final: combined & FINAL_BIT != 0,
            contents: bytes[HEADER_LEN..].to_vec(),
        })
    }
}

/// Splits instructions into fragments, assigning ids like mosh's `Fragmenter`.
#[derive(Debug, Default)]
pub struct Fragmenter {
    next_id: u64,
    last: Option<(Instruction, usize)>,
}

impl Fragmenter {
    /// Creates a fragmenter (the first instruction gets id 1).
    pub fn new() -> Self {
        Self::default()
    }

    /// Compresses and fragments `inst`; each fragment's encoding is at most `mtu` bytes
    /// (`mtu` must exceed [`HEADER_LEN`]).
    pub fn make_fragments(&mut self, inst: &Instruction, mtu: usize) -> Vec<Fragment> {
        let chunk = mtu.saturating_sub(HEADER_LEN).max(1);
        let same = self.last.as_ref().is_some_and(|(last, last_mtu)| {
            *last_mtu == chunk
                && last.old_num == inst.old_num
                && last.new_num == inst.new_num
                && last.ack_num == inst.ack_num
                && last.throwaway_num == inst.throwaway_num
                && last.chaff == inst.chaff
                && last.protocol_version == inst.protocol_version
        });
        if !same {
            self.next_id += 1;
        }
        self.last = Some((inst.clone(), chunk));
        let payload = compress(&inst.encode_to_vec());
        let pieces: Vec<&[u8]> = if payload.is_empty() {
            vec![&[][..]]
        } else {
            payload.chunks(chunk).collect()
        };
        let n = pieces.len();
        pieces
            .into_iter()
            .enumerate()
            .map(|(i, c)| Fragment {
                id: self.next_id,
                num: (i as u16) & MAX_FRAGMENT_NUM,
                is_final: i + 1 == n,
                contents: c.to_vec(),
            })
            .collect()
    }
}

/// Reassembles fragments of the newest instruction with bounded memory.
///
/// Duplicates are ignored, fragments of older instructions are dropped, and an instruction
/// whose fragments are inconsistent or exceed [`MAX_ASSEMBLY_BYTES`] is abandoned.
#[derive(Debug, Default)]
pub struct FragmentAssembly {
    current_id: Option<u64>,
    /// Current id is finished (completed or abandoned); ignore its fragments.
    done: bool,
    fragments: BTreeMap<u16, Vec<u8>>,
    total: Option<u16>,
    bytes: usize,
}

impl FragmentAssembly {
    /// Creates an empty assembly.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a fragment; returns the complete compressed instruction once all fragments of it
    /// have arrived.
    pub fn add(&mut self, frag: Fragment) -> Option<Vec<u8>> {
        match self.current_id {
            Some(id) if frag.id < id => return None,
            Some(id) if frag.id == id => {
                if self.done {
                    return None;
                }
            }
            _ => {
                self.current_id = Some(frag.id);
                self.done = false;
                self.fragments.clear();
                self.total = None;
                self.bytes = 0;
            }
        }
        if self.fragments.contains_key(&frag.num) {
            return None;
        }
        if self.total.is_some_and(|total| frag.num >= total) {
            self.abandon();
            return None;
        }
        if frag.is_final {
            let total = frag.num + 1;
            if self
                .fragments
                .keys()
                .next_back()
                .is_some_and(|&k| k >= total)
            {
                self.abandon();
                return None;
            }
            self.total = Some(total);
        }
        self.bytes += frag.contents.len();
        if self.bytes > MAX_ASSEMBLY_BYTES {
            self.abandon();
            return None;
        }
        self.fragments.insert(frag.num, frag.contents);
        if self
            .total
            .is_some_and(|t| usize::from(t) == self.fragments.len())
        {
            let mut out = Vec::with_capacity(self.bytes);
            for part in std::mem::take(&mut self.fragments).into_values() {
                out.extend_from_slice(&part);
            }
            self.done = true;
            self.total = None;
            self.bytes = 0;
            return Some(out);
        }
        None
    }

    /// Bytes currently buffered.
    pub fn buffered_bytes(&self) -> usize {
        self.bytes
    }

    fn abandon(&mut self) {
        self.done = true;
        self.fragments.clear();
        self.total = None;
        self.bytes = 0;
    }
}

/// zlib-compresses (RFC 1950, default level), as mosh's `Compressor::compress_str`.
pub fn compress(data: &[u8]) -> Vec<u8> {
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    // Writing to a Vec cannot fail.
    enc.write_all(data).expect("in-memory zlib write");
    enc.finish().expect("in-memory zlib finish")
}

/// Why an assembled instruction was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    /// Not valid zlib data.
    #[error("invalid zlib stream")]
    Inflate,
    /// Inflated size exceeds [`MAX_INSTRUCTION_BYTES`].
    #[error("instruction too large")]
    TooLarge,
    /// Not a valid `TransportBuffers.Instruction`.
    #[error("invalid instruction protobuf")]
    Protobuf,
}

/// Inflates (bounded) and parses an assembled instruction.
pub fn decode_instruction(compressed: &[u8]) -> Result<Instruction, DecodeError> {
    let raw = inflate_bounded(compressed, MAX_INSTRUCTION_BYTES)?;
    Instruction::decode(&raw[..]).map_err(|_| DecodeError::Protobuf)
}

/// Inflates a zlib stream, refusing output larger than `limit`.
pub fn inflate_bounded(compressed: &[u8], limit: usize) -> Result<Vec<u8>, DecodeError> {
    let mut out = Vec::new();
    ZlibDecoder::new(compressed)
        .take(limit as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|_| DecodeError::Inflate)?;
    if out.len() > limit {
        return Err(DecodeError::TooLarge);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn inst(new_num: u64, diff: Vec<u8>) -> Instruction {
        Instruction {
            protocol_version: Some(2),
            old_num: Some(0),
            new_num: Some(new_num),
            ack_num: Some(0),
            throwaway_num: Some(0),
            diff: Some(diff),
            chaff: Some(vec![]),
        }
    }

    /// Deterministic incompressible bytes.
    fn noise(n: usize) -> Vec<u8> {
        let mut x: u32 = 0x1234_5678;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            })
            .collect()
    }

    #[test]
    fn fragment_header_encoding() {
        let f = Fragment {
            id: 0x0102_0304_0506_0708,
            num: 3,
            is_final: true,
            contents: b"xy".to_vec(),
        };
        let b = f.encode();
        assert_eq!(b, [1, 2, 3, 4, 5, 6, 7, 8, 0x80, 3, b'x', b'y']);
        assert_eq!(Fragment::decode(&b).unwrap(), f);
        assert!(Fragment::decode(&b[..9]).is_none());
    }

    #[test]
    fn ids_follow_mosh_rules() {
        let mut fr = Fragmenter::new();
        let a = fr.make_fragments(&inst(1, b"a".to_vec()), 500);
        assert_eq!(a[0].id, 1);
        // Same header fields -> same id (a retransmission).
        let b = fr.make_fragments(&inst(1, b"a".to_vec()), 500);
        assert_eq!(b[0].id, 1);
        let c = fr.make_fragments(&inst(2, b"a".to_vec()), 500);
        assert_eq!(c[0].id, 2);
    }

    #[test]
    fn split_and_reassemble_in_order() {
        let i = inst(5, noise(3000));
        let frags = Fragmenter::new().make_fragments(&i, 472);
        assert!(frags.len() > 5);
        assert!(frags.iter().all(|f| f.encode().len() <= 472));
        assert!(frags.last().unwrap().is_final);
        assert!(frags[..frags.len() - 1].iter().all(|f| !f.is_final));
        let mut asm = FragmentAssembly::new();
        let mut out = None;
        for f in frags {
            assert!(out.is_none());
            out = asm.add(f);
        }
        assert_eq!(decode_instruction(&out.unwrap()).unwrap(), i);
    }

    #[test]
    fn out_of_order_and_duplicates() {
        let i = inst(5, noise(2000));
        let mut frags = Fragmenter::new().make_fragments(&i, 300);
        frags.reverse();
        frags.swap(1, 3);
        let mut asm = FragmentAssembly::new();
        let mut results = Vec::new();
        for f in frags {
            // Every fragment arrives twice; completion happens exactly once.
            results.extend(asm.add(f.clone()));
            results.extend(asm.add(f));
        }
        assert_eq!(results.len(), 1);
        assert_eq!(decode_instruction(&results[0]).unwrap(), i);
    }

    #[test]
    fn newer_instruction_replaces_older_and_older_is_dropped() {
        let mut fr = Fragmenter::new();
        let old = fr.make_fragments(&inst(1, noise(1000)), 300);
        let new = fr.make_fragments(&inst(2, b"new".to_vec()), 300);
        let mut asm = FragmentAssembly::new();
        assert!(asm.add(old[0].clone()).is_none());
        let done = asm.add(new[0].clone()).unwrap();
        assert_eq!(decode_instruction(&done).unwrap().new_num, Some(2));
        for f in old {
            assert!(asm.add(f).is_none());
        }
    }

    #[test]
    fn inconsistent_final_abandons() {
        let mut asm = FragmentAssembly::new();
        let f = |num, is_final| Fragment {
            id: 9,
            num,
            is_final,
            contents: vec![1],
        };
        assert!(asm.add(f(3, false)).is_none());
        // Final at 1 contradicts fragment 3.
        assert!(asm.add(f(1, true)).is_none());
        assert!(asm.add(f(0, false)).is_none());
        assert_eq!(asm.buffered_bytes(), 0);
        // Fragment beyond a known final.
        let mut asm = FragmentAssembly::new();
        assert!(
            asm.add(Fragment {
                id: 1,
                ..f(1, true)
            })
            .is_none()
        );
        assert!(
            asm.add(Fragment {
                id: 1,
                ..f(2, false)
            })
            .is_none()
        );
        assert!(
            asm.add(Fragment {
                id: 1,
                ..f(0, false)
            })
            .is_none()
        );
    }

    #[test]
    fn oversized_assembly_dropped() {
        let mut asm = FragmentAssembly::new();
        let big = vec![0u8; 60_000];
        let mut num = 0;
        while asm.buffered_bytes() + big.len() <= MAX_ASSEMBLY_BYTES {
            assert!(
                asm.add(Fragment {
                    id: 1,
                    num,
                    is_final: false,
                    contents: big.clone(),
                })
                .is_none()
            );
            num += 1;
        }
        assert!(
            asm.add(Fragment {
                id: 1,
                num,
                is_final: true,
                contents: big.clone(),
            })
            .is_none()
        );
        assert_eq!(asm.buffered_bytes(), 0);
        // Remaining fragments of that id are ignored.
        assert!(
            asm.add(Fragment {
                id: 1,
                num: num + 1,
                is_final: true,
                contents: vec![],
            })
            .is_none()
        );
    }

    #[test]
    fn decompression_bomb_rejected() {
        let bomb = compress(&vec![0u8; MAX_INSTRUCTION_BYTES + 1]);
        assert_eq!(decode_instruction(&bomb), Err(DecodeError::TooLarge));
        assert_eq!(decode_instruction(b"not zlib"), Err(DecodeError::Inflate));
        assert_eq!(
            decode_instruction(&compress(&[0xFF, 0xFF])),
            Err(DecodeError::Protobuf)
        );
    }
}
