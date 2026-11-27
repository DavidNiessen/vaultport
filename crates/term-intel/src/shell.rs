// SPDX-License-Identifier: GPL-3.0-or-later
//! Sans-IO shell integration parser.
//!
//! Feed terminal *output* to [`ShellIntegrationParser::feed`] in whatever chunks it arrives;
//! sequences split across chunks are reassembled. Recognized sequences:
//!
//! | Sequence | Meaning |
//! |---|---|
//! | `OSC 7 ; file://host/path` | working directory (percent-decoded) |
//! | `OSC 133 ; A` / `B` / `C` / `D [; exit]` | prompt start, command start, executed, finished |
//! | `OSC 133 ; C ; cmdline_url=…` | executed, with percent-encoded command line |
//! | `OSC 633 ; A`–`D`, `E ; cmdline`, `P ; Cwd=…` | VS Code variant |
//! | `OSC 1337 ; CurrentDir=…` | iTerm2 working directory |
//! | `OSC 9 ; 4 ; state ; percent` | ConEmu / Windows Terminal progress |
//!
//! In addition, the last visible output line is checked after every chunk for a password
//! prompt (see [`is_password_prompt`]). [`ShellEvent::PasswordPrompt`] is emitted once per
//! prompt, and [`ShellEvent::PasswordPromptEnded`] when the line is finished (newline) or a new
//! prompt / command starts. Use these to toggle sensitive mode in session recording.

use crate::vt::{LineBuf, VtEvent, VtScanner};
use serde::{Deserialize, Serialize};

/// Maximum length of a command line taken from `OSC 633 ; E`.
const MAX_COMMAND_LEN: usize = 4 * 1024;

/// An event recognized in the output stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "data")]
#[non_exhaustive]
pub enum ShellEvent {
    /// The shell reported its working directory (absolute path on the remote host).
    Cwd(String),
    /// The prompt is about to be drawn (`133;A`).
    PromptStart,
    /// The prompt is done; user input starts here (`133;B`).
    CommandStart,
    /// The command line was accepted and is executing (`133;C`).
    CommandExecuted {
        /// The command line, if the shell reported it (`633;E` or `cmdline_url`).
        command: Option<String>,
    },
    /// The command finished (`133;D`).
    CommandFinished {
        /// Exit status, if reported.
        exit: Option<i32>,
    },
    /// Progress indicator (`OSC 9;4`).
    Progress {
        state: ProgressState,
        /// 0–100, where the state carries a value.
        percent: Option<u8>,
    },
    /// The last output line looks like a password prompt.
    PasswordPrompt,
    /// A previously reported password prompt is no longer active.
    PasswordPromptEnded,
}

/// State of an `OSC 9;4` progress report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProgressState {
    /// Hide the progress indicator (`0`).
    Clear,
    /// Normal progress (`1`).
    Normal,
    /// Error state (`2`).
    Error,
    /// Busy without a known percentage (`3`).
    Indeterminate,
    /// Paused / warning (`4`).
    Paused,
}

/// Incremental shell integration parser. See the [module docs](self).
#[derive(Debug, Clone)]
pub struct ShellIntegrationParser {
    scanner: VtScanner,
    line: LineBuf,
    pending_command: Option<String>,
    password_active: bool,
}

impl Default for ShellIntegrationParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellIntegrationParser {
    /// Creates a parser.
    #[must_use]
    pub fn new() -> Self {
        Self {
            scanner: VtScanner::new(),
            line: LineBuf::new(512),
            pending_command: None,
            password_active: false,
        }
    }

    /// Feeds an output chunk and returns the events it completed.
    pub fn feed(&mut self, data: &[u8]) -> Vec<ShellEvent> {
        let mut out = Vec::new();
        self.feed_with(data, |e| out.push(e));
        out
    }

