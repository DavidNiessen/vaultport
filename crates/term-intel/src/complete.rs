// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic autocomplete.
//!
//! [`Completer::complete`] takes the current command line and cursor and returns ranked
//! [`Suggestion`]s from four sources:
//!
//! 1. **History**: prefix, then fuzzy matches of the current simple command, ranked by
//!    frecency ([`HistoryIndex`]). Accepting one replaces the whole simple command.
//! 2. **Snippets**: by label (prefix / fuzzy) or script content.
//! 3. **Command specs** ([`SpecSet`]): command names, subcommands, options and fixed values
//!    for the word under the cursor, following wrappers such as `sudo`.
//! 4. **Paths**: when a path is expected, the completer asks a [`PathProvider`] for the
//!    directory listing. Remote listing is asynchronous (SFTP), so the provider may answer
//!    "not available yet"; the result then carries [`Completion::needs_paths`], and the app
//!    calls [`Completer::complete_with_entries`] once it has the entries.
//!
//! For other dynamic argument kinds (branches, services, processes, …) the result names the
//! [`Completion::expected`] kind; the app can fetch candidates and merge them with
//! [`Completion::add_values`].
//!
//! Scores are fixed bands (paths 900, values 870, subcommands 860, options 840, commands 800,
//! snippets 300–700, history 350–850), so ordering is stable and explainable. Ties are broken
//! by label.

use crate::history::{HistoryIndex, fuzzy_quality};
use crate::spec::{ArgKind, CommandSpec, OptionSpec, SpecSet};
use crate::tokenize::{CommandContext, command_context};
use core_model::{Snippet, SnippetId};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::ops::Range;

/// Default maximum number of suggestions.
pub const DEFAULT_LIMIT: usize = 20;
/// Maximum number of snippets considered.
const MAX_SNIPPETS: usize = 10_000;
/// Maximum number of directory entries considered per listing.
const MAX_ENTRIES: usize = 20_000;

const SCORE_PATH: f64 = 900.0;
const SCORE_VALUE: f64 = 870.0;
const SCORE_SUBCOMMAND: f64 = 860.0;
const SCORE_OPTION: f64 = 840.0;
const SCORE_COMMAND: f64 = 800.0;

/// Where a suggestion comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SuggestionKind {
    Path,
    Value,
    Subcommand,
    Option,
    Command,
    Snippet,
    History,
}

/// One completion candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// Text that replaces [`Self::replace`] in the input line (already shell-quoted).
    pub text: String,
    /// Byte range of the input line to replace.
    pub replace: Range<usize>,
    /// What to show in the list.
    pub label: String,
    pub description: Option<String>,
    pub kind: SuggestionKind,
    /// Higher is better; see the [module docs](self) for the bands.
    pub score: f64,
    /// For snippet suggestions, the snippet to run (the app may fill in variables).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet_id: Option<SnippetId>,
}

/// An entry of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathEntry {
    /// File name without directory.
    pub name: String,
    pub is_dir: bool,
}

/// A directory listing the completer needs but could not get synchronously.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedsPaths {
    /// Directory as typed (unquoted): `""` is the working directory, otherwise it ends with
    /// `/` and may start with `~` or be relative. The app resolves it on the remote host.
    pub dir: String,
    /// Only directories are useful here (e.g. `cd`).
    pub directories_only: bool,
}

/// Supplies directory listings, e.g. from an SFTP cache.
pub trait PathProvider {
    /// Lists `dir` (see [`NeedsPaths::dir`]). Return `None` if the listing is not available
    /// right now; the completion result then asks for it.
    fn list_dir(&mut self, dir: &str) -> Option<Vec<PathEntry>>;
}

/// A provider that never has listings.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoPaths;

impl PathProvider for NoPaths {
    fn list_dir(&mut self, _dir: &str) -> Option<Vec<PathEntry>> {
        None
    }
}

struct FixedEntries<'a> {
    dir: &'a str,
    entries: &'a [PathEntry],
}

impl PathProvider for FixedEntries<'_> {
    fn list_dir(&mut self, dir: &str) -> Option<Vec<PathEntry>> {
        (dir == self.dir).then(|| self.entries.to_vec())
    }
}

/// The result of a completion request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    /// Ranked suggestions, best first.
    pub suggestions: Vec<Suggestion>,
    /// Byte range of the word being completed.
    pub replace: Range<usize>,
    /// The unquoted part of the word before the cursor.
    pub current: String,
    /// Argument kind expected at the cursor, for app-side dynamic generators.
    pub expected: Option<ArgKind>,
    /// Set when path suggestions are missing because the listing was not available.
    pub needs_paths: Option<NeedsPaths>,
    #[serde(skip)]
    open_quote: Option<char>,
    #[serde(skip)]
    limit: usize,
}

