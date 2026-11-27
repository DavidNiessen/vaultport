// SPDX-License-Identifier: GPL-3.0-or-later
//! Shell history import and a frecency-ranked, deduplicated history index.
//!
//! Parsers accept raw file bytes (history files are not guaranteed to be UTF-8) and return
//! entries oldest first:
//!
//! * [`parse_bash`]: `~/.bash_history`, with optional `#<unix time>` lines
//!   (`HISTTIMEFORMAT`). When timestamps are present, all lines up to the next timestamp form
//!   one (multi-line) entry, as bash itself reads them.
//! * [`parse_zsh`]: `~/.zsh_history`, plain or `EXTENDED_HISTORY` (`: 1700000000:0;cmd`),
//!   with `\` line continuations and zsh's metafied bytes (`0x83` escapes).
//! * [`parse_fish`]: `~/.local/share/fish/fish_history` (`- cmd: …` / `  when: …`).
//!
//! Memory is bounded: at most [`MAX_IMPORT_ENTRIES`] (most recent) entries are returned and
//! commands longer than [`MAX_COMMAND_LEN`] bytes are skipped.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Longest command (in bytes) kept from history.
pub const MAX_COMMAND_LEN: usize = 8 * 1024;
/// Maximum number of entries returned by the parsers (the most recent ones are kept).
pub const MAX_IMPORT_ENTRIES: usize = 200_000;
/// Default capacity of a [`HistoryIndex`].
pub const DEFAULT_CAPACITY: usize = 100_000;

/// One command read from a history file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub command: String,
    /// Unix time in seconds, when the file records it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
}

/// Supported history file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryFormat {
    Bash,
    Zsh,
    Fish,
}

impl HistoryFormat {
    /// Conventional history file location relative to the home directory.
    #[must_use]
    pub const fn default_path(self) -> &'static str {
        match self {
            Self::Bash => ".bash_history",
            Self::Zsh => ".zsh_history",
            Self::Fish => ".local/share/fish/fish_history",
        }
    }
}

/// Parses `data` in the given format.
#[must_use]
pub fn parse(format: HistoryFormat, data: &[u8]) -> Vec<HistoryEntry> {
    match format {
        HistoryFormat::Bash => parse_bash(data),
        HistoryFormat::Zsh => parse_zsh(data),
        HistoryFormat::Fish => parse_fish(data),
    }
}

fn lines(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    data.split(|&b| b == b'\n')
        .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
}

fn bash_timestamp(line: &[u8]) -> Option<i64> {
    let digits = line.strip_prefix(b"#")?;
    if digits.is_empty() || digits.len() > 18 || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(digits).ok()?.parse().ok()
}

/// Collects entries, enforcing the length and count limits.
#[derive(Default)]
struct Sink {
    out: Vec<HistoryEntry>,
}

impl Sink {
    fn push(&mut self, command: &[u8], timestamp: Option<i64>) {
        if command.len() > MAX_COMMAND_LEN {
            return;
        }
        let command = String::from_utf8_lossy(command);
        let command = command.trim_matches(|c: char| c == '\n' || c == '\r');
        if command.trim().is_empty() {
            return;
        }
        // Drop the oldest half when full, so the work stays amortized O(1).
        if self.out.len() >= MAX_IMPORT_ENTRIES * 2 {
            self.out.drain(..MAX_IMPORT_ENTRIES);
        }
        self.out.push(HistoryEntry {
            command: command.to_owned(),
            timestamp,
        });
    }

    fn finish(mut self) -> Vec<HistoryEntry> {
        if self.out.len() > MAX_IMPORT_ENTRIES {
            let excess = self.out.len() - MAX_IMPORT_ENTRIES;
            self.out.drain(..excess);
        }
        self.out
    }
}

/// Parses a bash history file.
#[must_use]
pub fn parse_bash(data: &[u8]) -> Vec<HistoryEntry> {
    let mut sink = Sink::default();
    let has_timestamps = lines(data).any(|l| bash_timestamp(l).is_some());
    if !has_timestamps {
        for l in lines(data) {
            sink.push(l, None);
        }
        return sink.finish();
    }
    let mut current: Option<(i64, Vec<u8>)> = None;
    for l in lines(data) {
        if let Some(ts) = bash_timestamp(l) {
            if let Some((t, cmd)) = current.take() {
                sink.push(&cmd, Some(t));
            }
            current = Some((ts, Vec::new()));
        } else if let Some((_, cmd)) = current.as_mut() {
            if cmd.len() <= MAX_COMMAND_LEN {
                if !cmd.is_empty() {
                    cmd.push(b'\n');
                }
                cmd.extend_from_slice(l);
            }
        } else {
            sink.push(l, None);
        }
    }
    if let Some((t, cmd)) = current {
        sink.push(&cmd, Some(t));
    }
    sink.finish()
}