    /// Feeds an output chunk, passing each completed event to `emit`.
    pub fn feed_with(&mut self, data: &[u8], mut emit: impl FnMut(ShellEvent)) {
        let Self {
            scanner,
            line,
            pending_command,
            password_active,
        } = self;
        scanner.feed(data, |ev| match ev {
            VtEvent::Print(t) => line.print(t, &mut |_| {}),
            VtEvent::Control(c) => {
                let mut completed = false;
                line.control(c, &mut |_| completed = true);
                if completed && *password_active {
                    *password_active = false;
                    emit(ShellEvent::PasswordPromptEnded);
                }
            }
            VtEvent::Osc(payload) => {
                if let Some(e) = parse_osc(payload, pending_command) {
                    if *password_active
                        && matches!(
                            e,
                            ShellEvent::PromptStart | ShellEvent::CommandExecuted { .. }
                        )
                    {
                        *password_active = false;
                        emit(ShellEvent::PasswordPromptEnded);
                    }
                    if matches!(e, ShellEvent::PromptStart) {
                        line.clear();
                    }
                    emit(e);
                }
            }
        });
        if !self.password_active && is_password_prompt_bytes(self.line.current_bytes()) {
            self.password_active = true;
            emit(ShellEvent::PasswordPrompt);
        }
    }

    /// Whether a password prompt is currently believed to be active.
    #[must_use]
    pub fn password_prompt_active(&self) -> bool {
        self.password_active
    }

    /// The current (last, incomplete) output line without escape sequences.
    #[must_use]
    pub fn current_line(&self) -> String {
        self.line.current()
    }
}

fn parse_osc(payload: &[u8], pending_command: &mut Option<String>) -> Option<ShellEvent> {
    let (code, rest) = split_once(payload, b';').unwrap_or((payload, &[]));
    match code {
        b"7" => parse_file_url(rest).map(ShellEvent::Cwd),
        b"133" | b"633" => parse_semantic(rest, code == b"633", pending_command),
        b"1337" => rest
            .strip_prefix(b"CurrentDir=")
            .filter(|p| !p.is_empty())
            .map(|p| ShellEvent::Cwd(String::from_utf8_lossy(p).into_owned())),
        b"9" => parse_progress(
            rest.strip_prefix(b"4;")
                .or_else(|| if rest == b"4" { Some(&[][..]) } else { None })?,
        ),
        _ => None,
    }
}

fn parse_semantic(
    rest: &[u8],
    vscode: bool,
    pending_command: &mut Option<String>,
) -> Option<ShellEvent> {
    let (kind, params) = split_once(rest, b';').unwrap_or((rest, &[]));
    match kind {
        b"A" => Some(ShellEvent::PromptStart),
        b"B" => Some(ShellEvent::CommandStart),
        b"C" => {
            let mut command = pending_command.take();
            for kv in params.split(|&b| b == b';') {
                if let Some(v) = kv.strip_prefix(b"cmdline_url=") {
                    command = Some(truncate_command(percent_decode(v)));
                }
            }
            Some(ShellEvent::CommandExecuted { command })
        }
        b"D" => {
            let first = params.split(|&b| b == b';').next().unwrap_or_default();
            let exit = std::str::from_utf8(first)
                .ok()
                .and_then(|s| s.trim().parse::<i32>().ok());
            Some(ShellEvent::CommandFinished { exit })
        }
        b"E" if vscode => {
            let line = params.split(|&b| b == b';').next().unwrap_or_default();
            *pending_command = Some(truncate_command(vscode_unescape(line)));
            None
        }
        b"P" if vscode => {
            let v = vscode_unescape(params);
            v.strip_prefix("Cwd=")
                .filter(|p| !p.is_empty())
                .map(|p| ShellEvent::Cwd(p.to_owned()))
        }
        _ => None,
    }
}

