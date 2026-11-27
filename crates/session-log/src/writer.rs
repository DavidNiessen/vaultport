// SPDX-License-Identifier: GPL-3.0-or-later
//! Streaming, encrypted log writer.

use crate::LogError;
use crate::cast::{CastEvent, CastHeader, EventKind, Utf8Stream, micros_to_secs};
use crate::format::{KIND_DATA, KIND_END, KIND_HEADER, LogSummary, preamble, seal_chunk};
use crate::meta::LogId;
use std::io::Write;
use std::time::{Duration, Instant};
use term_intel::ShellIntegrationParser;
use vault::SymmetricKey;

/// Writer settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterOptions {
    /// Record keyboard input (`i` events). Off by default: input contains everything the user
    /// types, including secrets typed at prompts the heuristics do not recognize.
    pub record_input: bool,
    /// Seal a chunk once this many bytes of event lines are buffered.
    pub flush_bytes: usize,
    /// Seal a chunk when buffered events are older than this.
    pub flush_interval: Duration,
}

impl Default for WriterOptions {
    fn default() -> Self {
        Self {
            record_input: false,
            flush_bytes: 64 * 1024,
            flush_interval: Duration::from_secs(2),
        }
    }
}

/// Output is split into events of at most this many bytes, bounding chunk sizes.
const MAX_EVENT_BYTES: usize = 16 * 1024;

/// Writes an encrypted asciicast v2 recording to `W`.
///
/// Events are buffered and sealed into chunks every [`WriterOptions::flush_bytes`] bytes or
/// [`WriterOptions::flush_interval`]; call [`Self::poll_flush`] from a timer so idle sessions
/// are flushed too, and [`Self::finish`] at the end (a log without its end chunk is reported
/// as truncated, though [`crate::LogReader::open_recovering`] can still read it).
///
/// Input is never recorded while sensitive mode is on ([`Self::set_sensitive`]) or while the
/// built-in detector sees a password prompt in the output.
#[derive(Debug)]
pub struct LogWriter<W: Write> {
    out: Option<W>,
    key: SymmetricKey,
    id: LogId,
    opts: WriterOptions,
    next_index: u64,
    buf: Vec<u8>,
    buf_since: Option<u64>,
    started: Instant,
    last_us: u64,
    sensitive: bool,
    prompt: ShellIntegrationParser,
    out_utf8: Utf8Stream,
    in_utf8: Utf8Stream,
    summary: LogSummary,
}

impl<W: Write> LogWriter<W> {
    /// Starts a log: writes the preamble and the header chunk.
    pub fn create(
        mut out: W,
        key: &SymmetricKey,
        id: LogId,
        header: &CastHeader,
        opts: WriterOptions,
    ) -> Result<Self, LogError> {
        out.write_all(&preamble(&id))?;
        let mut header = header.clone();
        header.version = 2;
        let rec = seal_chunk(key, &id, 0, KIND_HEADER, &serde_json::to_vec(&header)?)?;
        out.write_all(&rec)?;
        out.flush()?;
        Ok(Self {
            out: Some(out),
            key: key.clone(),
            id,
            opts,
            next_index: 1,
            buf: Vec::new(),
            buf_since: None,
            started: Instant::now(),
            last_us: 0,
            sensitive: false,
            prompt: ShellIntegrationParser::new(),
            out_utf8: Utf8Stream::default(),
            in_utf8: Utf8Stream::default(),
            summary: LogSummary::default(),
        })
    }

    /// The log id.
    #[must_use]
    pub fn id(&self) -> LogId {
        self.id
    }

    /// Number of chunks sealed so far (the header chunk included).
    #[must_use]
    pub fn chunks_written(&self) -> u64 {
        self.next_index
    }

    /// Time since the writer was created, for the `*_at` methods.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Enables or disables sensitive mode. While on, input is not recorded (e.g. while a
    /// password prompt is active).
    pub fn set_sensitive(&mut self, sensitive: bool) {
        if sensitive != self.sensitive {
            self.in_utf8.reset();
        }
        self.sensitive = sensitive;
    }

    /// Whether sensitive mode is on.
    #[must_use]
    pub fn is_sensitive(&self) -> bool {
        self.sensitive
    }

    /// Whether input is currently suppressed (sensitive mode or a detected password prompt).
    #[must_use]
    pub fn input_suppressed(&self) -> bool {
        self.sensitive || self.prompt.password_prompt_active()
    }

    /// Turns input recording on or off.
    pub fn set_record_input(&mut self, record: bool) {
        self.opts.record_input = record;
    }

    /// Records terminal output now.
    pub fn output(&mut self, data: &[u8]) -> Result<(), LogError> {
        self.output_at(self.elapsed(), data)
    }