/// Reverses zsh's metafication: `0x83, b` encodes the byte `b ^ 0x20`.
fn unmetafy(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut it = data.iter();
    while let Some(&b) = it.next() {
        if b == 0x83 {
            if let Some(&n) = it.next() {
                out.push(n ^ 0x20);
            }
        } else {
            out.push(b);
        }
    }
    out
}

/// Splits `: <start>:<elapsed>;<command>` into its parts.
fn zsh_extended(line: &[u8]) -> Option<(i64, &[u8])> {
    let rest = line.strip_prefix(b": ")?;
    let colon = rest.iter().position(|&b| b == b':')?;
    let semi = rest.iter().position(|&b| b == b';')?;
    if colon > semi {
        return None;
    }
    let start = &rest[..colon];
    let elapsed = &rest[colon + 1..semi];
    if start.is_empty()
        || start.len() > 18
        || !start.iter().all(u8::is_ascii_digit)
        || !elapsed.iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    let ts = std::str::from_utf8(start).ok()?.parse().ok()?;
    Some((ts, &rest[semi + 1..]))
}

/// Parses a zsh history file.
#[must_use]
pub fn parse_zsh(data: &[u8]) -> Vec<HistoryEntry> {
    let data = unmetafy(data);
    let mut sink = Sink::default();
    let mut current: Option<(Option<i64>, Vec<u8>)> = None;
    for l in lines(&data) {
        if let Some((_, cmd)) = current.as_mut() {
            // Previous line ended with a backslash: this line continues the command.
            if cmd.len() <= MAX_COMMAND_LEN {
                cmd.push(b'\n');
                cmd.extend_from_slice(l);
            }
        } else {
            let (ts, body) = match zsh_extended(l) {
                Some((ts, body)) => (Some(ts), body),
                None => (None, l),
            };
            current = Some((ts, body.to_vec()));
        }
        let continues = current
            .as_ref()
            .is_some_and(|(_, cmd)| cmd.last() == Some(&b'\\'));
        if continues {
            if let Some((_, cmd)) = current.as_mut() {
                cmd.pop();
            }
        } else if let Some((ts, cmd)) = current.take() {
            sink.push(&cmd, ts);
        }
    }
    if let Some((ts, cmd)) = current {
        sink.push(&cmd, ts);
    }
    sink.finish()
}

/// Undoes fish's history escaping (`\\` and `\n`).
fn fish_unescape(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'\\' {
            match s.get(i + 1) {
                Some(b'\\') => {
                    out.push(b'\\');
                    i += 2;
                    continue;
                }
                Some(b'n') => {
                    out.push(b'\n');
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        out.push(s[i]);
        i += 1;
    }
    out
}

/// Parses a fish history file.
#[must_use]
pub fn parse_fish(data: &[u8]) -> Vec<HistoryEntry> {
    let mut sink = Sink::default();
    let mut current: Option<(Vec<u8>, Option<i64>)> = None;
    for l in lines(data) {
        if let Some(cmd) = l.strip_prefix(b"- cmd:") {
            if let Some((c, t)) = current.take() {
                sink.push(&c, t);
            }
            let cmd = cmd.strip_prefix(b" ").unwrap_or(cmd);
            current = Some((fish_unescape(cmd), None));
        } else if let Some(when) = l.trim_ascii_start().strip_prefix(b"when:")
            && let Some((_, t)) = current.as_mut()
        {
            *t = std::str::from_utf8(when)
                .ok()
                .and_then(|s| s.trim().parse().ok());
        }
    }
    if let Some((c, t)) = current {
        sink.push(&c, t);
    }
    sink.finish()
}

/// A deduplicated command with usage statistics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub command: String,
    /// Number of times the command was used.
    pub count: u32,
    /// Most recent use (unix seconds), or 0 when unknown.
    #[serde(default)]
    pub last_used: i64,
    /// Insertion order of the most recent use, for tie-breaking undated entries.
    #[serde(default)]
    pub seq: u64,
}

/// A deduplicated, frecency-ranked command history with bounded size.
///
/// Serializes as `{ "capacity": n, "items": [...] }`.
#[derive(Debug, Clone)]
pub struct HistoryIndex {
    items: Vec<HistoryItem>,
    by_command: HashMap<String, usize>,
    next_seq: u64,
    capacity: usize,
}

impl Default for HistoryIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize)]
struct ReprRef<'a> {
    capacity: usize,
    items: &'a [HistoryItem],
}

