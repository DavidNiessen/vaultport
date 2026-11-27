// SPDX-License-Identifier: GPL-3.0-or-later
//! A forgiving POSIX-shell-like tokenizer.
//!
//! It understands single and double quotes, `$'…'`, backslash escapes, comments, command
//! substitution (`$(…)`, backticks, `${…}` are kept inside the word), control operators
//! (`|`, `|&`, `||`, `&&`, `&`, `;`, `;;`, newline, parentheses) and redirections (`>`, `>>`,
//! `<`, `<<`, `<<<`, `&>`, `2>`, `>&`, …). It never fails: unterminated quotes simply extend
//! to the end of the input and are reported through [`Token::open_quote`], which is exactly
//! the situation while the user is typing.
//!
//! [`command_context`] finds the simple command and the word being completed at the cursor;
//! [`parse_commands`] splits a full line into simple commands for analysis.

use std::ops::Range;

/// Kind of a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// A word (command name, argument, assignment).
    Word,
    /// `|`
    Pipe,
    /// `|&`
    PipeAll,
    /// `&&`
    And,
    /// `||`
    Or,
    /// `;`, `;;` or a newline.
    Semi,
    /// `&`
    Background,
    /// `(`
    OpenParen,
    /// `)`
    CloseParen,
    /// A redirection operator, including an fd prefix (`2>`, `>>`, `<`, `&>`, …).
    Redirect,
}

impl TokenKind {
    /// Whether this token separates simple commands.
    #[must_use]
    pub const fn is_separator(self) -> bool {
        !matches!(self, Self::Word | Self::Redirect)
    }
}

/// A token with its byte range in the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    /// For words: the value with quotes and escapes removed. For operators: the operator.
    pub text: String,
    /// Byte range in the input.
    pub span: Range<usize>,
    /// For words: the quote character that is still open at the end of input, if any.
    pub open_quote: Option<char>,
    /// For words: whether any part was quoted or escaped.
    pub quoted: bool,
}

/// Tokenizes `input`. Linear time; never fails.
#[must_use]
pub fn tokenize(input: &str) -> Vec<Token> {
    Lexer {
        s: input.as_bytes(),
        input,
        i: 0,
        out: Vec::new(),
    }
    .run()
}

struct Lexer<'a> {
    s: &'a [u8],
    input: &'a str,
    i: usize,
    out: Vec<Token>,
}

