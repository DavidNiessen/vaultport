// SPDX-License-Identifier: GPL-3.0-or-later
//! On-disk format (version 1).
//!
//! ```text
//! file    = magic(8) log_id(16) record*
//! magic   = "SSHCLOG" 0x01
//! record  = len(u32 LE) sealed(len)
//! sealed  = SymmetricKey::seal(aad, chunk)          // XChaCha20-Poly1305
//! aad     = "ssh-client/log/v1|{log_id}|{chunk_index}"
//! chunk   = kind(1) body
//!   kind 0 header: asciicast v2 header JSON          (always chunk 0)
//!   kind 1 data:   raw DEFLATE of asciicast event lines, each ending in "\n"
//!   kind 2 end:    summary JSON, `chunkCount` == its own index (always last)
//! ```
//!
//! Every chunk is authenticated together with the log id and its position, so modified,
//! swapped, reordered or foreign chunks fail to decrypt; a missing end chunk reveals
//! truncation, and anything after it is rejected.

use crate::LogError;
use crate::meta::LogId;
use serde::{Deserialize, Serialize};
use std::io::Read;
use vault::SymmetricKey;

/// File magic, including the format version byte.
pub const MAGIC: &[u8; 8] = b"SSHCLOG\x01";
/// Length of the plaintext preamble (magic + log id).
pub const PREAMBLE_LEN: usize = 8 + 16;
/// Largest accepted sealed record.
pub(crate) const MAX_RECORD: usize = 8 * 1024 * 1024;
/// Largest accepted decompressed data chunk.
pub(crate) const MAX_CHUNK_PLAIN: usize = 16 * 1024 * 1024;

pub(crate) const KIND_HEADER: u8 = 0;
pub(crate) const KIND_DATA: u8 = 1;
pub(crate) const KIND_END: u8 = 2;

/// Totals written into the end chunk and recomputed by the reader.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSummary {
    /// Number of chunks before the end chunk (header and data chunks).
    pub chunk_count: u64,
    /// Time of the last event, in seconds.
    pub duration: f64,
    /// Bytes of recorded output (UTF-8 as stored).
    pub output_bytes: u64,
    /// Bytes of recorded input.
    pub input_bytes: u64,
    /// Number of events of all kinds.
    pub events: u64,
}

pub(crate) fn aad(id: &LogId, index: u64) -> Vec<u8> {
    format!("ssh-client/log/v1|{id}|{index}").into_bytes()
}

pub(crate) fn preamble(id: &LogId) -> [u8; PREAMBLE_LEN] {
    let mut p = [0u8; PREAMBLE_LEN];
    p[..8].copy_from_slice(MAGIC);
    p[8..].copy_from_slice(id.as_uuid().as_bytes());
    p
}

/// Reads the log id from the start of a log file (without authenticating anything).
pub fn read_log_id(src: &mut impl Read) -> Result<LogId, LogError> {
    let mut p = [0u8; PREAMBLE_LEN];
    src.read_exact(&mut p).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            LogError::BadMagic
        } else {
            LogError::Io(e)
        }
    })?;
    if &p[..8] != MAGIC {
        return Err(LogError::BadMagic);
    }
    let mut id = [0u8; 16];
    id.copy_from_slice(&p[8..]);
    Ok(LogId::from_uuid(uuid::Uuid::from_bytes(id)))
}

pub(crate) fn seal_chunk(
    key: &SymmetricKey,
    id: &LogId,
    index: u64,
    kind: u8,
    body: &[u8],
) -> Result<Vec<u8>, LogError> {
    let mut plain = Vec::with_capacity(1 + body.len());
    plain.push(kind);
    plain.extend_from_slice(body);
    let blob = key.seal(&aad(id, index), &plain)?;
    let len = u32::try_from(blob.len())
        .ok()
        .filter(|&l| l as usize <= MAX_RECORD)
        .ok_or_else(|| LogError::Corrupt("chunk too large".into()))?;
    let mut rec = Vec::with_capacity(4 + blob.len());
    rec.extend_from_slice(&len.to_le_bytes());
    rec.extend_from_slice(&blob);
    Ok(rec)
}

/// Reads the next record. `Ok(None)` at a clean end of file; `Err(Truncated)` for a partial
/// record.
pub(crate) fn read_record(src: &mut impl Read, chunks: u64) -> Result<Option<Vec<u8>>, LogError> {
    let mut len = [0u8; 4];
    let mut got = 0;
    while got < 4 {
        match src.read(&mut len[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err(LogError::Truncated { chunks }),
            Ok(n) => got += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_RECORD {
        return Err(LogError::Corrupt(format!(
            "record of {len} bytes is too large"
        )));
    }
    let mut buf = vec![0u8; len];
    src.read_exact(&mut buf).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            LogError::Truncated { chunks }
        } else {
            LogError::Io(e)
        }
    })?;
    Ok(Some(buf))
}

/// Decrypts a record; returns `(kind, body)`.
pub(crate) fn open_chunk(
    key: &SymmetricKey,
    id: &LogId,
    index: u64,
    record: &[u8],
) -> Result<(u8, Vec<u8>), LogError> {
    let plain = key.open(&aad(id, index), record).map_err(|e| match e {
        vault::VaultError::Authentication | vault::VaultError::Corrupt(_) => {
            LogError::Tampered { chunk: index }
        }
        other => LogError::Crypto(other),
    })?;
    let (&kind, body) = plain
        .split_first()
        .ok_or_else(|| LogError::Corrupt("empty chunk".into()))?;
    Ok((kind, body.to_vec()))
}