#[derive(Deserialize)]
struct Repr {
    #[serde(default = "default_capacity")]
    capacity: usize,
    #[serde(default)]
    items: Vec<HistoryItem>,
}

fn default_capacity() -> usize {
    DEFAULT_CAPACITY
}

impl Serialize for HistoryIndex {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        ReprRef {
            capacity: self.capacity,
            items: &self.items,
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for HistoryIndex {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let repr = Repr::deserialize(d)?;
        let mut idx = Self::with_capacity(repr.capacity);
        let mut items = repr.items;
        items.sort_by_key(|i| i.seq);
        for it in items {
            idx.merge_item(it);
        }
        idx.enforce_capacity();
        Ok(idx)
    }
}

/// Recency weight of a use `age` seconds ago (unknown age gets the lowest weight).
fn recency_weight(last_used: i64, now: i64) -> f64 {
    if last_used <= 0 {
        return 0.25;
    }
    let age = now.saturating_sub(last_used).max(0);
    match age {
        0..3_600 => 4.0,
        3_600..86_400 => 2.0,
        86_400..604_800 => 1.0,
        604_800..2_592_000 => 0.5,
        _ => 0.25,
    }
}

impl HistoryIndex {
    /// An empty index with [`DEFAULT_CAPACITY`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// An empty index holding at most `capacity` distinct commands (minimum 16).
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: Vec::new(),
            by_command: HashMap::new(),
            next_seq: 1,
            capacity: capacity.max(16),
        }
    }

    /// Number of distinct commands.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the index is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// All items, in no particular order.
    #[must_use]
    pub fn items(&self) -> &[HistoryItem] {
        &self.items
    }

    /// Looks up a command.
    #[must_use]
    pub fn get(&self, command: &str) -> Option<&HistoryItem> {
        self.by_command.get(command).map(|&i| &self.items[i])
    }

    /// Records one use of `command` at `timestamp` (unix seconds).
    ///
    /// Commands starting with a space are ignored (the `HISTCONTROL=ignorespace` convention
    /// for keeping a command out of history), as are empty and over-long commands.
    pub fn record(&mut self, command: &str, timestamp: Option<i64>) {
        if command.starts_with(' ') {
            return;
        }
        let command = command.trim_end();
        if command.is_empty() || command.len() > MAX_COMMAND_LEN {
            return;
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        self.merge_item(HistoryItem {
            command: command.to_owned(),
            count: 1,
            last_used: timestamp.unwrap_or(0),
            seq,
        });
        if self.items.len() > self.capacity {
            self.enforce_capacity();
        }
    }

    /// Records all entries (oldest first), e.g. from [`parse`].
    pub fn import(&mut self, entries: &[HistoryEntry]) {
        for e in entries {
            self.record(&e.command, e.timestamp);
        }
    }

    /// Removes a command (e.g. when the user deletes a suggestion).
    pub fn remove(&mut self, command: &str) -> bool {
        let Some(i) = self.by_command.remove(command) else {
            return false;
        };
        self.items.swap_remove(i);
        if let Some(moved) = self.items.get(i) {
            self.by_command.insert(moved.command.clone(), i);
        }
        true
    }

    fn merge_item(&mut self, item: HistoryItem) {
        self.next_seq = self.next_seq.max(item.seq.saturating_add(1));
        if let Some(&i) = self.by_command.get(&item.command) {
            let existing = &mut self.items[i];
            existing.count = existing.count.saturating_add(item.count);
            existing.last_used = existing.last_used.max(item.last_used);
            existing.seq = existing.seq.max(item.seq);
        } else {
            self.by_command
                .insert(item.command.clone(), self.items.len());
            self.items.push(item);
        }
    }

    fn latest_timestamp(&self) -> i64 {
        self.items.iter().map(|i| i.last_used).max().unwrap_or(0)
    }

    /// Evicts the lowest-ranked items down to 90 % of capacity.
    fn enforce_capacity(&mut self) {
        if self.items.len() <= self.capacity {
            return;
        }
        let now = self.latest_timestamp();
        let keep = self.capacity - self.capacity / 10;
        let mut items = std::mem::take(&mut self.items);
        items.sort_by(|a, b| {
            self.score(b, now)
                .total_cmp(&self.score(a, now))
                .then(b.seq.cmp(&a.seq))
        });
        items.truncate(keep);
        self.by_command = items
            .iter()
            .enumerate()
            .map(|(i, it)| (it.command.clone(), i))
            .collect();
        self.items = items;
    }

    /// Frecency score of `item` at time `now` (unix seconds). Higher is better.
    #[must_use]
    pub fn score(&self, item: &HistoryItem, now: i64) -> f64 {
        let frequency = 1.0 + approx_ln(item.count);
        let order = item.seq as f64 / self.next_seq.max(1) as f64;
        frequency * recency_weight(item.last_used, now) + 0.1 * order
    }

    /// Commands starting with `prefix` (case-sensitive, excluding `prefix` itself), best
    /// first, with their scores.
    #[must_use]
    pub fn prefix_matches(&self, prefix: &str, now: i64, limit: usize) -> Vec<(&HistoryItem, f64)> {
        let hits = self
            .items
            .iter()
            .filter(|it| it.command.len() > prefix.len() && it.command.starts_with(prefix))
            .map(|it| (it, self.score(it, now)));
        top_k(hits, limit)
    }

    /// Commands containing the characters of `query` in order (ASCII case-insensitive),
    /// ranked by frecency times match quality (contiguous, early matches score higher).
    #[must_use]
    pub fn fuzzy_matches(&self, query: &str, now: i64, limit: usize) -> Vec<(&HistoryItem, f64)> {
        let q = query.trim();
        if q.is_empty() {
            return top_k(self.items.iter().map(|it| (it, self.score(it, now))), limit);
        }
        let q = q.to_ascii_lowercase();
        let hits = self.items.iter().filter_map(|it| {
            fuzzy_quality(q.as_bytes(), it.command.as_bytes())
                .map(|quality| (it, self.score(it, now) * quality))
        });
        top_k(hits, limit)
    }
}