impl Completion {
    /// Ranks app-provided candidates for [`Self::expected`] (e.g. branch names) against the
    /// current word and merges them into [`Self::suggestions`].
    pub fn add_values<S: AsRef<str>>(&mut self, values: impl IntoIterator<Item = S>) {
        let kind_desc = self.expected.map(|k| format!("{k:?}").to_lowercase());
        for v in values.into_iter().take(MAX_ENTRIES) {
            let v = v.as_ref();
            if let Some(bonus) = prefix_bonus(&self.current, v) {
                self.suggestions.push(Suggestion {
                    text: quote_word(self.open_quote, v, false),
                    replace: self.replace.clone(),
                    label: v.to_owned(),
                    description: kind_desc.clone(),
                    kind: SuggestionKind::Value,
                    score: SCORE_VALUE + bonus,
                    snippet_id: None,
                });
            }
        }
        finalize(&mut self.suggestions, self.limit);
    }
}

#[derive(Debug, Clone)]
struct SnippetEntry {
    id: SnippetId,
    label: String,
    label_lower: String,
    script: String,
    script_lower: String,
}

/// The completion engine. See the [module docs](self).
#[derive(Debug, Clone)]
pub struct Completer {
    specs: Cow<'static, SpecSet>,
    history: HistoryIndex,
    snippets: Vec<SnippetEntry>,
    limit: usize,
}

impl Default for Completer {
    fn default() -> Self {
        Self::new()
    }
}

impl Completer {
    /// A completer with the built-in specs and empty history.
    #[must_use]
    pub fn new() -> Self {
        Self {
            specs: Cow::Borrowed(SpecSet::builtin()),
            history: HistoryIndex::new(),
            snippets: Vec::new(),
            limit: DEFAULT_LIMIT,
        }
    }

    /// Replaces the command specs.
    #[must_use]
    pub fn with_specs(mut self, specs: SpecSet) -> Self {
        self.specs = Cow::Owned(specs);
        self
    }

    /// The command specs in use.
    #[must_use]
    pub fn specs(&self) -> &SpecSet {
        &self.specs
    }

    /// The history index.
    #[must_use]
    pub fn history(&self) -> &HistoryIndex {
        &self.history
    }

    /// Mutable access to the history index (to record executed commands).
    pub fn history_mut(&mut self) -> &mut HistoryIndex {
        &mut self.history
    }

    /// Replaces the history index.
    pub fn set_history(&mut self, history: HistoryIndex) {
        self.history = history;
    }

    /// Replaces the snippets offered as suggestions.
    pub fn set_snippets(&mut self, snippets: &[Snippet]) {
        self.snippets = snippets
            .iter()
            .take(MAX_SNIPPETS)
            .map(|s| SnippetEntry {
                id: s.id,
                label: s.label.clone(),
                label_lower: s.label.to_lowercase(),
                script: s.script.clone(),
                script_lower: s.script.to_lowercase(),
            })
            .collect();
    }

