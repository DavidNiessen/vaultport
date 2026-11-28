// SPDX-License-Identifier: GPL-3.0-or-later
//! Command blocks: the terminal output stream split into commands and their output.
//!
//! [`BlockRecorder`] follows the shell integration marks (`OSC 133` / `633`, see
//! [`crate::shell`]) in terminal *output* and keeps the most recent commands with their plain
//! text output, exit status and working directory. The app uses it to give AI agents (and the
//! local AI) context such as "the last command failed with this output" without them ever
//! seeing the raw stream. Without shell integration it still keeps the last lines of output.
//!
//! Memory is bounded: at most [`MAX_BLOCKS`] blocks of at most [`MAX_BLOCK_LINES`] lines of
//! at most 1 KiB (the middle of longer outputs is dropped, long lines are wrapped) and
//! [`MAX_TAIL_LINES`] lines of plain tail — about 32 MiB per session in the worst case.

use crate::shell::{ShellEvent, parse_osc};
use crate::vt::{LineBuf, VtEvent, VtScanner};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Finished blocks kept.
pub const MAX_BLOCKS: usize = 30;
/// Output lines kept per block (head and tail).
pub const MAX_BLOCK_LINES: usize = 1000;
/// Plain output lines kept regardless of blocks.
pub const MAX_TAIL_LINES: usize = 1000;
const MAX_LINE_BYTES: usize = 1024;
/// Echoed input kept between the end of the prompt and the start of the command.
const MAX_INPUT_BYTES: usize = 4096;

/// One command and its output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    /// Sequence number within the session, starting at 1.
    pub id: u64,
    /// The command line, as the shell reported it (or as it was echoed).
    pub command: String,
    /// Plain text output, lines separated by `\n`.
    pub output: String,
    /// Output lines left out in the middle because the output was too long.
    pub omitted_lines: usize,
    /// Exit status, once the command finished and the shell reported it.
    pub exit: Option<i32>,
    /// Working directory the command ran in.
    pub cwd: Option<String>,
    /// Unix time in milliseconds.
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, Default)]
struct Lines {
    head: Vec<String>,
    tail: VecDeque<String>,
    omitted: usize,
}

impl Lines {
    fn push(&mut self, line: String) {
        let half = MAX_BLOCK_LINES / 2;
        if self.head.len() < half {
            self.head.push(line);
            return;
        }
        self.tail.push_back(line);
        if self.tail.len() > half {
            self.tail.pop_front();
            self.omitted += 1;
        }
    }