impl Lexer<'_> {
    fn peek(&self, off: usize) -> Option<u8> {
        self.s.get(self.i + off).copied()
    }

    fn op(&mut self, kind: TokenKind, len: usize) {
        let start = self.i;
        self.i += len;
        self.out.push(Token {
            kind,
            text: self.input[start..self.i].to_owned(),
            span: start..self.i,
            open_quote: None,
            quoted: false,
        });
    }

    fn run(mut self) -> Vec<Token> {
        while let Some(b) = self.peek(0) {
            match b {
                b' ' | b'\t' | b'\r' => self.i += 1,
                b'\n' => self.op(TokenKind::Semi, 1),
                b'#' => {
                    while self.peek(0).is_some_and(|c| c != b'\n') {
                        self.i += 1;
                    }
                }
                b'|' => match self.peek(1) {
                    Some(b'|') => self.op(TokenKind::Or, 2),
                    Some(b'&') => self.op(TokenKind::PipeAll, 2),
                    _ => self.op(TokenKind::Pipe, 1),
                },
                b'&' => match self.peek(1) {
                    Some(b'&') => self.op(TokenKind::And, 2),
                    Some(b'>') => {
                        let len = if self.peek(2) == Some(b'>') { 3 } else { 2 };
                        self.op(TokenKind::Redirect, len);
                    }
                    _ => self.op(TokenKind::Background, 1),
                },
                b';' => {
                    let len = if self.peek(1) == Some(b';') { 2 } else { 1 };
                    self.op(TokenKind::Semi, len);
                }
                b'(' => self.op(TokenKind::OpenParen, 1),
                b')' => self.op(TokenKind::CloseParen, 1),
                b'>' | b'<' => {
                    let len = self.redirect_len();
                    self.op(TokenKind::Redirect, len);
                }
                _ => self.word(),
            }
        }
        self.out
    }

    fn redirect_len(&self) -> usize {
        let a = self.peek(0);
        let b = self.peek(1);
        let c = self.peek(2);
        match (a, b, c) {
            (Some(b'<'), Some(b'<'), Some(b'<')) => 3,
            (Some(b'<'), Some(b'<' | b'&' | b'>'), _)
            | (Some(b'>'), Some(b'>' | b'&' | b'|'), _) => 2,
            _ => 1,
        }
    }

    /// Consumes a word starting at `self.i`.
    fn word(&mut self) {
        let start = self.i;
        let mut text: Vec<u8> = Vec::new();
        let mut open_quote = None;
        let mut quoted = false;
        while let Some(b) = self.peek(0) {
            match b {
                b' ' | b'\t' | b'\r' | b'\n' | b'|' | b'&' | b';' | b'(' | b')' => break,
                b'<' | b'>' => {
                    // `2>file`: a word made only of digits becomes the fd of the redirection.
                    if !quoted && !text.is_empty() && text.iter().all(u8::is_ascii_digit) {
                        let len = self.redirect_len();
                        self.i += len;
                        self.out.push(Token {
                            kind: TokenKind::Redirect,
                            text: self.input[start..self.i].to_owned(),
                            span: start..self.i,
                            open_quote: None,
                            quoted: false,
                        });
                        return;
                    }
                    break;
                }
                b'\\' => {
                    quoted = true;
                    match self.peek(1) {
                        Some(b'\n') => self.i += 2,
                        Some(c) => {
                            let end = (self.i + 1 + utf8_len(c)).min(self.s.len());
                            text.extend_from_slice(&self.s[self.i + 1..end]);
                            self.i = end;
                        }
                        None => {
                            self.i += 1;
                            open_quote = Some('\\');
                        }
                    }
                }
                b'\'' => {
                    quoted = true;
                    self.i += 1;
                    if let Some(p) = self.s[self.i..].iter().position(|&c| c == b'\'') {
                        text.extend_from_slice(&self.s[self.i..self.i + p]);
                        self.i += p + 1;
                    } else {
                        text.extend_from_slice(&self.s[self.i..]);
                        self.i = self.s.len();
                        open_quote = Some('\'');
                    }
                }
                b'"' => {
                    quoted = true;
                    self.i += 1;
                    if !self.double_quoted(&mut text) {
                        open_quote = Some('"');
                    }
                }
                b'$' if self.peek(1) == Some(b'\'') => {
                    quoted = true;
                    self.i += 2;
                    if !self.ansi_c_quoted(&mut text) {
                        open_quote = Some('\'');
                    }
                }
                b'$' if matches!(self.peek(1), Some(b'(' | b'{')) => {
                    let from = self.i;
                    self.i += 1;
                    self.balanced();
                    text.extend_from_slice(&self.s[from..self.i]);
                }
                b'`' => {
                    let from = self.i;
                    self.i += 1;
                    if let Some(p) = self.s[self.i..].iter().position(|&c| c == b'`') {
                        self.i += p + 1;
                    } else {
                        self.i = self.s.len();
                        open_quote = Some('`');
                    }
                    text.extend_from_slice(&self.s[from..self.i]);
                }
                _ => {
                    text.push(b);
                    self.i += 1;
                }
            }
        }
        self.out.push(Token {
            kind: TokenKind::Word,
            text: String::from_utf8_lossy(&text).into_owned(),
            span: start..self.i,
            open_quote,
            quoted,
        });
    }

    /// Inside `"…"`; returns false if unterminated.
    fn double_quoted(&mut self, text: &mut Vec<u8>) -> bool {
        while let Some(b) = self.peek(0) {
            match b {
                b'"' => {
                    self.i += 1;
                    return true;
                }
                b'\\' => match self.peek(1) {
                    Some(c @ (b'"' | b'\\' | b'$' | b'`')) => {
                        text.push(c);
                        self.i += 2;
                    }
                    Some(b'\n') => self.i += 2,
                    _ => {
                        text.push(b'\\');
                        self.i += 1;
                    }
                },
                b'$' if matches!(self.peek(1), Some(b'(' | b'{')) => {
                    let from = self.i;
                    self.i += 1;
                    self.balanced();
                    text.extend_from_slice(&self.s[from..self.i]);
                }
                _ => {
                    text.push(b);
                    self.i += 1;
                }
            }
        }
        false
    }

    /// Inside `$'…'`; returns false if unterminated.
    fn ansi_c_quoted(&mut self, text: &mut Vec<u8>) -> bool {
        while let Some(b) = self.peek(0) {
            match b {
                b'\'' => {
                    self.i += 1;
                    return true;
                }
                b'\\' => {
                    let esc = match self.peek(1) {
                        Some(b'n') => Some(b'\n'),
                        Some(b't') => Some(b'\t'),
                        Some(b'r') => Some(b'\r'),
                        Some(b'e' | b'E') => Some(0x1b),
                        Some(b'a') => Some(0x07),
                        Some(c @ (b'\\' | b'\'' | b'"' | b'?')) => Some(c),
                        _ => None,
                    };
                    if let Some(e) = esc {
                        text.push(e);
                        self.i += 2;
                    } else {
                        text.push(b'\\');
                        self.i += 1;
                    }
                }
                _ => {
                    text.push(b);
                    self.i += 1;
                }
            }
        }
        false
    }

    /// At an opening `(` or `{`: skips to the matching closer, honoring nested quotes.
    fn balanced(&mut self) {
        let mut depth = 0usize;
        while let Some(b) = self.peek(0) {
            match b {
                b'(' | b'{' => depth += 1,
                b')' | b'}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        self.i += 1;
                        return;
                    }
                }
                b'\\' => self.i += 1,
                b'\'' => {
                    if let Some(p) = self.s[self.i + 1..].iter().position(|&c| c == b'\'') {
                        self.i += p + 1;
                    } else {
                        self.i = self.s.len();
                        return;
                    }
                }
                b'"' => {
                    let mut j = self.i + 1;
                    while j < self.s.len() && self.s[j] != b'"' {
                        if self.s[j] == b'\\' {
                            j += 1;
                        }
                        j += 1;
                    }
                    self.i = j.min(self.s.len());
                }
                _ => {}
            }
            self.i += 1;
        }
        self.i = self.i.min(self.s.len());
    }
}

