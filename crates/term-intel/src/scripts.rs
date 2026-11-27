// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in shell integration scripts.
//!
//! Each script makes an interactive shell emit `OSC 7` (working directory) and `OSC 133`
//! prompt/command marks (zsh and fish 3 also report the command line via `OSC 633 ; E`), which
//! [`crate::shell::ShellIntegrationParser`] understands. The scripts are:
//!
//! * **opt-in**: they do nothing unless `SSH_CLIENT_SHELL_INTEGRATION` is set (non-empty) in
//!   the remote environment, so sourcing them from a dotfile is harmless;
//! * **idempotent**: sourcing twice installs the hooks once;
//! * **small**: no external commands, no subshells per prompt.
//!
//! The app may inject one of them after login (e.g. by sending
//! `export SSH_CLIENT_SHELL_INTEGRATION=1; eval "$(…)"`) only with the user's consent.

/// Name of the environment variable that enables the scripts.
pub const ENABLE_VAR: &str = "SSH_CLIENT_SHELL_INTEGRATION";

/// Integration script for bash 4.4+ (uses `PROMPT_COMMAND`, `PS0` and `PS1`).
pub const BASH: &str = include_str!("../scripts/bash.sh");
/// Integration script for zsh 5.1+ (uses `precmd` / `preexec` hooks).
pub const ZSH: &str = include_str!("../scripts/zsh.zsh");
/// Integration script for fish 3.1+ (wraps `fish_prompt`, uses `fish_preexec` /
/// `fish_postexec` events).
pub const FISH: &str = include_str!("../scripts/fish.fish");

/// Shells with an integration script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl Shell {
    /// Guesses the shell from a login shell path or name such as `/usr/bin/zsh` or `-bash`.
    #[must_use]
    pub fn from_path(path: &str) -> Option<Self> {
        let name = path
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .trim_start_matches('-');
        match name {
            "bash" => Some(Self::Bash),
            "zsh" => Some(Self::Zsh),
            "fish" => Some(Self::Fish),
            _ => None,
        }
    }

    /// The integration script for this shell.
    #[must_use]
    pub const fn script(self) -> &'static str {
        match self {
            Self::Bash => BASH,
            Self::Zsh => ZSH,
            Self::Fish => FISH,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn scripts_are_guarded_and_licensed() {
        for sh in [Shell::Bash, Shell::Zsh, Shell::Fish] {
            let s = sh.script();
            assert!(s.starts_with("# SPDX-License-Identifier: GPL-3.0-or-later\n"));
            assert!(s.contains(ENABLE_VAR));
            assert!(s.contains("__sshc_si"));
            assert!(s.contains("]7;file://"));
            assert!(s.contains("]133;A"));
            assert!(s.len() < 4096, "keep scripts small");
        }
    }

    #[test]
    fn shell_detection() {
        assert_eq!(Shell::from_path("/usr/bin/zsh"), Some(Shell::Zsh));
        assert_eq!(Shell::from_path("-bash"), Some(Shell::Bash));
        assert_eq!(Shell::from_path("fish"), Some(Shell::Fish));
        assert_eq!(Shell::from_path("/bin/sh"), None);
    }

    /// Syntax-checks the scripts with locally installed shells, if any.
    #[test]
    fn scripts_parse_with_installed_shells() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        for (bin, script) in [("bash", BASH), ("zsh", ZSH), ("fish", FISH)] {
            let Ok(mut child) = Command::new(bin)
                .arg("-n")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
            else {
                continue;
            };
            child
                .stdin
                .take()
                .unwrap()
                .write_all(script.as_bytes())
                .unwrap();
            let out = child.wait_with_output().unwrap();
            assert!(
                out.status.success(),
                "{bin}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}
