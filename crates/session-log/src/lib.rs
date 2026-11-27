// SPDX-License-Identifier: GPL-3.0-or-later
//! Encrypted terminal session recording in asciicast v2 format.
//!
//! * [`LogWriter`] records output (`o`), resizes (`r`), markers (`m`) and, only if enabled,
//!   input (`i`). Input is never recorded while [`LogWriter::set_sensitive`] is on or while
//!   a password prompt is detected in the output.
//! * Events are buffered, DEFLATE-compressed and sealed with the vault's
//!   [`vault::SymmetricKey`] (XChaCha20-Poly1305) into length-prefixed chunks of a single
//!   file. Each chunk's associated data binds the log id and the chunk index, and a final end
//!   chunk records the chunk count and totals, so bit flips, swapped, reordered, foreign or
//!   missing chunks and truncation are all detected. See [`mod@format`] for the layout.
//! * [`LogReader`] streams and verifies events, searches output with ANSI escapes stripped
//!   ([`LogReader::search`]), exports a plain `.cast` file ([`LogReader::export_cast`]) and
//!   recomputes totals ([`LogReader::verify`]). [`LogReader::open_recovering`] reads logs
//!   whose writer crashed before finishing.
//! * [`LogMetadata`] is what the app stores in the vault about each log;
//!   [`select_for_deletion`] applies a [`RetentionPolicy`].
//!
//! Memory is bounded: chunks are sealed every 64 KiB (or 2 s), and the reader rejects records
//! over 8 MiB and decompressed chunks over 16 MiB.

#![forbid(unsafe_code)]

mod cast;
mod error;
pub mod format;
mod meta;
mod reader;
mod writer;

pub use cast::{CastEvent, CastHeader, EventKind};
pub use error::LogError;
pub use format::{LogSummary, read_log_id};
pub use meta::{LogId, LogMetadata, RetentionPolicy, select_for_deletion};
pub use reader::{LogReader, MAX_MATCHES, Match};
pub use writer::{LogWriter, WriterOptions};

#[cfg(test)]
mod tests;