fn utf8_len(lead: u8) -> usize {
    match lead {
        0xf0..=0xff => 4,
        0xe0..=0xef => 3,
        0xc0..=0xdf => 2,
        _ => 1,
    }
}

/// The simple command around the cursor.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandContext {
    /// Complete words of the simple command before the word being completed (unquoted).
    pub words: Vec<String>,
    /// The (unquoted) part of the word being completed that lies before the cursor.
    pub current: String,
    /// Byte range of the input to replace when accepting a suggestion for `current`.
    pub replace: Range<usize>,
    /// Quote still open in the current word.
    pub open_quote: Option<char>,
    /// The current word is the target of a redirection (`> fi|`).
    pub after_redirect: bool,
    /// Byte offset where the simple command starts.
    pub command_start: usize,
}

/// Finds the simple command and the word being completed at `cursor` (a byte offset, clamped
/// to the input and to a char boundary). Only text before the cursor is considered.
#[must_use]
pub fn command_context(line: &str, cursor: usize) -> CommandContext {
    let mut cursor = cursor.min(line.len());
    while !line.is_char_boundary(cursor) {
        cursor -= 1;
    }
    let before = &line[..cursor];
    let tokens = tokenize(before);
    let seg_start = tokens
        .iter()
        .rposition(|t| t.kind.is_separator())
        .map_or(0, |i| i + 1);
    let seg = &tokens[seg_start..];
    let command_start = seg
        .first()
        .map_or_else(|| tokens.last().map_or(0, |t| t.span.end), |t| t.span.start);

    // Is the cursor inside the last word, or after whitespace/operator?
    let last = seg.last();
    let in_word = last.is_some_and(|t| t.kind == TokenKind::Word && t.span.end == cursor);

    let mut ctx = CommandContext {
        command_start,
        replace: cursor..cursor,
        ..CommandContext::default()
    };
    let (complete, current) = if in_word {
        (&seg[..seg.len() - 1], last)
    } else {
        (seg, None)
    };
    let mut pending_redirect = false;
    for t in complete {
        match t.kind {
            TokenKind::Redirect => pending_redirect = true,
            TokenKind::Word if pending_redirect => pending_redirect = false,
            TokenKind::Word => ctx.words.push(t.text.clone()),
            _ => {}
        }
    }
    ctx.after_redirect = pending_redirect;
    if let Some(t) = current {
        ctx.current.clone_from(&t.text);
        ctx.replace = t.span.start..cursor;
        ctx.open_quote = t.open_quote;
    }
    ctx
}