fn parse_progress(params: &[u8]) -> Option<ShellEvent> {
    let mut it = params.split(|&b| b == b';');
    let st = it.next().and_then(parse_u32).unwrap_or(0);
    let pr = it.next().and_then(parse_u32).map(|p| p.min(100) as u8);
    let (state, percent) = match st {
        0 => (ProgressState::Clear, None),
        1 => (ProgressState::Normal, Some(pr.unwrap_or(0))),
        2 => (ProgressState::Error, pr),
        3 => (ProgressState::Indeterminate, None),
        4 => (ProgressState::Paused, pr),
        _ => return None,
    };
    Some(ShellEvent::Progress { state, percent })
}

fn parse_u32(b: &[u8]) -> Option<u32> {
    std::str::from_utf8(b).ok()?.trim().parse().ok()
}

fn split_once(b: &[u8], sep: u8) -> Option<(&[u8], &[u8])> {
    let i = b.iter().position(|&x| x == sep)?;
    Some((&b[..i], &b[i + 1..]))
}

fn truncate_command(mut s: String) -> String {
    if s.len() > MAX_COMMAND_LEN {
        let mut cut = MAX_COMMAND_LEN;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
    }
    s
}

/// Parses `file://host/path` (or `kitty-shell-cwd://host/path`, or a bare absolute path) and
/// returns the percent-decoded path.
fn parse_file_url(url: &[u8]) -> Option<String> {
    let after_scheme = url
        .strip_prefix(b"file://")
        .or_else(|| url.strip_prefix(b"kitty-shell-cwd://"));
    let path = match after_scheme {
        Some(rest) => {
            let slash = rest.iter().position(|&b| b == b'/')?;
            &rest[slash..]
        }
        None if url.first() == Some(&b'/') => url,
        None => return None,
    };
    Some(percent_decode(path))
}