    /// Records terminal output at `t` since the start (clamped to be monotonic).
    pub fn output_at(&mut self, t: Duration, data: &[u8]) -> Result<(), LogError> {
        self.prompt.feed_with(data, |_| {});
        for piece in data.chunks(MAX_EVENT_BYTES) {
            let text = self.out_utf8.decode(piece);
            if !text.is_empty() {
                self.summary.output_bytes += text.len() as u64;
                self.push(t, EventKind::Output, text)?;
            }
        }
        Ok(())
    }

    /// Records user input now (only if enabled and not suppressed).
    pub fn input(&mut self, data: &[u8]) -> Result<(), LogError> {
        self.input_at(self.elapsed(), data)
    }

    /// Records user input at `t` (only if enabled and not suppressed).
    pub fn input_at(&mut self, t: Duration, data: &[u8]) -> Result<(), LogError> {
        if !self.opts.record_input || self.input_suppressed() {
            return Ok(());
        }
        for piece in data.chunks(MAX_EVENT_BYTES) {
            let text = self.in_utf8.decode(piece);
            if !text.is_empty() {
                self.summary.input_bytes += text.len() as u64;
                self.push(t, EventKind::Input, text)?;
            }
        }
        Ok(())
    }

    /// Records a terminal resize now.
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), LogError> {
        self.resize_at(self.elapsed(), cols, rows)
    }

    /// Records a terminal resize at `t`.
    pub fn resize_at(&mut self, t: Duration, cols: u16, rows: u16) -> Result<(), LogError> {
        self.push(t, EventKind::Resize, format!("{cols}x{rows}"))
    }

    /// Records a marker (bookmark within the recording) now.
    pub fn marker(&mut self, label: &str) -> Result<(), LogError> {
        self.marker_at(self.elapsed(), label)
    }

    /// Records a marker at `t`.
    pub fn marker_at(&mut self, t: Duration, label: &str) -> Result<(), LogError> {
        self.push(t, EventKind::Marker, label.to_owned())
    }

    fn push(&mut self, t: Duration, kind: EventKind, data: String) -> Result<(), LogError> {
        if self.out.is_none() {
            return Err(LogError::Finished);
        }
        let us = u64::try_from(t.as_micros())
            .unwrap_or(u64::MAX)
            .max(self.last_us);
        self.last_us = us;
        let line = CastEvent {
            time: micros_to_secs(us),
            kind,
            data,
        }
        .to_line()?;
        self.buf.extend_from_slice(line.as_bytes());
        self.buf.push(b'\n');
        self.summary.events += 1;
        self.summary.duration = micros_to_secs(us);
        let since = *self.buf_since.get_or_insert(us);
        let interval = u64::try_from(self.opts.flush_interval.as_micros()).unwrap_or(u64::MAX);
        if self.buf.len() >= self.opts.flush_bytes || us.saturating_sub(since) >= interval {
            self.flush()?;
        }
        Ok(())
    }

    /// Seals buffered events if they are older than the flush interval. Call periodically.
    pub fn poll_flush(&mut self) -> Result<(), LogError> {
        self.poll_flush_at(self.elapsed())
    }

    /// Like [`Self::poll_flush`] with an explicit time.
    pub fn poll_flush_at(&mut self, t: Duration) -> Result<(), LogError> {
        let now = u64::try_from(t.as_micros()).unwrap_or(u64::MAX);
        let interval = u64::try_from(self.opts.flush_interval.as_micros()).unwrap_or(u64::MAX);
        if self
            .buf_since
            .is_some_and(|since| now.saturating_sub(since) >= interval)
        {
            self.flush()?;
        }
        Ok(())
    }

    /// Seals all buffered events into a chunk now.
    pub fn flush(&mut self) -> Result<(), LogError> {
        let Some(out) = self.out.as_mut() else {
            return Err(LogError::Finished);
        };
        if self.buf.is_empty() {
            return Ok(());
        }
        let compressed = miniz_oxide::deflate::compress_to_vec(&self.buf, 6);
        let rec = seal_chunk(&self.key, &self.id, self.next_index, KIND_DATA, &compressed)?;
        out.write_all(&rec)?;
        out.flush()?;
        self.next_index += 1;
        self.buf.clear();
        self.buf_since = None;
        Ok(())
    }

    /// Flushes remaining events, writes the end chunk and returns the sink and the summary.
    pub fn finish(mut self) -> Result<(W, LogSummary), LogError> {
        let t = Duration::from_micros(self.last_us);
        let tail = self.out_utf8.finish();
        if !tail.is_empty() {
            self.summary.output_bytes += tail.len() as u64;
            self.push(t, EventKind::Output, tail)?;
        }
        self.flush()?;
        self.summary.chunk_count = self.next_index;
        let body = serde_json::to_vec(&self.summary)?;
        let rec = seal_chunk(&self.key, &self.id, self.next_index, KIND_END, &body)?;
        let mut out = self.out.take().ok_or(LogError::Finished)?;
        out.write_all(&rec)?;
        out.flush()?;
        Ok((out, self.summary))
    }
}