/// Cheap, monotonic piecewise-linear approximation of `ln(n)` (0 for `n <= 1`).
fn approx_ln(n: u32) -> f64 {
    if n <= 1 {
        return 0.0;
    }
    let l = n.ilog2();
    let base = 1u32 << l;
    let frac = f64::from(n - base) / f64::from(base);
    (f64::from(l) + frac) * std::f64::consts::LN_2
}

/// Greedy subsequence match quality in `(0, 1]`, or `None` when `needle` is not a
/// subsequence of `hay`. `needle` must already be ASCII-lowercased.
pub(crate) fn fuzzy_quality(needle: &[u8], hay: &[u8]) -> Option<f64> {
    let (&first_c, rest) = needle.split_first()?;
    if needle.len() > hay.len() {
        return None;
    }
    let find = |from: usize, c: u8| -> Option<usize> {
        let tail = hay.get(from..)?;
        let pos = if c.is_ascii_lowercase() {
            tail.iter().position(|&h| h | 0x20 == c)
        } else {
            tail.iter().position(|&h| h == c)
        };
        pos.map(|p| p + from)
    };
    let first = find(0, first_c)?;
    let mut last = first;
    for &c in rest {
        last = find(last + 1, c)?;
    }
    let span = (last - first + 1) as f64;
    let compactness = needle.len() as f64 / span;
    let start_bonus = if first == 0 { 1.0 } else { 0.7 };
    Some(compactness * start_bonus)
}

