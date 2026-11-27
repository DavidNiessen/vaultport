// SPDX-License-Identifier: GPL-3.0-or-later
//! Streaming, authenticated log reader with search and export.

use crate::LogError;
use crate::cast::{CastEvent, CastHeader, EventKind};
use crate::format::{
    KIND_DATA, KIND_END, KIND_HEADER, LogSummary, MAX_CHUNK_PLAIN, open_chunk, read_log_id,
    read_record,
};
use crate::meta::LogId;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{Read, Write};
use term_intel::AnsiStripper;
use vault::SymmetricKey;

/// Maximum number of search results.
pub const MAX_MATCHES: usize = 1_000;
/// Maximum query length in characters.
const MAX_QUERY_CHARS: usize = 256;
/// Characters of context before a match in an excerpt.
const EXCERPT_BEFORE: usize = 40;
/// Characters of context after a match in an excerpt.
const EXCERPT_AFTER: usize = 80;

/// A search hit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    /// Seconds since the start of the recording at which the matching line began.
    pub time: f64,
    /// The matching line (escape sequences stripped), shortened around the match.
    pub line_excerpt: String,
}

/// Reads an encrypted log, verifying every chunk. Iterate it to get the events in order;
/// iteration stops after the first error.
#[derive(Debug)]
pub struct LogReader<R: Read> {
    src: R,
    key: SymmetricKey,
    id: LogId,
    next_index: u64,
    header: CastHeader,
    pending: VecDeque<CastEvent>,
    end: Option<LogSummary>,
    stats: LogSummary,
    recover: bool,
    done: bool,
}

impl<R: Read> LogReader<R> {
    /// Opens a log, requiring it to belong to `expected` and to be complete (a missing end
    /// chunk is reported as [`LogError::Truncated`] when reached).
    pub fn open(src: R, key: &SymmetricKey, expected: LogId) -> Result<Self, LogError> {
        Self::open_inner(src, key, expected, false)
    }

    /// Opens a possibly incomplete log (e.g. after a crash): reading stops cleanly where the
    /// authentic data ends. Tampering is still detected. Check [`Self::is_complete`].
    pub fn open_recovering(src: R, key: &SymmetricKey, expected: LogId) -> Result<Self, LogError> {
        Self::open_inner(src, key, expected, true)
    }

    fn open_inner(
        mut src: R,
        key: &SymmetricKey,
        expected: LogId,
        recover: bool,
    ) -> Result<Self, LogError> {
        let id = read_log_id(&mut src)?;
        if id != expected {
            return Err(LogError::WrongLog);
        }
        let rec = read_record(&mut src, 0)?.ok_or(LogError::Truncated { chunks: 0 })?;
        let (kind, body) = open_chunk(key, &id, 0, &rec)?;
        if kind != KIND_HEADER {
            return Err(LogError::Corrupt("first chunk is not a header".into()));
        }
        let header: CastHeader = serde_json::from_slice(&body)?;
        if header.version != 2 {
            return Err(LogError::Corrupt("unsupported asciicast version".into()));
        }
        Ok(Self {
            src,
            key: key.clone(),
            id,
            next_index: 1,
            header,
            pending: VecDeque::new(),
            end: None,
            stats: LogSummary::default(),
            recover,
            done: false,
        })
    }

    /// The asciicast header.
    #[must_use]
    pub fn header(&self) -> &CastHeader {
        &self.header
    }

    /// The log id.
    #[must_use]
    pub fn id(&self) -> LogId {
        self.id
    }