    /// Sets the maximum number of suggestions (at least 1).
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit.max(1);
    }

    /// Completes `line` at byte offset `cursor`, at unix time `now` (seconds, for frecency).
    /// Path suggestions are never available synchronously; see [`Completion::needs_paths`].
    #[must_use]
    pub fn complete(&self, line: &str, cursor: usize, now: i64) -> Completion {
        self.complete_with(line, cursor, now, &mut NoPaths)
    }

    /// Like [`Self::complete`], with directory listings from `paths`.
    pub fn complete_with(
        &self,
        line: &str,
        cursor: usize,
        now: i64,
        paths: &mut dyn PathProvider,
    ) -> Completion {
        let ctx = command_context(line, cursor);
        let cursor = ctx.replace.end;
        let mut out = Completion {
            suggestions: Vec::new(),
            replace: ctx.replace.clone(),
            current: ctx.current.clone(),
            expected: None,
            needs_paths: None,
            open_quote: ctx.open_quote,
            limit: self.limit,
        };
        let segment = &line[ctx.command_start.min(cursor)..cursor];
        self.history_suggestions(
            segment,
            ctx.command_start..cursor,
            line.is_empty(),
            now,
            &mut out,
        );
        self.snippet_suggestions(segment, ctx.command_start..cursor, &mut out);
        self.spec_suggestions(line, &ctx, paths, &mut out);
        finalize(&mut out.suggestions, self.limit);
        out
    }

    /// Completes again after the app fetched the listing requested by
    /// [`Completion::needs_paths`].
    #[must_use]
    pub fn complete_with_entries(
        &self,
        line: &str,
        cursor: usize,
        now: i64,
        dir: &str,
        entries: &[PathEntry],
    ) -> Completion {
        self.complete_with(line, cursor, now, &mut FixedEntries { dir, entries })
    }

    fn history_suggestions(
        &self,
        segment: &str,
        replace: Range<usize>,
        empty_line: bool,
        now: i64,
        out: &mut Completion,
    ) {
        if segment.trim().is_empty() && !empty_line {
            return;
        }
        let band = |s: f64, base: f64, width: f64| base + width * s / (s + 2.0);
        let mut push = |cmd: &str, score: f64| {
            out.suggestions.push(Suggestion {
                text: cmd.to_owned(),
                replace: replace.clone(),
                label: cmd.to_owned(),
                description: None,
                kind: SuggestionKind::History,
                score,
                snippet_id: None,
            });
        };
        let prefix = if segment.is_empty() {
            Vec::new()
        } else {
            self.history.prefix_matches(segment, now, self.limit)
        };
        let found = prefix.len();
        for (item, s) in prefix {
            push(&item.command, band(s, 600.0, 250.0));
        }
        if found < self.limit && (segment.len() >= 2 || segment.is_empty()) {
            for (item, s) in self.history.fuzzy_matches(segment, now, self.limit) {
                if item.command != segment && !item.command.starts_with(segment) {
                    push(&item.command, band(s, 350.0, 100.0));
                } else if segment.is_empty() {
                    push(&item.command, band(s, 600.0, 250.0));
                }
            }
        }
    }

    fn snippet_suggestions(&self, segment: &str, replace: Range<usize>, out: &mut Completion) {
        let query = segment.trim();
        if query.is_empty() || self.snippets.is_empty() {
            return;
        }
        let q = query.to_lowercase();
        let q_ascii = query.to_ascii_lowercase();
        for s in &self.snippets {
            let score = if s.label_lower.starts_with(&q) {
                Some(700.0 - (s.label.len() as f64).min(99.0))
            } else if let Some(quality) = fuzzy_quality(q_ascii.as_bytes(), s.label.as_bytes()) {
                Some(450.0 + 100.0 * quality)
            } else if q.len() >= 2 && s.script_lower.contains(&q) {
                Some(300.0)
            } else {
                None
            };
            if let Some(score) = score {
                out.suggestions.push(Suggestion {
                    text: s.script.clone(),
                    replace: replace.clone(),
                    label: s.label.clone(),
                    description: s.script.lines().next().map(str::to_owned),
                    kind: SuggestionKind::Snippet,
                    score,
                    snippet_id: Some(s.id),
                });
            }
        }
    }

    fn spec_suggestions(
        &self,
        line: &str,
        ctx: &CommandContext,
        paths: &mut dyn PathProvider,
        out: &mut Completion,
    ) {
        let cur = ctx.current.as_str();
        if ctx.after_redirect {
            out.expected = Some(ArgKind::Path);
            path_suggestions(ctx, false, paths, out);
            return;
        }
        match resolve(&self.specs, &ctx.words) {
            Position::CommandName => {
                if cur.contains('/') {
                    out.expected = Some(ArgKind::Path);
                    path_suggestions(ctx, false, paths, out);
                } else {
                    out.expected = Some(ArgKind::Command);
                    self.command_names(ctx, out);
                }
            }
            Position::Unknown => {
                out.expected = Some(ArgKind::Path);
                path_suggestions(ctx, false, paths, out);
            }
            Position::InSpec(r) => self.in_spec(line, ctx, &r, paths, out),
        }
    }

    fn command_names(&self, ctx: &CommandContext, out: &mut Completion) {
        let cur = ctx.current.as_str();
        if cur.is_empty() {
            return; // History covers the empty command position.
        }
        for (name, spec) in self.specs.names() {
            if let Some(bonus) = prefix_bonus(cur, name) {
                if name == cur {
                    continue;
                }
                out.suggestions.push(Suggestion {
                    text: quote_word(ctx.open_quote, name, false),
                    replace: ctx.replace.clone(),
                    label: name.to_owned(),
                    description: Some(spec.description.clone()),
                    kind: SuggestionKind::Command,
                    score: SCORE_COMMAND + bonus,
                    snippet_id: None,
                });
            }
        }
    }

    fn in_spec(
        &self,
        line: &str,
        ctx: &CommandContext,
        r: &Resolved<'_>,
        paths: &mut dyn PathProvider,
        out: &mut Completion,
    ) {
        let cur = ctx.current.as_str();
        let Some(&node) = r.chain.last() else { return };

        // Value of an option given as the previous word (`-n <cur>`).
        if let Some(opt) = r.pending_opt {
            if let Some(arg) = &opt.arg {
                out.expected = Some(arg.kind);
                values(ctx, &arg.values, ctx.replace.clone(), out);
                if arg.kind.is_path() {
                    path_suggestions(ctx, arg.kind == ArgKind::Directory, paths, out);
                }
            }
            return;
        }

        // `--opt=<cur>`
        if !r.options_ended
            && cur.starts_with("--")
            && let Some((name, value)) = cur.split_once('=')
        {
            if let Some(opt) = find_option(&r.chain, name)
                && let Some(arg) = &opt.arg
            {
                out.expected = Some(arg.kind);
                let raw = &line[ctx.replace.clone()];
                if raw.starts_with(&format!("{name}=")) {
                    let start = ctx.replace.start + name.len() + 1;
                    let sub_ctx = CommandContext {
                        current: value.to_owned(),
                        replace: start..ctx.replace.end,
                        ..ctx.clone()
                    };
                    values(&sub_ctx, &arg.values, sub_ctx.replace.clone(), out);
                    if arg.kind.is_path() {
                        path_suggestions(&sub_ctx, arg.kind == ArgKind::Directory, paths, out);
                    }
                }
            }
            return;
        }

        if cur.starts_with('-') && !r.options_ended {
            option_suggestions(ctx, r, out);
            return;
        }

        if r.positional == 0 {
            for sub in &node.subcommands {
                let hit = sub
                    .names
                    .iter()
                    .find_map(|n| prefix_bonus(cur, n).map(|b| (n, b)));
                if let Some((name, bonus)) = hit {
                    out.suggestions.push(Suggestion {
                        text: quote_word(ctx.open_quote, name, false),
                        replace: ctx.replace.clone(),
                        label: name.clone(),
                        description: Some(sub.description.clone()),
                        kind: SuggestionKind::Subcommand,
                        score: SCORE_SUBCOMMAND + bonus,
                        snippet_id: None,
                    });
                }
            }
        }
        match node.arg_at(r.positional) {
            Some(arg) => {
                out.expected = Some(arg.kind);
                values(ctx, &arg.values, ctx.replace.clone(), out);
                if arg.kind == ArgKind::Command {
                    self.command_names(ctx, out);
                } else if arg.kind.is_path() {
                    path_suggestions(ctx, arg.kind == ArgKind::Directory, paths, out);
                }
            }
            None if node.subcommands.is_empty() && cur.is_empty() => {
                option_suggestions(ctx, r, out);
            }
            None => {}
        }
    }
}