/// A redirection in a simple command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirection {
    /// The operator, e.g. `>`, `2>>`, `<`.
    pub op: String,
    /// The target word (unquoted); empty if missing.
    pub target: String,
}

/// A simple command produced by [`parse_commands`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SimpleCommand {
    /// Words with quotes removed (assignments included).
    pub words: Vec<String>,
    pub redirections: Vec<Redirection>,
    /// Whether stdin comes from the previous command through a pipe.
    pub piped_from_previous: bool,
}

/// Splits a full command line into simple commands, in order.
#[must_use]
pub fn parse_commands(line: &str) -> Vec<SimpleCommand> {
    let mut out = Vec::new();
    let mut cur = SimpleCommand::default();
    let mut pending: Option<String> = None;
    let mut next_piped = false;
    let flush =
        |cur: &mut SimpleCommand, out: &mut Vec<SimpleCommand>, pending: &mut Option<String>| {
            if let Some(op) = pending.take() {
                cur.redirections.push(Redirection {
                    op,
                    target: String::new(),
                });
            }
            if !cur.words.is_empty() || !cur.redirections.is_empty() {
                out.push(std::mem::take(cur));
            } else {
                *cur = SimpleCommand::default();
            }
        };
    for t in tokenize(line) {
        match t.kind {
            TokenKind::Word => {
                if let Some(op) = pending.take() {
                    cur.redirections.push(Redirection { op, target: t.text });
                } else {
                    if cur.words.is_empty() {
                        cur.piped_from_previous = next_piped;
                    }
                    cur.words.push(t.text);
                }
            }
            TokenKind::Redirect => {
                if let Some(op) = pending.replace(t.text) {
                    cur.redirections.push(Redirection {
                        op,
                        target: String::new(),
                    });
                }
            }
            kind => {
                flush(&mut cur, &mut out, &mut pending);
                next_piped = matches!(kind, TokenKind::Pipe | TokenKind::PipeAll);
            }
        }
    }
    flush(&mut cur, &mut out, &mut pending);
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use proptest::prelude::*;

    fn kinds(s: &str) -> Vec<(TokenKind, String)> {
        tokenize(s).into_iter().map(|t| (t.kind, t.text)).collect()
    }

    #[test]
    fn quotes_and_escapes() {
        let t = tokenize(r#"echo 'a b' "c \"d\" $x" e\ f $'g\nh' "$(ls "x y")""#);
        let words: Vec<_> = t.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(
            words,
            ["echo", "a b", "c \"d\" $x", "e f", "g\nh", "$(ls \"x y\")"]
        );
        assert!(t[1].quoted && !t[0].quoted);
        assert_eq!(t[1].span, 5..10);
    }

    #[test]
    fn operators_and_redirections() {
        use TokenKind::*;
        let k = kinds("a|b||c&&d;e&f |& g 2>/dev/null >>log <in &>all <<<str ; (h)");
        let expect = [
            (Word, "a"),
            (Pipe, "|"),
            (Word, "b"),
            (Or, "||"),
            (Word, "c"),
            (And, "&&"),
            (Word, "d"),
            (Semi, ";"),
            (Word, "e"),
            (Background, "&"),
            (Word, "f"),
            (PipeAll, "|&"),
            (Word, "g"),
            (Redirect, "2>"),
            (Word, "/dev/null"),
            (Redirect, ">>"),
            (Word, "log"),
            (Redirect, "<"),
            (Word, "in"),
            (Redirect, "&>"),
            (Word, "all"),
            (Redirect, "<<<"),
            (Word, "str"),
            (Semi, ";"),
            (OpenParen, "("),
            (Word, "h"),
            (CloseParen, ")"),
        ];
        let expect: Vec<_> = expect.iter().map(|(k, s)| (*k, (*s).to_owned())).collect();
        assert_eq!(k, expect);
    }

    #[test]
    fn comments_and_unterminated_quotes() {
        assert_eq!(kinds("ls # comment | x").len(), 1);
        let t = tokenize("echo \"unterminated | still");
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].open_quote, Some('"'));
        assert_eq!(t[1].text, "unterminated | still");
        assert_eq!(tokenize("x '")[1].open_quote, Some('\''));
        assert_eq!(tokenize("x \\")[1].open_quote, Some('\\'));
    }

    #[test]
    fn context_at_cursor() {
        let c = command_context("cd /tmp && git che", 18);
        assert_eq!(c.words, ["git"]);
        assert_eq!(c.current, "che");
        assert_eq!(c.replace, 15..18);
        assert_eq!(c.command_start, 11);

        let c = command_context("ls -la | grep ", 14);
        assert_eq!(c.words, ["grep"]);
        assert_eq!(c.current, "");
        assert_eq!(c.replace, 14..14);

        let c = command_context("cat 'My Fi", 10);
        assert_eq!(c.current, "My Fi");
        assert_eq!(c.open_quote, Some('\''));
        assert_eq!(c.replace, 4..10);

        let c = command_context("echo hi > ou", 12);
        assert!(c.after_redirect);
        assert_eq!(c.words, ["echo", "hi"]);

        let c = command_context("git commit -m x", 6);
        assert_eq!(c.words, ["git"]);
        assert_eq!(c.current, "co");
        assert_eq!(c.replace, 4..6);
        let c = command_context("cat My\\ Fi", 10);
        assert_eq!(c.current, "My Fi");
        // Cursor inside a multi-byte char is clamped.
        let c = command_context("é", 1);
        assert_eq!(c.current, "");
    }

    #[test]
    fn simple_commands() {
        let v = parse_commands("curl -fsSL https://x | sudo bash; echo done > /tmp/o 2>&1");
        assert_eq!(v.len(), 3);
        assert_eq!(v[0].words, ["curl", "-fsSL", "https://x"]);
        assert!(v[1].piped_from_previous);
        assert_eq!(v[1].words, ["sudo", "bash"]);
        assert!(!v[2].piped_from_previous);
        assert_eq!(v[2].redirections[0].target, "/tmp/o");
        assert_eq!(v[2].redirections[1].op, "2>&");
        assert_eq!(v[2].redirections[1].target, "1");
    }

    proptest! {
        #[test]
        fn tokenizer_never_panics(s in "\\PC{0,80}", cursor in 0usize..100) {
            let _ = tokenize(&s);
            let _ = parse_commands(&s);
            let c = command_context(&s, cursor);
            prop_assert!(c.replace.end <= s.len());
            prop_assert!(c.replace.start <= c.replace.end);
        }

        #[test]
        fn tokenizer_handles_shell_noise(s in "[a-z'\"\\\\$()`{}|&;<> \n0-9]{0,60}", cursor in 0usize..70) {
            let _ = tokenize(&s);
            let _ = parse_commands(&s);
            let _ = command_context(&s, cursor);
        }
    }
}