    /// Whether the end chunk has been read (only known after reading all events).
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.end.is_some()
    }

    /// Totals computed from the events read so far.
    #[must_use]
    pub fn stats(&self) -> &LogSummary {
        &self.stats
    }

    /// Returns the next event, `Ok(None)` at the end.
    pub fn next_event(&mut self) -> Result<Option<CastEvent>, LogError> {
        loop {
            if let Some(e) = self.pending.pop_front() {
                self.account(&e);
                return Ok(Some(e));
            }
            if self.done {
                return Ok(None);
            }
            match self.read_chunk() {
                Ok(()) => {}
                Err(e) => {
                    self.done = true;
                    return Err(e);
                }
            }
            if self.done && self.pending.is_empty() {
                self.check_end()?;
            }
        }
    }

    fn account(&mut self, e: &CastEvent) {
        self.stats.events += 1;
        self.stats.duration = self.stats.duration.max(e.time);
        match e.kind {
            EventKind::Output => self.stats.output_bytes += e.data.len() as u64,
            EventKind::Input => self.stats.input_bytes += e.data.len() as u64,
            EventKind::Resize | EventKind::Marker => {}
        }
    }

    fn check_end(&mut self) -> Result<(), LogError> {
        let Some(end) = self.end else { return Ok(()) };
        let s = &self.stats;
        if end.events != s.events
            || end.output_bytes != s.output_bytes
            || end.input_bytes != s.input_bytes
        {
            return Err(LogError::Corrupt("end chunk totals do not match".into()));
        }
        Ok(())
    }

    /// Reads and decrypts one chunk, queueing its events.
    fn read_chunk(&mut self) -> Result<(), LogError> {
        let index = self.next_index;
        let rec = match read_record(&mut self.src, index) {
            Ok(Some(rec)) => rec,
            Ok(None) | Err(LogError::Truncated { .. }) if self.recover => {
                self.done = true;
                return Ok(());
            }
            Ok(None) => return Err(LogError::Truncated { chunks: index }),
            Err(e) => return Err(e),
        };
        let (kind, body) = open_chunk(&self.key, &self.id, index, &rec)?;
        self.next_index += 1;
        self.stats.chunk_count = self.next_index;
        match kind {
            KIND_DATA => {
                let plain =
                    miniz_oxide::inflate::decompress_to_vec_with_limit(&body, MAX_CHUNK_PLAIN)
                        .map_err(|_| LogError::Corrupt("bad compressed chunk".into()))?;
                let text = std::str::from_utf8(&plain)
                    .map_err(|_| LogError::Corrupt("chunk is not UTF-8".into()))?;
                for line in text.split('\n').filter(|l| !l.is_empty()) {
                    self.pending.push_back(CastEvent::from_line(line)?);
                }
            }
            KIND_END => {
                let summary: LogSummary = serde_json::from_slice(&body)?;
                if summary.chunk_count != index {
                    return Err(LogError::Corrupt("end chunk count mismatch".into()));
                }
                if read_record(&mut self.src, index + 1)?.is_some() {
                    return Err(LogError::Corrupt("data after the end chunk".into()));
                }
                self.stats.chunk_count = index;
                self.end = Some(summary);
                self.done = true;
            }
            _ => return Err(LogError::Corrupt(format!("unexpected chunk kind {kind}"))),
        }
        Ok(())
    }

    /// Reads all remaining events, verifying the log, and returns its totals.
    pub fn verify(mut self) -> Result<LogSummary, LogError> {
        while self.next_event()?.is_some() {}
        Ok(self.stats)
    }

    /// Writes the decrypted recording as a plain asciicast v2 `.cast` file.
    pub fn export_cast(mut self, mut out: impl Write) -> Result<LogSummary, LogError> {
        let mut header = serde_json::to_vec(&self.header)?;
        header.push(b'\n');
        out.write_all(&header)?;
        while let Some(e) = self.next_event()? {
            out.write_all(e.to_line()?.as_bytes())?;
            out.write_all(b"\n")?;
        }
        out.flush()?;
        Ok(self.stats)
    }

    /// Case-insensitive search over the output text, with escape sequences stripped. Returns
    /// at most [`MAX_MATCHES`] lines.
    pub fn search(mut self, query: &str) -> Result<Vec<Match>, LogError> {
        let needle: Vec<char> = query
            .trim()
            .chars()
            .take(MAX_QUERY_CHARS)
            .flat_map(char::to_lowercase)
            .collect();
        let mut matches = Vec::new();
        if needle.is_empty() {
            return Ok(matches);
        }
        let mut stripper = AnsiStripper::new();
        let mut line_time = 0.0;
        while let Some(e) = self.next_event()? {
            if e.kind != EventKind::Output {
                continue;
            }
            if stripper.current_line_is_empty() {
                line_time = e.time;
            }
            let mut first = true;
            stripper.feed(e.data.as_bytes(), |line| {
                let t = if first { line_time } else { e.time };
                first = false;
                if matches.len() < MAX_MATCHES
                    && let Some(excerpt) = excerpt(line, &needle)
                {
                    matches.push(Match {
                        time: t,
                        line_excerpt: excerpt,
                    });
                }
            });
            if !first {
                line_time = e.time;
            }
            if matches.len() >= MAX_MATCHES {
                return Ok(matches);
            }
        }
        if let Some(excerpt) = excerpt(&stripper.current_line(), &needle) {
            matches.push(Match {
                time: line_time,
                line_excerpt: excerpt,
            });
        }
        Ok(matches)
    }
}

impl<R: Read> Iterator for LogReader<R> {
    type Item = Result<CastEvent, LogError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_event().transpose()
    }
}

/// Finds `needle` (lowercase chars) in `line` case-insensitively and returns an excerpt.
fn excerpt(line: &str, needle: &[char]) -> Option<String> {
    // Lowercase per char, remembering which original char each lowercase char came from.
    let orig: Vec<char> = line.chars().collect();
    let mut lower: Vec<(usize, char)> = Vec::with_capacity(orig.len());
    for (i, c) in orig.iter().enumerate() {
        lower.extend(c.to_lowercase().map(|l| (i, l)));
    }
    if needle.len() > lower.len() {
        return None;
    }
    let pos = (0..=lower.len() - needle.len()).find(|&s| {
        lower[s..s + needle.len()]
            .iter()
            .map(|p| p.1)
            .eq(needle.iter().copied())
    })?;
    let start_char = lower[pos].0;
    let end_char = lower[pos + needle.len() - 1].0 + 1;
    let from = start_char.saturating_sub(EXCERPT_BEFORE);
    let to = (end_char + EXCERPT_AFTER).min(orig.len());
    let mut s = String::new();
    if from > 0 {
        s.push('…');
    }
    s.extend(&orig[from..to]);
    if to < orig.len() {
        s.push('…');
    }
    Some(s.trim().to_owned())
}