/// Bonus within a band for a candidate matching `prefix`: exact-case prefix matches beat
/// case-insensitive ones, shorter candidates beat longer ones. `None` if it does not match.
fn prefix_bonus(prefix: &str, candidate: &str) -> Option<f64> {
    let len_penalty = (candidate.chars().count() as f64).min(99.0) / 100.0;
    if candidate.starts_with(prefix) {
        Some(5.0 - len_penalty)
    } else if candidate
        .get(..prefix.len())
        .is_some_and(|p| p.eq_ignore_ascii_case(prefix))
    {
        Some(2.0 - len_penalty)
    } else {
        None
    }
}

fn values(ctx: &CommandContext, vals: &[String], replace: Range<usize>, out: &mut Completion) {
    for v in vals {
        if let Some(bonus) = prefix_bonus(&ctx.current, v) {
            out.suggestions.push(Suggestion {
                text: quote_word(ctx.open_quote, v, false),
                replace: replace.clone(),
                label: v.clone(),
                description: None,
                kind: SuggestionKind::Value,
                score: SCORE_VALUE + bonus,
                snippet_id: None,
            });
        }
    }
}

fn option_suggestions(ctx: &CommandContext, r: &Resolved<'_>, out: &mut Completion) {
    let cur = ctx.current.as_str();
    let long_only = cur.starts_with("--");
    let mut seen: Vec<&str> = Vec::new();
    for (depth, node) in r.chain.iter().rev().enumerate() {
        for opt in &node.options {
            let canonical = opt.names[0].as_str();
            if seen.contains(&canonical) {
                continue;
            }
            seen.push(canonical);
            if !opt.repeat && r.used.contains(&canonical) {
                continue;
            }
            let pick = opt
                .names
                .iter()
                .filter(|n| !long_only || n.starts_with("--"))
                .find_map(|n| prefix_bonus(cur, n).map(|b| (n, b)));
            if let Some((name, bonus)) = pick {
                if name == cur {
                    continue;
                }
                out.suggestions.push(Suggestion {
                    text: name.clone(),
                    replace: ctx.replace.clone(),
                    label: opt.names.join(", "),
                    description: Some(opt.description.clone()),
                    kind: SuggestionKind::Option,
                    // Options of the innermost subcommand first.
                    score: SCORE_OPTION + bonus - depth as f64,
                    snippet_id: None,
                });
            }
        }
    }
}

