// SPDX-License-Identifier: GPL-3.0-or-later
//! Terminal intelligence for ssh-client: deterministic, offline, no AI.
//!
//! * [`shell`]: sans-IO parser for shell integration escape sequences (OSC 7, 133, 633, 1337,
//!   9;4) and heuristic password prompt detection.
//! * [`scripts`]: small opt-in bash / zsh / fish snippets that make a remote shell emit those
//!   sequences.
//! * [`history`]: bash / zsh / fish history import and a frecency-ranked [`HistoryIndex`].
//! * [`complete`]: the [`Completer`] (history, snippets, command specs and remote paths).
//! * [`spec`]: the command spec model. The built-in specs for about 60 common commands are in
//!   `specs/commands.json`; JSON cannot carry an SPDX header, so note that this file is part
//!   of this crate and licensed GPL-3.0-or-later like the rest of it.
//! * [`risk`]: deterministic [`assess_risk`] rating of command lines.
//! * [`tokenize`]: quote-aware shell tokenizer used by completion and risk rating.
//! * [`vt`]: incremental VT escape scanner and ANSI stripper.
//!
//! All parsers are incremental where it matters, robust to arbitrary input splits, never
//! panic on untrusted input and use bounded memory.

#![forbid(unsafe_code)]

pub mod complete;
pub mod history;
pub mod risk;
pub mod scripts;
pub mod shell;
pub mod spec;
pub mod tokenize;
pub mod vt;

pub use complete::{
    Completer, Completion, NeedsPaths, PathEntry, PathProvider, Suggestion, SuggestionKind,
};
pub use history::{HistoryEntry, HistoryFormat, HistoryIndex, HistoryItem};
pub use risk::{Risk, RiskLevel, assess_risk};
pub use shell::{ProgressState, ShellEvent, ShellIntegrationParser, is_password_prompt};
pub use spec::{ArgKind, SpecSet};
pub use vt::{AnsiStripper, VtEvent, VtScanner, strip_ansi};