    fn text(&self, current: Option<&str>) -> String {
        let mut parts: Vec<&str> = self.head.iter().map(String::as_str).collect();
        let marker;
        if self.omitted > 0 {
            marker = format!("… {} lines omitted …", self.omitted);
            parts.push(&marker);
        }
        parts.extend(self.tail.iter().map(String::as_str));
        if let Some(c) = current.filter(|c| !c.is_empty()) {
            parts.push(c);
        }
        parts.join("\n")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Before any mark, or after `D`.
    Idle,
    /// Between `A` and `B`: the prompt is drawn.
    Prompt,
    /// Between `B` and `C`: the user types (echoed input).
    Input,
    /// Between `C` and `D`: the command runs.
    Running,
}

/// Splits terminal output into [`Block`]s. See the [module docs](self).
#[derive(Debug, Clone)]
pub struct BlockRecorder {
    scanner: VtScanner,
    line: LineBuf,
    phase: Phase,
    pending_command: Option<String>,
    input: LineBuf,
    cwd: Option<String>,
    next_id: u64,
    running: Option<(Block, Lines)>,
    blocks: VecDeque<Block>,
    tail: VecDeque<String>,
    integration: bool,
}

impl Default for BlockRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockRecorder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            scanner: VtScanner::new(),
            line: LineBuf::new(MAX_LINE_BYTES),
            phase: Phase::Idle,
            pending_command: None,
            input: LineBuf::new(MAX_INPUT_BYTES),
            cwd: None,
            next_id: 1,
            running: None,
            blocks: VecDeque::new(),
            tail: VecDeque::new(),
            integration: false,
        }
    }

    /// Feeds terminal output received at `now` (unix milliseconds).
    pub fn feed(&mut self, data: &[u8], now: i64) {
        let mut scanner = std::mem::take(&mut self.scanner);
        scanner.feed(data, |ev| match ev {
            VtEvent::Print(t) => {
                if self.phase == Phase::Input {
                    self.input.print(t, &mut |_| {});
                }
                let mut done = Vec::new();
                self.line.print(t, &mut |l| done.push(l.to_owned()));
                for l in &done {
                    self.complete_line(l);
                }
            }
            VtEvent::Control(c) => {
                if self.phase == Phase::Input && c != b'\n' && c != b'\r' {
                    self.input.control(c, &mut |_| {});
                }
                let mut done = Vec::new();
                self.line.control(c, &mut |l| done.push(l.to_owned()));
                for l in &done {
                    self.complete_line(l);
                }
            }
            VtEvent::Osc(payload) => {
                if let Some(e) = parse_osc(payload, &mut self.pending_command) {
                    self.event(e, now);
                }
            }
        });
        self.scanner = scanner;
    }

    fn complete_line(&mut self, line: &str) {
        let line = line.trim_end().to_owned();
        if let Some((_, lines)) = &mut self.running {
            lines.push(line.clone());
        }
        self.tail.push_back(line);
        if self.tail.len() > MAX_TAIL_LINES {
            self.tail.pop_front();
        }
    }

    fn event(&mut self, e: ShellEvent, now: i64) {
        match e {
            ShellEvent::Cwd(c) => self.cwd = Some(c),
            ShellEvent::PromptStart => {
                self.integration = true;
                // A command that ended without `D` (e.g. bash without the D mark).
                self.finish(None, now);
                self.line.clear();
                self.pending_command = None;
                self.phase = Phase::Prompt;
            }
            ShellEvent::CommandStart => {
                self.integration = true;
                self.input.clear();
                self.phase = Phase::Input;
            }
            ShellEvent::CommandExecuted { command } => {
                self.integration = true;
                self.finish(None, now);
                let typed = self.input.current().trim().to_owned();
                self.input.clear();
                let command = command.filter(|c| !c.trim().is_empty()).unwrap_or(typed);
                // The echoed command line is not output.
                self.line.clear();
                let block = Block {
                    id: self.next_id,
                    command,
                    output: String::new(),
                    omitted_lines: 0,
                    exit: None,
                    cwd: self.cwd.clone(),
                    started_at: now,
                    finished_at: None,
                };
                self.next_id += 1;
                self.running = Some((block, Lines::default()));
                self.phase = Phase::Running;
            }
            ShellEvent::CommandFinished { exit } => {
                self.integration = true;
                self.finish(exit, now);
                self.phase = Phase::Idle;
            }
            _ => {}
        }
    }

    fn finish(&mut self, exit: Option<i32>, now: i64) {
        let Some((mut block, mut lines)) = self.running.take() else {
            return;
        };
        let rest = self.line.current();
        if !rest.trim().is_empty() {
            lines.push(rest.trim_end().to_owned());
        }
        self.line.clear();
        block.output = lines.text(None);
        block.omitted_lines = lines.omitted;
        block.exit = exit;
        block.finished_at = Some(now);
        if block.command.is_empty() && block.output.is_empty() {
            return;
        }
        self.blocks.push_back(block);
        if self.blocks.len() > MAX_BLOCKS {
            self.blocks.pop_front();
        }
    }

    /// Finished blocks, oldest first.
    #[must_use]
    pub fn blocks(&self) -> &VecDeque<Block> {
        &self.blocks
    }

    /// The command that is running now, with its output so far.
    #[must_use]
    pub fn running(&self) -> Option<Block> {
        self.running.as_ref().map(|(b, lines)| {
            let current = self.line.current();
            Block {
                output: lines.text(Some(current.trim_end())),
                omitted_lines: lines.omitted,
                ..b.clone()
            }
        })
    }

    /// The last `n` finished blocks plus the running one, oldest first.
    #[must_use]
    pub fn recent(&self, n: usize) -> Vec<Block> {
        let mut out: Vec<Block> = self
            .blocks
            .iter()
            .skip(self.blocks.len().saturating_sub(n))
            .cloned()
            .collect();
        if let Some(r) = self.running() {
            out.push(r);
        }
        out
    }

    /// The last `n` lines of plain output (including the current, unfinished line).
    #[must_use]
    pub fn tail(&self, n: usize) -> Vec<String> {
        let mut out: Vec<String> = self
            .tail
            .iter()
            .skip(self.tail.len().saturating_sub(n))
            .cloned()
            .collect();
        let current = self.line.current();
        if !current.trim().is_empty() {
            out.push(current.trim_end().to_owned());
        }
        out
    }

    /// The working directory last reported by the shell.
    #[must_use]
    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    /// Whether the shell emits integration marks (blocks are only recorded then).
    #[must_use]
    pub fn has_integration(&self) -> bool {
        self.integration
    }

    /// Whether a command is running right now.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.phase == Phase::Running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "\x1b]133;A\x07";
    const B: &str = "\x1b]133;B\x07";

    fn prompt(cwd: &str) -> String {
        format!("\x1b]7;file://host{cwd}\x07{A}\x1b[32muser@host\x1b[0m:{cwd}$ {B}")
    }

    #[test]
    fn records_commands_with_output_and_exit() {
        let mut r = BlockRecorder::new();
        r.feed(b"Welcome\r\n", 1);
        r.feed(prompt("/srv").as_bytes(), 2);
        r.feed(b"ls -la\r\n", 3);
        r.feed(b"\x1b]633;E;ls -la\x07\x1b]133;C\x07", 4);
        r.feed(b"\x1b[1;34mapp\x1b[0m\r\nREADME.md\r\n", 5);
        assert!(r.is_running());
        assert_eq!(r.running().map(|b| b.output), Some("app\nREADME.md".into()));
        r.feed(b"\x1b]133;D;0\x07", 6);
        r.feed(prompt("/srv").as_bytes(), 7);
        r.feed(b"false\r\n\x1b]133;C\x07", 8);
        r.feed(b"\x1b]133;D;1\x07", 9);
        let blocks: Vec<_> = r.blocks().iter().cloned().collect();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].command, "ls -la");
        assert_eq!(blocks[0].output, "app\nREADME.md");
        assert_eq!(blocks[0].exit, Some(0));
        assert_eq!(blocks[0].cwd.as_deref(), Some("/srv"));
        assert_eq!((blocks[0].started_at, blocks[0].finished_at), (4, Some(6)));
        // Without 633;E the echoed input is the command.
        assert_eq!(blocks[1].command, "false");
        assert_eq!(blocks[1].exit, Some(1));
        assert!(r.has_integration());
        assert!(r.tail(10).contains(&"README.md".to_owned()));
    }

    #[test]
    fn marks_split_across_chunks() {
        let mut r = BlockRecorder::new();
        let stream = format!(
            "{}echo hi\r\n\x1b]633;E;echo hi\x07\x1b]133;C\x07hi\r\n\x1b]133;D;0\x07",
            prompt("/")
        );
        for b in stream.as_bytes().chunks(3) {
            r.feed(b, 0);
        }
        let b = &r.blocks()[0];
        assert_eq!((b.command.as_str(), b.output.as_str()), ("echo hi", "hi"));
    }

    #[test]
    fn long_output_keeps_head_and_tail() {
        let mut r = BlockRecorder::new();
        r.feed(
            format!("{}seq\r\n\x1b]133;C\x07", prompt("/")).as_bytes(),
            0,
        );
        for i in 0..5000 {
            r.feed(format!("{i}\r\n").as_bytes(), 0);
        }
        r.feed(b"\x1b]133;D;0\x07", 0);
        let b = &r.blocks()[0];
        assert_eq!(b.omitted_lines, 5000 - MAX_BLOCK_LINES);
        assert!(b.output.starts_with("0\n1\n"));
        assert!(b.output.ends_with("4998\n4999"));
        assert!(b.output.contains("lines omitted"));
    }

    #[test]
    fn bounded_number_of_blocks() {
        let mut r = BlockRecorder::new();
        for i in 0..(MAX_BLOCKS + 10) {
            r.feed(
                format!("{}c{i}\r\n\x1b]133;C\x07x\r\n\x1b]133;D;0\x07", prompt("/")).as_bytes(),
                0,
            );
        }
        assert_eq!(r.blocks().len(), MAX_BLOCKS);
        assert_eq!(r.blocks()[0].command, "c10");
        assert_eq!(r.recent(2).len(), 2);
    }

    #[test]
    fn without_integration_only_the_tail_is_kept() {
        let mut r = BlockRecorder::new();
        r.feed(b"$ uptime\r\n 10:00 up 3 days\r\n$ ", 0);
        assert!(!r.has_integration());
        assert!(r.blocks().is_empty());
        assert_eq!(r.tail(10), ["$ uptime", " 10:00 up 3 days", "$"]);
    }

    #[test]
    fn a_new_prompt_closes_an_unfinished_block() {
        let mut r = BlockRecorder::new();
        r.feed(
            format!("{}sleep 9\r\n\x1b]133;C\x07^C\r\n", prompt("/")).as_bytes(),
            0,
        );
        r.feed(prompt("/").as_bytes(), 5);
        assert_eq!(r.blocks()[0].command, "sleep 9");
        assert_eq!(r.blocks()[0].exit, None);
        assert!(!r.is_running());
    }
}