fn path_suggestions(
    ctx: &CommandContext,
    directories_only: bool,
    paths: &mut dyn PathProvider,
    out: &mut Completion,
) {
    let cur = ctx.current.as_str();
    let (dir, partial) = match cur.rfind('/') {
        Some(i) => (&cur[..=i], &cur[i + 1..]),
        None => ("", cur),
    };
    let Some(entries) = paths.list_dir(dir) else {
        out.needs_paths = Some(NeedsPaths {
            dir: dir.to_owned(),
            directories_only,
        });
        return;
    };
    let show_hidden = partial.starts_with('.');
    for e in entries.iter().take(MAX_ENTRIES) {
        if e.name.is_empty() || e.name == "." || e.name == ".." || e.name.contains('/') {
            continue;
        }
        if directories_only && !e.is_dir {
            continue;
        }
        if e.name.starts_with('.') && !show_hidden {
            continue;
        }
        let Some(bonus) = prefix_bonus(partial, &e.name) else {
            continue;
        };
        let mut value = format!("{dir}{}", e.name);
        if e.is_dir {
            value.push('/');
        }
        out.suggestions.push(Suggestion {
            text: quote_word(ctx.open_quote, &value, e.is_dir),
            replace: ctx.replace.clone(),
            label: if e.is_dir {
                format!("{}/", e.name)
            } else {
                e.name.clone()
            },
            description: None,
            kind: SuggestionKind::Path,
            score: SCORE_PATH + bonus + if e.is_dir { 0.5 } else { 0.0 },
            snippet_id: None,
        });
    }
}

/// Quotes `value` for insertion, continuing an open quote if there is one. Directories keep
/// the quote open so the user can continue typing the path.
fn quote_word(open: Option<char>, value: &str, keep_open: bool) -> String {
    match open {
        Some('\'') => {
            let mut s = format!("'{}", value.replace('\'', "'\\''"));
            if !keep_open {
                s.push('\'');
            }
            s
        }
        Some('"') => {
            let mut s = String::from("\"");
            for c in value.chars() {
                if matches!(c, '"' | '\\' | '$' | '`') {
                    s.push('\\');
                }
                s.push(c);
            }
            if !keep_open {
                s.push('"');
            }
            s
        }
        _ => {
            let mut s = String::with_capacity(value.len());
            for (i, c) in value.chars().enumerate() {
                let tilde_home = i == 0 && c == '~';
                if !tilde_home
                    && (c.is_whitespace()
                        || matches!(
                            c,
                            '\'' | '"'
                                | '\\'
                                | '$'
                                | '`'
                                | '&'
                                | '|'
                                | ';'
                                | '<'
                                | '>'
                                | '('
                                | ')'
                                | '*'
                                | '?'
                                | '['
                                | ']'
                                | '{'
                                | '}'
                                | '!'
                                | '#'
                        ))
                {
                    s.push('\\');
                }
                s.push(c);
            }
            s
        }
    }
}

/// Deduplicates by (text, range), sorts best first and truncates.
fn finalize(v: &mut Vec<Suggestion>, limit: usize) {
    v.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(a.kind.cmp(&b.kind))
            .then_with(|| a.label.cmp(&b.label))
            .then_with(|| a.text.cmp(&b.text))
    });
    let mut seen = std::collections::HashSet::new();
    v.retain(|s| seen.insert((s.text.clone(), s.replace.start, s.replace.end)));
    v.truncate(limit);
}

