// SPDX-License-Identifier: GPL-3.0-or-later
//! Terminal intelligence for ssh-client: deterministic, offline, no AI.
//!
//! * [`shell`]: sans-IO parser for shell integration escape sequences (OSC 7, 133, 633, 1337,
//!   9;4) and heuristic password prompt detection.
//! * [`scripts`]: small opt-in bash / zsh / fish snippets that make a remote shell emit those
//!   sequences.
//! * [`vt`]: incremental VT escape scanner and ANSI stripper.
//!
//! All parsers are incremental, robust to arbitrary input splits, never panic on untrusted
//! input and use bounded memory.

#![forbid(unsafe_code)]

pub mod shell;
pub mod vt;

pub use shell::{ProgressState, ShellEvent, ShellIntegrationParser, is_password_prompt};
pub use vt::{AnsiStripper, VtEvent, VtScanner, strip_ansi};