/// Decodes `%HH` escapes (invalid escapes are kept literally), then UTF-8 (lossy).
#[must_use]
pub fn percent_decode(s: &[u8]) -> String {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'%'
            && i + 2 < s.len()
            && let (Some(h), Some(l)) = (hex_val(s[i + 1]), hex_val(s[i + 2]))
        {
            out.push(h << 4 | l);
            i += 3;
            continue;
        }
        out.push(s[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Decodes VS Code's `OSC 633` escaping: `\\` and `\xHH`.
fn vscode_unescape(s: &[u8]) -> String {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'\\' {
            if s.get(i + 1) == Some(&b'\\') {
                out.push(b'\\');
                i += 2;
                continue;
            }
            if s.get(i + 1) == Some(&b'x')
                && let (Some(h), Some(l)) = (
                    s.get(i + 2).copied().and_then(hex_val),
                    s.get(i + 3).copied().and_then(hex_val),
                )
            {
                out.push(h << 4 | l);
                i += 4;
                continue;
            }
        }
        out.push(s[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Words that mark a secret prompt, lowercase, in several languages.
const SECRET_WORDS: &[&str] = &[
    "password",
    "passphrase",
    "passcode",
    "passwort",
    "kennwort",
    "mot de passe",
    "phrase secrète",
    "phrase de passe",
    "contraseña",
    "contrasena",
    "clave",
    "frase de contraseña",
    "senha",
    "wachtwoord",
    "lösenord",
    "adgangskode",
    "salasana",
    "hasło",
    "heslo",
    "пароль",
    "密码",
    "密碼",
    "パスワード",
    "암호",
    "비밀번호",
    "verification code",
    "enter pin",
    "pin for",
    "pin code",
];

/// Heuristically decides whether `line` (the last output line, escapes stripped) is a
/// prompt for a secret, e.g. `[sudo] password for alice: `, `Password:`,
/// `Enter passphrase for key '/home/a/.ssh/id_ed25519': `, `Passwort:`, `Mot de passe :` or
/// `Contraseña:`.
///
/// The line must end with a colon (optionally followed by whitespace) and contain one of the
/// known secret words near its end.
#[must_use]
pub fn is_password_prompt(line: &str) -> bool {
    let trimmed = line.trim_end();
    let Some(body) = trimmed
        .strip_suffix(':')
        .or_else(|| trimmed.strip_suffix('：'))
    else {
        return false;
    };
    if body.chars().count() > 200 || body.trim().is_empty() {
        return false;
    }
    let lower = body.to_lowercase();
    // Only look at the tail so long log lines mentioning a password do not match.
    let tail_start = lower.char_indices().rev().nth(79).map_or(0, |(i, _)| i);
    let tail = &lower[tail_start..];
    SECRET_WORDS.iter().any(|w| tail.contains(w))
}

fn is_password_prompt_bytes(line: &[u8]) -> bool {
    if line.is_empty() || line.len() > 1024 {
        return false;
    }
    let end = line.iter().rposition(|b| !b.is_ascii_whitespace());
    match end {
        // Cheap pre-check: must end with ':' or the last byte of '：' (U+FF1A = EF BC 9A).
        Some(i) if line[i] == b':' || line[i] == 0x9a => {
            is_password_prompt(&String::from_utf8_lossy(line))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use proptest::prelude::*;

    fn feed_all(chunks: &[&[u8]]) -> Vec<ShellEvent> {
        let mut p = ShellIntegrationParser::new();
        chunks.iter().flat_map(|c| p.feed(c)).collect()
    }

    fn cwd(p: &str) -> ShellEvent {
        ShellEvent::Cwd(p.into())
    }

    #[test]
    fn osc7_is_percent_decoded() {
        let ev = feed_all(&[b"\x1b]7;file://box/home/a/My%20Dir/%C3%A9\x07"]);
        assert_eq!(ev, [cwd("/home/a/My Dir/é")]);
        let ev = feed_all(&[b"\x1b]7;file:///tmp\x1b\\"]);
        assert_eq!(ev, [cwd("/tmp")]);
        let ev = feed_all(&[b"\x1b]7;file://nohostpath\x07"]);
        assert!(ev.is_empty());
        assert_eq!(percent_decode(b"100%"), "100%");
        assert_eq!(percent_decode(b"%4"), "%4");
        assert_eq!(percent_decode(b"%zz%41"), "%zzA");
    }

    #[test]
    fn osc133_lifecycle() {
        let ev = feed_all(&[
            b"\x1b]133;A\x07$ \x1b]133;B\x07",
            b"\x1b]133;C\x07output\r\n\x1b]133;D;127\x07",
            b"\x1b]133;D\x07\x1b]133;D;abc\x07",
        ]);
        assert_eq!(
            ev,
            [
                ShellEvent::PromptStart,
                ShellEvent::CommandStart,
                ShellEvent::CommandExecuted { command: None },
                ShellEvent::CommandFinished { exit: Some(127) },
                ShellEvent::CommandFinished { exit: None },
                ShellEvent::CommandFinished { exit: None },
            ]
        );
    }

    #[test]
    fn osc633_command_line_and_cwd() {
        let ev = feed_all(&[
            b"\x1b]633;E;echo a\\x3bb \\\\ c;nonce\x07\x1b]633;C\x07",
            b"\x1b]633;P;Cwd=/srv/x\x07\x1b]633;C\x07",
        ]);
        assert_eq!(
            ev,
            [
                ShellEvent::CommandExecuted {
                    command: Some("echo a;b \\ c".into())
                },
                cwd("/srv/x"),
                ShellEvent::CommandExecuted { command: None },
            ]
        );
        let ev = feed_all(&[b"\x1b]133;C;cmdline_url=ls%20-la\x07"]);
        assert_eq!(
            ev,
            [ShellEvent::CommandExecuted {
                command: Some("ls -la".into())
            }]
        );
    }

    #[test]
    fn iterm_cwd_and_progress() {
        let ev = feed_all(&[
            b"\x1b]1337;CurrentDir=/var/log\x07",
            b"\x1b]9;4;1;42\x07\x1b]9;4;3\x07\x1b]9;4;0;0\x07\x1b]9;4;2;250\x07\x1b]9;hello\x07",
        ]);
        assert_eq!(
            ev,
            [
                cwd("/var/log"),
                ShellEvent::Progress {
                    state: ProgressState::Normal,
                    percent: Some(42)
                },
                ShellEvent::Progress {
                    state: ProgressState::Indeterminate,
                    percent: None
                },
                ShellEvent::Progress {
                    state: ProgressState::Clear,
                    percent: None
                },
                ShellEvent::Progress {
                    state: ProgressState::Error,
                    percent: Some(100)
                },
            ]
        );
    }

    #[test]
    fn byte_by_byte_feeding_matches_whole() {
        let data: &[u8] = b"\x1b]133;A\x07x\x1b]7;file://h/a%20b\x1b\\\x1b[0m\x1b]133;D;1\x07";
        let whole = feed_all(&[data]);
        let mut p = ShellIntegrationParser::new();
        let split: Vec<_> = data.chunks(1).flat_map(|c| p.feed(c)).collect();
        assert_eq!(whole, split);
        assert_eq!(whole.len(), 3);
    }

    #[test]
    fn password_prompt_heuristics() {
        for yes in [
            "[sudo] password for alice: ",
            "Password:",
            "alice@db1's password: ",
            "Enter passphrase for key '/home/a/.ssh/id_ed25519': ",
            "Enter PASSPHRASE:",
            "(current) UNIX password:",
            "Retype new password:",
            "[sudo] Passwort für alice: ",
            "Mot de passe : ",
            "Contraseña:",
            "Password for 'https://git.example.com':",
            "Пароль:",
            "密码：",
            "Verification code: ",
        ] {
            assert!(is_password_prompt(yes), "{yes}");
        }
        for no in [
            "",
            ":",
            "alice@host:~$ ",
            "Password changed.",
            "password: hunter2",
            "Enter your name:",
            "The password policy requires at least eight characters and the following text keeps going on and on and on and on until it is very long:",
        ] {
            assert!(!is_password_prompt(no), "{no}");
        }
    }

    #[test]
    fn password_prompt_events() {
        let mut p = ShellIntegrationParser::new();
        assert!(p.feed(b"\x1b[1m[sudo] pass").is_empty());
        assert_eq!(p.feed(b"word for a: "), [ShellEvent::PasswordPrompt]);
        assert!(p.password_prompt_active());
        // Repeated chunks on the same line do not re-emit.
        assert!(p.feed(b"").is_empty());
        assert_eq!(p.feed(b"\r\n"), [ShellEvent::PasswordPromptEnded]);
        assert!(!p.password_prompt_active());
        assert_eq!(p.feed(b"Password: "), [ShellEvent::PasswordPrompt]);
        assert_eq!(
            p.feed(b"\x1b]133;A\x07"),
            [ShellEvent::PasswordPromptEnded, ShellEvent::PromptStart]
        );
    }

    proptest! {
        #[test]
        fn never_panics_and_split_invariant(data in proptest::collection::vec(any::<u8>(), 0..300), cut in 0usize..300) {
            let cut = cut.min(data.len());
            let a = feed_all(&[&data]);
            let b = feed_all(&[&data[..cut], &data[cut..]]);
            // Password prompt detection is evaluated at chunk ends, so compare OSC events only.
            let strip = |v: Vec<ShellEvent>| v.into_iter().filter(|e| !matches!(e, ShellEvent::PasswordPrompt | ShellEvent::PasswordPromptEnded)).collect::<Vec<_>>();
            prop_assert_eq!(strip(a), strip(b));
        }

        #[test]
        fn osc_fragments_never_panic(s in "\\x1b\\](7|133|633|1337|9);[ -~]{0,40}(\\x07|\\x1b\\\\)?") {
            let _ = feed_all(&[s.as_bytes()]);
        }
    }
}