/// The spec position reached by the complete words before the cursor.
struct Resolved<'s> {
    /// Command, then nested subcommands.
    chain: Vec<&'s CommandSpec>,
    /// Number of positional arguments seen at the innermost node.
    positional: usize,
    /// An option whose value is the word being completed.
    pending_opt: Option<&'s OptionSpec>,
    /// Canonical names of options already given.
    used: Vec<&'s str>,
    options_ended: bool,
}

enum Position<'s> {
    /// Completing the command name itself.
    CommandName,
    /// The command has no spec.
    Unknown,
    InSpec(Resolved<'s>),
}

fn find_option<'s>(chain: &[&'s CommandSpec], word: &str) -> Option<&'s OptionSpec> {
    chain.iter().rev().find_map(|c| c.option(word))
}

fn is_assignment(w: &str) -> bool {
    w.split_once('=').is_some_and(|(n, _)| {
        !n.is_empty()
            && !n.starts_with(|c: char| c.is_ascii_digit())
            && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

fn resolve<'s>(specs: &'s SpecSet, words: &[String]) -> Position<'s> {
    let mut words = words;
    loop {
        while words.first().is_some_and(|w| is_assignment(w)) {
            words = &words[1..];
        }
        let Some(first) = words.first() else {
            return Position::CommandName;
        };
        let name = first.rsplit('/').next().unwrap_or(first);
        let Some(spec) = specs.get(name) else {
            return Position::Unknown;
        };
        let mut r = Resolved {
            chain: vec![spec],
            positional: 0,
            pending_opt: None,
            used: Vec::new(),
            options_ended: false,
        };
        let mut restart = None;
        let mut i = 1;
        while i < words.len() {
            let w = words[i].as_str();
            let node = r.chain[r.chain.len() - 1];
            if !r.options_ended && w == "--" {
                r.options_ended = true;
                i += 1;
                continue;
            }
            if !r.options_ended && w.len() > 1 && w.starts_with('-') {
                let name = w.split('=').next().unwrap_or(w);
                if let Some(opt) = find_option(&r.chain, name) {
                    r.used.push(opt.names[0].as_str());
                    if opt.arg.is_some() && !w.contains('=') {
                        if i + 1 < words.len() {
                            i += 2;
                        } else {
                            r.pending_opt = Some(opt);
                            i += 1;
                        }
                        continue;
                    }
                }
                i += 1;
                continue;
            }
            if r.positional == 0
                && let Some(sub) = node.subcommand(w)
            {
                r.chain.push(sub);
                i += 1;
                continue;
            }
            if node.wrapper
                && node
                    .arg_at(r.positional)
                    .is_some_and(|a| a.kind == ArgKind::Command)
            {
                restart = Some(i);
                break;
            }
            r.positional += 1;
            i += 1;
        }
        match restart {
            Some(i) => words = &words[i..],
            None => return Position::InSpec(r),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    const NOW: i64 = 1_700_000_000;

    fn labels(c: &Completion) -> Vec<&str> {
        c.suggestions.iter().map(|s| s.label.as_str()).collect()
    }

    fn texts(c: &Completion) -> Vec<&str> {
        c.suggestions.iter().map(|s| s.text.as_str()).collect()
    }

    struct Fs;
    impl PathProvider for Fs {
        fn list_dir(&mut self, dir: &str) -> Option<Vec<PathEntry>> {
            let e = |n: &str, d: bool| PathEntry {
                name: n.into(),
                is_dir: d,
            };
            match dir {
                "" => Some(vec![
                    e("src", true),
                    e("README.md", false),
                    e("My Docs", true),
                    e(".git", true),
                    e("scripts", true),
                ]),
                "/etc/" => Some(vec![e("nginx", true), e("hosts", false)]),
                _ => None,
            }
        }
    }

    #[test]
    fn command_names_from_specs() {
        let c = Completer::new().complete("syst", 4, NOW);
        assert_eq!(labels(&c)[0], "systemctl");
        assert_eq!(c.expected, Some(ArgKind::Command));
        assert_eq!(c.replace, 0..4);
    }

    #[test]
    fn subcommands_and_options() {
        let comp = Completer::new();
        let c = comp.complete("git che", 7, NOW);
        assert_eq!(labels(&c), ["checkout", "cherry-pick"]);
        assert_eq!(c.suggestions[0].replace, 4..7);
        assert_eq!(c.suggestions[0].kind, SuggestionKind::Subcommand);

        let c = comp.complete("git commit --am", 15, NOW);
        assert_eq!(texts(&c), ["--amend"]);
        let c = comp.complete("git commit -m x --a", 19, NOW);
        assert!(texts(&c).contains(&"--amend"));
        assert!(texts(&c).contains(&"--all"));
        // Already used non-repeatable options are not offered again.
        let c = comp.complete("git commit --amend --a", 22, NOW);
        assert!(!texts(&c).contains(&"--amend"));
        // Aliased subcommands.
        let c = comp.complete("git remote r", 12, NOW);
        assert!(labels(&c).contains(&"remove"));
        assert!(labels(&c).contains(&"rename"));
    }

    #[test]
    fn option_values_and_enums() {
        let comp = Completer::new();
        let c = comp.complete("journalctl -p e", 15, NOW);
        assert_eq!(labels(&c), ["err", "emerg"]);
        assert_eq!(c.expected, Some(ArgKind::Enum));
        let c = comp.complete("journalctl -u ", 14, NOW);
        assert_eq!(c.expected, Some(ArgKind::Service));
        let c = comp.complete("kubectl get pods -o y", 21, NOW);
        assert_eq!(texts(&c), ["yaml"]);
        let c = comp.complete("docker run --restart=un", 23, NOW);
        assert_eq!(texts(&c), ["unless-stopped"]);
        assert_eq!(c.suggestions[0].replace, 21..23);
    }

    #[test]
    fn dynamic_kinds_are_reported_and_mergeable() {
        let comp = Completer::new();
        let mut c = comp.complete("git checkout ma", 15, NOW);
        assert_eq!(c.expected, Some(ArgKind::Ref));
        c.add_values(["main", "master", "feature/x"]);
        assert_eq!(labels(&c), ["main", "master"]);
        let c = comp.complete("systemctl restart ng", 20, NOW);
        assert_eq!(c.expected, Some(ArgKind::Service));
        let c = comp.complete("kill -9 ", 8, NOW);
        assert_eq!(c.expected, Some(ArgKind::Process));
    }

    #[test]
    fn sudo_and_wrappers_delegate() {
        let comp = Completer::new();
        let c = comp.complete("sudo systemctl st", 17, NOW);
        assert!(labels(&c).contains(&"status"));
        assert!(labels(&c).contains(&"start"));
        let c = comp.complete("sudo -u postgres ps", 19, NOW);
        assert!(labels(&c).contains(&"psql"));
        let c = comp.complete("sudo -u ", 8, NOW);
        assert_eq!(c.expected, Some(ArgKind::User));
        let c = comp.complete("FOO=1 nohup docker ps -", 23, NOW);
        assert!(texts(&c).contains(&"-a"));
        let c = comp.complete("ls -la | grep --ignore", 22, NOW);
        assert_eq!(texts(&c), ["--ignore-case"]);
    }

    #[test]
    fn paths_async_roundtrip() {
        let comp = Completer::new();
        let c = comp.complete("cd sr", 5, NOW);
        assert_eq!(
            c.needs_paths,
            Some(NeedsPaths {
                dir: String::new(),
                directories_only: true
            })
        );
        let entries = [
            PathEntry {
                name: "src".into(),
                is_dir: true,
            },
            PathEntry {
                name: "srv.txt".into(),
                is_dir: false,
            },
        ];
        let c = comp.complete_with_entries("cd sr", 5, NOW, "", &entries);
        assert!(c.needs_paths.is_none());
        assert_eq!(texts(&c), ["src/"]);

        let c = comp.complete("cat /etc/ho", 11, NOW);
        assert_eq!(c.needs_paths.unwrap().dir, "/etc/");
        let c = comp.complete_with("cat /etc/ho", 11, NOW, &mut Fs);
        assert_eq!(texts(&c), ["/etc/hosts"]);
    }

    #[test]
    fn paths_are_quoted_and_hidden_files_filtered() {
        let comp = Completer::new();
        let c = comp.complete_with("ls M", 4, NOW, &mut Fs);
        assert_eq!(texts(&c), ["My\\ Docs/"]);
        let c = comp.complete_with("ls 'M", 5, NOW, &mut Fs);
        assert_eq!(texts(&c), ["'My Docs/"]);
        assert_eq!(c.suggestions[0].replace, 3..5);
        let c = comp.complete_with("ls ", 3, NOW, &mut Fs);
        assert!(!labels(&c).contains(&".git/"));
        let c = comp.complete_with("ls .", 4, NOW, &mut Fs);
        assert_eq!(labels(&c), [".git/"]);
        // Unknown commands and redirections fall back to paths.
        let c = comp.complete_with("./configure > REA", 17, NOW, &mut Fs);
        assert_eq!(texts(&c), ["README.md"]);
        let c = comp.complete_with("frobnicate sc", 13, NOW, &mut Fs);
        assert_eq!(texts(&c), ["scripts/"]);
        assert_eq!(quote_word(None, "~/a b", false), "~/a\\ b");
        assert_eq!(quote_word(Some('"'), "a$b", false), "\"a\\$b\"");
        assert_eq!(quote_word(Some('\''), "it's", false), "'it'\\''s'");
    }

    #[test]
    fn history_and_snippets() {
        let mut comp = Completer::new();
        comp.history_mut()
            .record("git checkout -b feature/login", Some(NOW - 60));
        comp.history_mut().record("git status", Some(NOW - 30));
        comp.history_mut()
            .record("docker compose logs -f api", Some(NOW - 3600 * 48));
        let snippet = Snippet {
            id: SnippetId::new(),
            label: "Restart API".into(),
            script: "docker compose restart api".into(),
            package: None,
            tags: Vec::new(),
            targets: Vec::new(),
            run_count: 0,
            created_at: 0,
            updated_at: 0,
        };
        comp.set_snippets(std::slice::from_ref(&snippet));

        let c = comp.complete("git ch", 6, NOW);
        let hist: Vec<_> = c
            .suggestions
            .iter()
            .filter(|s| s.kind == SuggestionKind::History)
            .collect();
        assert_eq!(hist[0].text, "git checkout -b feature/login");
        assert_eq!(hist[0].replace, 0..6);
        // Spec suggestions outrank history.
        assert_eq!(c.suggestions[0].kind, SuggestionKind::Subcommand);

        let c = comp.complete("cd /srv && git st", 17, NOW);
        let h = c
            .suggestions
            .iter()
            .find(|s| s.kind == SuggestionKind::History)
            .unwrap();
        assert_eq!(h.text, "git status");
        assert_eq!(h.replace, 11..17);

        let c = comp.complete("rest", 4, NOW);
        let s = c
            .suggestions
            .iter()
            .find(|s| s.kind == SuggestionKind::Snippet)
            .unwrap();
        assert_eq!(s.snippet_id, Some(snippet.id));
        assert_eq!(s.text, "docker compose restart api");
        let c = comp.complete("compose restart", 15, NOW);
        assert!(
            c.suggestions
                .iter()
                .any(|s| s.kind == SuggestionKind::Snippet)
        );

        // Fuzzy history.
        let c = comp.complete("dclogs", 6, NOW);
        assert!(
            c.suggestions
                .iter()
                .any(|s| s.text == "docker compose logs -f api")
        );

        // Empty line: most frecent history first.
        let c = comp.complete("", 0, NOW);
        assert_eq!(c.suggestions[0].text, "git status");
    }

    #[test]
    fn results_are_deterministic_and_bounded() {
        let mut comp = Completer::new();
        comp.set_limit(3);
        let a = comp.complete("git ", 4, NOW);
        let b = comp.complete("git ", 4, NOW);
        assert_eq!(a, b);
        assert_eq!(a.suggestions.len(), 3);
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"kind\":\"subcommand\""));
    }

    #[test]
    fn completion_is_fast_with_50k_history() {
        let mut comp = Completer::new();
        for i in 0..50_000i64 {
            comp.history_mut().record(
                &format!("ssh deploy@host-{i} 'systemctl status app-{i}'"),
                Some(NOW - i),
            );
        }
        let mut best = std::time::Duration::MAX;
        for _ in 0..5 {
            let start = std::time::Instant::now();
            let c = comp.complete("ssh deploy@host-4", 17, NOW);
            best = best.min(start.elapsed());
            assert!(!c.suggestions.is_empty());
        }
        // Target is < 2 ms in release builds; generous bound for debug/CI machines.
        let bound = if cfg!(debug_assertions) { 300 } else { 10 };
        assert!(best.as_millis() < bound, "took {best:?}");
    }

    #[test]
    fn never_panics_on_odd_input() {
        let comp = Completer::new();
        for line in [
            "",
            " ",
            "'",
            "\"",
            "\\",
            "git '",
            "a=b",
            "sudo",
            "sudo sudo sudo ",
            "x --=",
            "--=",
            "git commit --message=",
            "é",
            "ls é",
            "$(",
            "a | | b",
            "docker run -p",
        ] {
            for cursor in 0..=line.len() + 2 {
                let _ = comp.complete_with(line, cursor, NOW, &mut Fs);
            }
        }
    }
}