/// Selects the `limit` best `(item, score)` pairs, sorted best first; ties are broken by
/// the more recent sequence number, then by command text, so results are deterministic.
fn top_k<'a>(
    hits: impl Iterator<Item = (&'a HistoryItem, f64)>,
    limit: usize,
) -> Vec<(&'a HistoryItem, f64)> {
    if limit == 0 {
        return Vec::new();
    }
    let cmp = |a: &(&HistoryItem, f64), b: &(&HistoryItem, f64)| {
        b.1.total_cmp(&a.1)
            .then(b.0.seq.cmp(&a.0.seq))
            .then_with(|| a.0.command.cmp(&b.0.command))
    };
    let mut v: Vec<_> = hits.collect();
    if v.len() > limit {
        v.select_nth_unstable_by(limit - 1, cmp);
        v.truncate(limit);
    }
    v.sort_by(cmp);
    v
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use proptest::prelude::*;

    fn cmds(v: &[HistoryEntry]) -> Vec<&str> {
        v.iter().map(|e| e.command.as_str()).collect()
    }

    #[test]
    fn bash_plain_and_timestamped() {
        let v = parse_bash(b"ls -la\n\ncd /tmp\r\n#not a ts\n");
        assert_eq!(cmds(&v), ["ls -la", "cd /tmp", "#not a ts"]);
        assert!(v.iter().all(|e| e.timestamp.is_none()));

        let v = parse_bash(b"#1700000000\nls\n#1700000010\nfor i in 1 2; do\necho $i\ndone\n");
        assert_eq!(cmds(&v), ["ls", "for i in 1 2; do\necho $i\ndone"]);
        assert_eq!(v[0].timestamp, Some(1_700_000_000));
        assert_eq!(v[1].timestamp, Some(1_700_000_010));
    }

    #[test]
    fn zsh_extended_continuations_and_metafied() {
        let mut data = b": 1700000000:0;git status\n: 1700000005:3;echo a\\\nb\n".to_vec();
        // "echo →" (E2 86 92): zsh metafies 0x86 and 0x92 as 0x83 followed by byte ^ 0x20.
        data.extend_from_slice(b": 1700000009:0;echo \xe2\x83\xa6\x83\xb2\n");
        data.extend_from_slice(b"plain entry\n");
        let v = parse_zsh(&data);
        assert_eq!(
            cmds(&v),
            ["git status", "echo a\nb", "echo →", "plain entry"]
        );
        assert_eq!(v[0].timestamp, Some(1_700_000_000));
        assert_eq!(v[2].timestamp, Some(1_700_000_009));
        assert_eq!(v[3].timestamp, None);
    }

    #[test]
    fn fish_history() {
        let data = b"- cmd: ls -la\n  when: 1700000000\n- cmd: echo a\\\\b\\nsecond\n  when: 1700000100\n  paths:\n    - /tmp\n- cmd: cd /srv\n";
        let v = parse_fish(data);
        assert_eq!(cmds(&v), ["ls -la", "echo a\\b\nsecond", "cd /srv"]);
        assert_eq!(v[0].timestamp, Some(1_700_000_000));
        assert_eq!(v[1].timestamp, Some(1_700_000_100));
        assert_eq!(v[2].timestamp, None);
    }

    #[test]
    fn index_dedups_and_ranks() {
        let mut idx = HistoryIndex::new();
        let now = 1_700_100_000;
        idx.record("git status", Some(now - 30 * 86_400));
        for _ in 0..5 {
            idx.record("git status", Some(now - 40 * 86_400));
        }
        idx.record("git stash", Some(now - 60));
        idx.record("git log", Some(now - 10 * 86_400));
        idx.record(" secret --token x", Some(now));
        assert_eq!(idx.len(), 3);
        assert_eq!(idx.get("git status").unwrap().count, 6);
        let m: Vec<_> = idx.prefix_matches("git s", now, 10);
        assert_eq!(m.len(), 2);
        // Very recent beats frequent-but-old.
        assert_eq!(m[0].0.command, "git stash");
        let f = idx.fuzzy_matches("gst", now, 10);
        assert!(f.iter().any(|(i, _)| i.command == "git status"));
        assert!(idx.remove("git log"));
        assert!(!idx.remove("git log"));
        assert_eq!(idx.get("git stash").unwrap().command, "git stash");
    }

    #[test]
    fn index_serde_roundtrip_and_capacity() {
        let mut idx = HistoryIndex::with_capacity(20);
        for i in 0..100 {
            idx.record(&format!("cmd {i}"), Some(1_700_000_000 + i));
        }
        assert!(idx.len() <= 20);
        assert!(idx.get("cmd 99").is_some(), "most recent survives eviction");
        let json = serde_json::to_string(&idx).unwrap();
        let back: HistoryIndex = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), idx.len());
        assert_eq!(back.get("cmd 99"), idx.get("cmd 99"));
        let empty: HistoryIndex = serde_json::from_str("{}").unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn prefix_search_is_fast_on_50k_entries() {
        let mut idx = HistoryIndex::new();
        for i in 0..50_000i64 {
            idx.record(
                &format!("git commit -m 'change number {i}'"),
                Some(1_700_000_000 + i),
            );
        }
        let now = 1_700_100_000;
        // Best of a few runs, to ignore scheduler noise and cold caches.
        let mut best = std::time::Duration::MAX;
        let (mut m, mut f) = (Vec::new(), Vec::new());
        for _ in 0..5 {
            let start = std::time::Instant::now();
            m = idx.prefix_matches("git commit -m 'change number 4", now, 10);
            f = idx.fuzzy_matches("gcmchange", now, 10);
            best = best.min(start.elapsed());
        }
        let took = best;
        assert_eq!(m.len(), 10);
        assert_eq!(f.len(), 10);
        // Target is < 2 ms in release builds; allow generous slack for debug/CI machines.
        let bound = if cfg!(debug_assertions) { 250 } else { 10 };
        assert!(took.as_millis() < bound, "took {took:?}");
    }

    proptest! {
        #[test]
        fn parsers_never_panic(data in proptest::collection::vec(any::<u8>(), 0..512)) {
            let _ = parse_bash(&data);
            let _ = parse_zsh(&data);
            let _ = parse_fish(&data);
        }
    }
}
