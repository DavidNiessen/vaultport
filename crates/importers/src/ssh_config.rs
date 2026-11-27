// SPDX-License-Identifier: GPL-3.0-or-later
//! OpenSSH `ssh_config` importer.
//!
//! Every concrete alias in a `Host` line (a pattern without `*`, `?` or `!`) becomes a
//! [`Host`]. Its settings are computed like `ssh -G` would: all blocks whose patterns match the
//! alias are applied in file order and the **first** value obtained for each option wins
//! (multi-valued options such as `IdentityFile` and `LocalForward` accumulate).
//!
//! Settings that come from blocks matching *everything* (`Host *`, `Match all` and options
//! before the first `Host` line) are put into one group, [`DEFAULTS_GROUP_LABEL`], that all
//! imported hosts belong to, so they are inherited exactly like other group defaults. Other
//! wildcard blocks (e.g. `Host *.prod.example.com`) are applied directly to the matching
//! concrete hosts; if they match none, a warning is emitted.
//!
//! * `Match` blocks (other than `Match all`) are skipped with a warning.
//! * `Include` is resolved through a caller-supplied callback ([`parse_with_includes`]),
//!   nested at most [`MAX_INCLUDE_DEPTH`] levels. [`include_paths`] lists a file's includes.
//! * `IdentityFile` paths become [`ImportedKey`](crate::ImportedKey) placeholders; the caller
//!   loads (and expands `~`, `%d`, ...) them.
//! * `ProxyJump` hops are resolved to imported hosts by alias, or new hosts are created for
//!   them (with that name's own matching settings, as OpenSSH does). The first hop's own jump
//!   chain is prepended, mirroring how OpenSSH connects to it.
//! * `ProxyCommand` is not supported (warning).

use crate::bundle::{ImportBundle, KeyFileFormat, WarningKind};
use crate::common::{parse_port, split_host_port};
use crate::error::ImportError;
use crate::finalize::finalize;
use crate::text::{MAX_INPUT_BYTES, check_size};
use core_model::{
    Credential, EnvVar, ForwardKind, Group, GroupId, Host, HostConfig, HostId, HostKeyPolicy,
    KeyId, PortForwardRule, RuleId, glob_match,
};
use std::collections::{HashMap, HashSet};

/// Maximum `Include` nesting depth.
pub const MAX_INCLUDE_DEPTH: usize = 8;
/// Label of the group that receives `Host *` defaults.
pub const DEFAULTS_GROUP_LABEL: &str = "OpenSSH defaults";

const MAX_BLOCKS: usize = 10_000;
const MAX_WILDCARD_BLOCKS: usize = 1_000;
const MAX_HOSTS: usize = 10_000;
const MAX_INCLUDES: usize = 1_000;
const MAX_JUMP_DEPTH: usize = 16;
const MAX_PATTERN_LEN: usize = 1_024;

/// Parses an `ssh_config` file. `Include` directives are not followed (each one produces a
/// warning); use [`parse_with_includes`] to resolve them.
pub fn parse(input: &str) -> Result<ImportBundle, ImportError> {
    parse_with_includes(input, |_| None)
}

/// Lists the arguments of the `Include` directives of `input` (not recursive), unexpanded, in
/// order. Relative paths are relative to `~/.ssh` (user config) in OpenSSH, and may contain
/// globs and `~`.
pub fn include_paths(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in input.lines() {
        if let Ok(Some((kw, args))) = split_line(line)
            && kw == "include"
        {
            out.extend(args);
        }
    }
    out
}

/// Parses an `ssh_config` file, resolving each `Include` argument through `resolver`.
///
/// `resolver` receives the raw argument (e.g. `config.d/*` or `~/.ssh/work`) and returns the
/// text to include, or `None` if it cannot be resolved (a warning is emitted). For glob
/// arguments it should return the concatenation of all matching files in lexical order, as
/// OpenSSH does. Includes nest at most [`MAX_INCLUDE_DEPTH`] levels, and the main file plus
/// all included text must not exceed [`MAX_INPUT_BYTES`] in total.
pub fn parse_with_includes(
    main: &str,
    resolver: impl Fn(&str) -> Option<String>,
) -> Result<ImportBundle, ImportError> {
    check_size(main.len())?;
    let mut bundle = ImportBundle::default();
    let mut p = Parser {
        blocks: vec![Block {
            kind: BlockKind::Global,
            guards: Vec::new(),
            line: 0,
            settings: Vec::new(),
        }],
        resolver: &resolver,
        total_bytes: main.len(),
        includes: 0,
        unknown: HashSet::new(),
        limit_warned: false,
    };
    p.parse_text(&mut bundle, main, None, 0, 0);
    let Parser { blocks, .. } = p;
    build(&mut bundle, &blocks);
    Ok(finalize(bundle))
}

// ---------------------------------------------------------------------------------------------
// Lexing
// ---------------------------------------------------------------------------------------------

/// Splits a config line into a lowercase keyword and its arguments, following OpenSSH's
/// rules: `keyword arg...` or `keyword=arg...`, double or single quotes, backslash escapes, and
/// comments starting at an unquoted `#` token.
fn split_line(raw: &str) -> Result<Option<(String, Vec<String>)>, &'static str> {
    let s = raw.trim_matches(|c: char| c.is_whitespace());
    if s.is_empty() || s.starts_with('#') {
        return Ok(None);
    }
    let end = s
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(s.len());
    let (kw, rest) = s.split_at_checked(end).ok_or("invalid line")?;
    let rest = rest.trim_start_matches([' ', '\t']);
    let rest = rest.strip_prefix('=').unwrap_or(rest);
    Ok(Some((kw.to_ascii_lowercase(), split_args(rest)?)))
}

fn split_args(s: &str) -> Result<Vec<String>, &'static str> {
    let mut args = Vec::new();
    let mut chars = s.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let Some(&first) = chars.peek() else { break };
        if first == '#' {
            break;
        }
        let mut arg = String::new();
        let mut quote: Option<char> = None;
        while let Some(&c) = chars.peek() {
            match (quote, c) {
                (None, c) if c.is_whitespace() => break,
                (None, '"' | '\'') => {
                    quote = Some(c);
                    chars.next();
                }
                (Some(q), c) if c == q => {
                    quote = None;
                    chars.next();
                }
                (_, '\\') => {
                    chars.next();
                    match chars.peek() {
                        Some(&n) if n == '\\' || n == '"' || n == '\'' || n == ' ' => {
                            arg.push(n);
                            chars.next();
                        }
                        _ => arg.push('\\'),
                    }
                }
                _ => {
                    arg.push(c);
                    chars.next();
                }
            }
        }
        if quote.is_some() {
            return Err("unterminated quote");
        }
        args.push(arg);
    }
    Ok(args)
}

// ---------------------------------------------------------------------------------------------
// Parsing into blocks
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum BlockKind {
    /// Options before the first `Host`/`Match` line.
    Global,
    /// `Host` block; patterns are lowercase.
    Host(Vec<String>),
    /// `Match all`.
    MatchAll,
    /// Any other `Match` (ignored) or an invalid `Host` line.
    Ignored,
}

#[derive(Debug, Clone)]
struct Block {
    kind: BlockKind,
    /// Host patterns of enclosing blocks at the time this block's file was included. OpenSSH
    /// only reads an `Include` for hosts that match the enclosing block.
    guards: Vec<Vec<String>>,
    line: usize,
    settings: Vec<Setting>,
}

impl Block {
    fn is_star(&self) -> bool {
        let own = match &self.kind {
            BlockKind::Global | BlockKind::MatchAll => true,
            BlockKind::Host(p) => is_star_patterns(p),
            BlockKind::Ignored => false,
        };
        own && self.guards.iter().all(|g| is_star_patterns(g))
    }

    fn applies_to(&self, alias: &str) -> bool {
        let own = match &self.kind {
            BlockKind::Global | BlockKind::MatchAll => true,
            BlockKind::Host(p) => patterns_match(p, alias),
            BlockKind::Ignored => false,
        };
        own && self.guards.iter().all(|g| patterns_match(g, alias))
    }

    fn has_wildcards(&self) -> bool {
        match &self.kind {
            BlockKind::Host(p) => p.iter().any(|p| !is_concrete(p)),
            _ => true,
        }
    }
}

fn is_star_patterns(p: &[String]) -> bool {
    !p.is_empty() && p.iter().all(|p| p == "*")
}

fn is_concrete(p: &str) -> bool {
    !p.is_empty() && !p.starts_with('!') && !p.contains(['*', '?'])
}

/// `alias` (lowercase) matches if any positive pattern matches and no negated one does.
fn patterns_match(patterns: &[String], alias: &str) -> bool {
    let mut matched = false;
    for p in patterns {
        if let Some(neg) = p.strip_prefix('!') {
            if glob_match(neg, alias) {
                return false;
            }
        } else if !matched && glob_match(p, alias) {
            matched = true;
        }
    }
    matched
}

#[derive(Debug, Clone)]
struct ForwardSpec {
    kind: ForwardKind,
    bind_address: String,
    bind_port: u16,
    dest: Option<(String, u16)>,
}

#[derive(Debug, Clone)]
enum Setting {
    HostName(String),
    User(String),
    Port(u16),
    IdentityFile(String),
    /// Empty for `ProxyJump none`.
    ProxyJump(Vec<String>),
    /// A (non-`none`) `ProxyCommand`; mutually exclusive with `ProxyJump`.
    ProxyCommand,
    ForwardAgent(bool),
    ForwardX11(bool),
    Forward(ForwardSpec),
    SetEnv(Vec<EnvVar>),
    KeepAlive(u32),
    Compression(bool),
    ConnectTimeout(u32),
    StrictHostKeyChecking(HostKeyPolicy),
}

struct Parser<'r> {
    blocks: Vec<Block>,
    resolver: &'r dyn Fn(&str) -> Option<String>,
    total_bytes: usize,
    includes: usize,
    unknown: HashSet<String>,
    limit_warned: bool,
}

/// Context for warnings: line number and the include file (if not the main file).
#[derive(Clone, Copy)]
struct Loc<'a> {
    line: usize,
    source: Option<&'a str>,
}

impl Loc<'_> {
    fn warn(self, b: &mut ImportBundle, kind: WarningKind, msg: impl Into<String>) {
        b.warn(kind, Some(self.line), self.source, msg);
    }
}

impl Parser<'_> {
    /// Parses `text`; options before its first `Host`/`Match` line go into `block`.
    fn parse_text(
        &mut self,
        b: &mut ImportBundle,
        text: &str,
        source: Option<&str>,
        depth: usize,
        block: usize,
    ) {
        let guards = self
            .blocks
            .get(block)
            .map(|blk| {
                let mut g = blk.guards.clone();
                if let BlockKind::Host(p) = &blk.kind {
                    g.push(p.clone());
                }
                g
            })
            .unwrap_or_default();
        let mut cur = block;
        for (idx, raw) in text.lines().enumerate() {
            let loc = Loc {
                line: idx + 1,
                source,
            };
            let (kw, args) = match split_line(raw) {
                Ok(Some(v)) => v,
                Ok(None) => continue,
                Err(e) => {
                    loc.warn(b, WarningKind::Invalid, format!("line skipped: {e}"));
                    continue;
                }
            };
            match kw.as_str() {
                "host" | "match" => {
                    if self.blocks.len() >= MAX_BLOCKS {
                        if !self.limit_warned {
                            self.limit_warned = true;
                            loc.warn(
                                b,
                                WarningKind::Limit,
                                format!("more than {MAX_BLOCKS} blocks; the rest was ignored"),
                            );
                        }
                        return;
                    }
                    let kind = if kw == "host" {
                        host_block_kind(b, loc, &args)
                    } else if args.len() == 1
                        && args.first().is_some_and(|a| a.eq_ignore_ascii_case("all"))
                    {
                        BlockKind::MatchAll
                    } else {
                        loc.warn(
                            b,
                            WarningKind::Unsupported,
                            "Match block is not supported; its options were ignored",
                        );
                        BlockKind::Ignored
                    };
                    self.blocks.push(Block {
                        kind,
                        guards: guards.clone(),
                        line: loc.line,
                        settings: Vec::new(),
                    });
                    cur = self.blocks.len() - 1;
                }
                "include" => {
                    cur = self.include(b, loc, &args, depth, cur);
                }
                _ => {
                    if let Some(s) = self.setting(b, loc, &kw, &args)
                        && let Some(blk) = self.blocks.get_mut(cur)
                    {
                        blk.settings.push(s);
                    }
                }
            }
        }
    }

    /// Processes an `Include` line inside block `cur`; returns the block that subsequent
    /// lines belong to.
    fn include(
        &mut self,
        b: &mut ImportBundle,
        loc: Loc<'_>,
        args: &[String],
        depth: usize,
        cur: usize,
    ) -> usize {
        if args.is_empty() {
            loc.warn(b, WarningKind::Invalid, "Include without a path");
            return cur;
        }
        if self
            .blocks
            .get(cur)
            .is_some_and(|blk| matches!(blk.kind, BlockKind::Ignored))
        {
            loc.warn(
                b,
                WarningKind::Unsupported,
                "Include inside a Match block was not followed",
            );
            return cur;
        }
        let mut cur = cur;
        for path in args {
            if depth + 1 > MAX_INCLUDE_DEPTH {
                loc.warn(
                    b,
                    WarningKind::Include,
                    format!(
                        "Include {path:?} nested deeper than {MAX_INCLUDE_DEPTH} levels; skipped"
                    ),
                );
                continue;
            }
            if self.includes >= MAX_INCLUDES {
                loc.warn(
                    b,
                    WarningKind::Limit,
                    format!("more than {MAX_INCLUDES} includes; {path:?} skipped"),
                );
                continue;
            }
            self.includes += 1;
            let Some(content) = (self.resolver)(path) else {
                loc.warn(
                    b,
                    WarningKind::Include,
                    format!("Include {path:?} could not be resolved"),
                );
                continue;
            };
            self.total_bytes = self.total_bytes.saturating_add(content.len());
            if self.total_bytes > MAX_INPUT_BYTES {
                loc.warn(
                    b,
                    WarningKind::Limit,
                    format!("Include {path:?} exceeds the total size limit; skipped"),
                );
                continue;
            }
            let before = self.blocks.len();
            self.parse_text(b, &content, Some(path), depth + 1, cur);
            // OpenSSH restores the enclosing block after an Include. If the included text
            // opened new blocks, continue the enclosing block in a fresh block so the file
            // order of options (which decides "first value wins") is preserved.
            if self.blocks.len() > before
                && self.blocks.len() < MAX_BLOCKS
                && let Some(enclosing) = self.blocks.get(cur)
            {
                let mut cont = enclosing.clone();
                cont.settings.clear();
                self.blocks.push(cont);
                cur = self.blocks.len() - 1;
            }
        }
        cur
    }

    fn setting(
        &mut self,
        b: &mut ImportBundle,
        loc: Loc<'_>,
        kw: &str,
        args: &[String],
    ) -> Option<Setting> {
        let first = args.first().map(String::as_str);
        let Some(value) = first else {
            loc.warn(b, WarningKind::Invalid, format!("{kw}: missing value"));
            return None;
        };
        let setting = match kw {
            "hostname" => Some(Setting::HostName(value.to_owned())),
            "user" => Some(Setting::User(value.to_owned())),
            "port" => parse_port(value).map(Setting::Port),
            "identityfile" => {
                if value.eq_ignore_ascii_case("none") {
                    return None;
                }
                Some(Setting::IdentityFile(value.to_owned()))
            }
            "proxyjump" => {
                if value.eq_ignore_ascii_case("none") {
                    Some(Setting::ProxyJump(Vec::new()))
                } else {
                    Some(Setting::ProxyJump(
                        value
                            .split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                            .collect(),
                    ))
                }
            }
            "proxycommand" => {
                if value.eq_ignore_ascii_case("none") {
                    Some(Setting::ProxyJump(Vec::new()))
                } else {
                    loc.warn(
                        b,
                        WarningKind::Unsupported,
                        "ProxyCommand is not supported; use ProxyJump or a proxy instead",
                    );
                    Some(Setting::ProxyCommand)
                }
            }
            "forwardagent" => {
                let v = yes_no(value);
                if v.is_none() {
                    loc.warn(
                        b,
                        WarningKind::Unsupported,
                        "ForwardAgent with a custom agent socket is not supported; \
                         forwarding the default agent instead",
                    );
                }
                Some(Setting::ForwardAgent(v.unwrap_or(true)))
            }
            "forwardx11" => yes_no(value).map(Setting::ForwardX11),
            "localforward" | "remoteforward" | "dynamicforward" => {
                let spec = parse_forward(kw, args);
                if let Err(e) = &spec {
                    loc.warn(b, WarningKind::Unsupported, format!("{kw} skipped: {e}"));
                }
                return spec.ok().map(Setting::Forward);
            }
            "sendenv" => {
                loc.warn(
                    b,
                    WarningKind::Unsupported,
                    "SendEnv forwards local variables and was not imported; \
                     use SetEnv-style variables instead",
                );
                return None;
            }
            "setenv" => {
                let mut vars = Vec::new();
                for a in args {
                    match a.split_once('=') {
                        Some((n, v))
                            if !n.is_empty()
                                && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') =>
                        {
                            vars.push(EnvVar {
                                name: n.to_owned(),
                                value: v.to_owned(),
                            });
                        }
                        _ => loc.warn(
                            b,
                            WarningKind::Invalid,
                            "SetEnv: invalid NAME=value pair; ignored",
                        ),
                    }
                }
                return Some(Setting::SetEnv(vars));
            }
            "serveraliveinterval" => value.parse::<u32>().ok().map(Setting::KeepAlive),
            "compression" => yes_no(value).map(Setting::Compression),
            "connecttimeout" => value.parse::<u32>().ok().map(Setting::ConnectTimeout),
            "stricthostkeychecking" => {
                let policy = match value.to_ascii_lowercase().as_str() {
                    "yes" | "true" => HostKeyPolicy::Strict,
                    "ask" => HostKeyPolicy::Ask,
                    "accept-new" => {
                        loc.warn(
                            b,
                            WarningKind::Unsupported,
                            "StrictHostKeyChecking accept-new is imported as ask",
                        );
                        HostKeyPolicy::Ask
                    }
                    "no" | "off" | "false" => {
                        loc.warn(
                            b,
                            WarningKind::Unsupported,
                            "disabling host key checking is not supported; imported as ask",
                        );
                        HostKeyPolicy::Ask
                    }
                    _ => return invalid(b, loc, kw, value),
                };
                Some(Setting::StrictHostKeyChecking(policy))
            }
            other => {
                if self.unknown.insert(other.to_owned()) {
                    let shown: String = other.chars().take(64).collect();
                    loc.warn(
                        b,
                        WarningKind::Unsupported,
                        format!("option {shown:?} is not supported and was ignored"),
                    );
                }
                return None;
            }
        };
        if setting.is_none() {
            return invalid(b, loc, kw, value);
        }
        setting
    }
}

/// Warns about an invalid option value; returns `None` for convenience.
fn invalid(b: &mut ImportBundle, loc: Loc<'_>, kw: &str, value: &str) -> Option<Setting> {
    let shown: String = value.chars().take(64).collect();
    loc.warn(
        b,
        WarningKind::Invalid,
        format!("{kw}: invalid value {shown:?}; ignored"),
    );
    None
}

fn host_block_kind(b: &mut ImportBundle, loc: Loc<'_>, args: &[String]) -> BlockKind {
    let patterns: Vec<String> = args
        .iter()
        .filter(|p| !p.is_empty())
        .map(|p| p.to_ascii_lowercase())
        .collect();
    if patterns.is_empty() {
        loc.warn(
            b,
            WarningKind::Invalid,
            "Host line without patterns; block ignored",
        );
        return BlockKind::Ignored;
    }
    if patterns.iter().any(|p| p.len() > MAX_PATTERN_LEN) {
        loc.warn(
            b,
            WarningKind::Limit,
            "Host pattern too long; block ignored",
        );
        return BlockKind::Ignored;
    }
    BlockKind::Host(patterns)
}

fn yes_no(v: &str) -> Option<bool> {
    match v.to_ascii_lowercase().as_str() {
        "yes" | "true" => Some(true),
        "no" | "false" => Some(false),
        _ => None,
    }
}

/// Parses a listen specification `[bind_address:]port` (also `[v6]:port` and `addr/port`).
fn parse_listen(s: &str) -> Result<(String, u16), &'static str> {
    if s.starts_with('/') || s.starts_with('~') {
        return Err("Unix domain socket forwarding is not supported");
    }
    if let Some(p) = parse_port(s) {
        return Ok(("localhost".to_owned(), p));
    }
    let (addr, port) = split_addr_port(s).ok_or("invalid listen address")?;
    let addr = match addr {
        "" | "*" => "0.0.0.0",
        a => a,
    };
    Ok((addr.to_owned(), port))
}

/// Splits `addr:port`, `[addr]:port` or `addr/port`; `addr` may be empty.
fn split_addr_port(s: &str) -> Option<(&str, u16)> {
    if let Some(rest) = s.strip_prefix('[') {
        let (h, p) = rest.split_once("]:")?;
        return Some((h, parse_port(p)?));
    }
    let (h, p) = if s.contains('/') {
        s.rsplit_once('/')?
    } else {
        s.rsplit_once(':')?
    };
    Some((h, parse_port(p)?))
}

fn parse_connect(s: &str) -> Result<(String, u16), &'static str> {
    if s.starts_with('/') || s.starts_with('~') {
        return Err("Unix domain socket forwarding is not supported");
    }
    match split_addr_port(s) {
        Some((h, p)) if !h.is_empty() => Ok((h.to_owned(), p)),
        _ => Err("invalid destination"),
    }
}

fn parse_forward(kw: &str, args: &[String]) -> Result<ForwardSpec, &'static str> {
    let listen = args.first().ok_or("missing listen port")?;
    let (bind_address, bind_port) = parse_listen(listen)?;
    let kind = match kw {
        "localforward" => ForwardKind::Local,
        "remoteforward" => ForwardKind::Remote,
        _ => ForwardKind::Dynamic,
    };
    let dest = match (kind, args.get(1)) {
        (ForwardKind::Dynamic, _) => None,
        (ForwardKind::Remote, None) => {
            return Err("remote dynamic (SOCKS) forwarding is not supported");
        }
        (_, None) => return Err("missing destination"),
        (_, Some(d)) => Some(parse_connect(d)?),
    };
    Ok(ForwardSpec {
        kind,
        bind_address,
        bind_port,
        dest,
    })
}

// ---------------------------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------------------------

/// A value and whether it came from a block that matches every host.
type Slot<T> = Option<(T, bool)>;

#[derive(Debug, Default)]
struct Effective {
    hostname: Slot<String>,
    user: Slot<String>,
    port: Slot<u16>,
    identity_files: Vec<(String, bool)>,
    proxy_jump: Slot<Vec<String>>,
    forward_agent: Slot<bool>,
    x11: Slot<bool>,
    forwards: Vec<ForwardSpec>,
    env: Vec<(EnvVar, bool)>,
    keepalive: Slot<u32>,
    compression: Slot<bool>,
    connect_timeout: Slot<u32>,
    strict: Slot<HostKeyPolicy>,
}

fn first<T: Clone>(slot: &mut Slot<T>, v: &T, star: bool) {
    if slot.is_none() {
        *slot = Some((v.clone(), star));
    }
}

impl Effective {
    fn apply(&mut self, block: &Block) {
        let star = block.is_star();
        for s in &block.settings {
            match s {
                Setting::HostName(v) => first(&mut self.hostname, v, star),
                Setting::User(v) => first(&mut self.user, v, star),
                Setting::Port(v) => first(&mut self.port, v, star),
                Setting::IdentityFile(v) => {
                    if !self.identity_files.iter().any(|(p, _)| p == v) {
                        self.identity_files.push((v.clone(), star));
                    }
                }
                Setting::ProxyJump(v) => first(&mut self.proxy_jump, v, star),
                Setting::ProxyCommand => first(&mut self.proxy_jump, &Vec::new(), star),
                Setting::ForwardAgent(v) => first(&mut self.forward_agent, v, star),
                Setting::ForwardX11(v) => first(&mut self.x11, v, star),
                Setting::Forward(f) => self.forwards.push(f.clone()),
                Setting::SetEnv(vars) => {
                    for v in vars {
                        if !self.env.iter().any(|(e, _)| e.name == v.name) {
                            self.env.push((v.clone(), star));
                        }
                    }
                }
                Setting::KeepAlive(v) => first(&mut self.keepalive, v, star),
                Setting::Compression(v) => first(&mut self.compression, v, star),
                Setting::ConnectTimeout(v) => first(&mut self.connect_timeout, v, star),
                Setting::StrictHostKeyChecking(v) => first(&mut self.strict, v, star),
            }
        }
    }

    /// Builds the sparse configuration. For hosts (`group == false`), values that come from
    /// match-everything blocks are left unset because the host inherits them from the
    /// defaults group.
    fn to_config(&self, b: &mut ImportBundle, group: bool) -> HostConfig {
        fn take<T: Clone>(slot: Option<&(T, bool)>, group: bool) -> Option<T> {
            slot.filter(|(_, star)| group || !star)
                .map(|(v, _)| v.clone())
        }
        let mut c = HostConfig::default();
        c.ssh.port = take(self.port.as_ref(), group);
        c.ssh.agent_forwarding = take(self.forward_agent.as_ref(), group);
        c.ssh.x11_forwarding = take(self.x11.as_ref(), group);
        c.ssh.keepalive_secs = take(self.keepalive.as_ref(), group);
        c.ssh.compression = take(self.compression.as_ref(), group);
        c.ssh.connect_timeout_secs = take(self.connect_timeout.as_ref(), group);
        c.ssh.host_key_policy = take(self.strict.as_ref(), group);
        if group || self.env.iter().any(|(_, star)| !star) {
            let env: Vec<EnvVar> = self.env.iter().map(|(e, _)| e.clone()).collect();
            if !env.is_empty() {
                c.ssh.env = Some(env);
            }
        }
        // A credential is replaced as a whole by inheritance, so the host gets the complete
        // credential as soon as any part of it is host specific.
        let specific = self.user.as_ref().is_some_and(|(_, star)| !star)
            || self.identity_files.iter().any(|(_, star)| !star);
        if group || specific {
            let keys: Vec<KeyId> = self
                .identity_files
                .iter()
                .map(|(p, _)| b.key_for_path(p, KeyFileFormat::OpenSsh))
                .collect();
            let username = self.user.as_ref().map(|(u, _)| u.clone());
            if username.is_some() || !keys.is_empty() {
                c.ssh.credential = Some(Credential::Inline {
                    username,
                    password: None,
                    keys,
                });
            }
        }
        c
    }
}

/// Expands the `%h` and `%%` tokens of `HostName`.
fn expand_hostname(template: &str, alias: &str) -> Result<String, char> {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            match chars.next() {
                Some('h') => out.push_str(alias),
                Some('%') => out.push('%'),
                Some(other) => return Err(other),
                None => return Err('%'),
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

/// A parsed `ProxyJump` hop: `[user@]host[:port]` or `ssh://[user@]host[:port]`.
fn parse_hop(s: &str) -> Option<(Option<String>, String, Option<u16>)> {
    let s = s
        .strip_prefix("ssh://")
        .map_or(s, |r| r.trim_end_matches('/'));
    let (user, hostport) = match s.rsplit_once('@') {
        Some((u, h)) if !u.is_empty() => (Some(u.to_owned()), h),
        Some(_) => return None,
        None => (None, s),
    };
    let (host, port) = split_host_port(hostport)?;
    Some((user, host.to_owned(), port))
}

#[derive(Debug, Clone)]
enum ChainState {
    InProgress,
    Done(Vec<HostId>),
}

type HostKey = (String, Option<String>, Option<u16>);

struct Builder<'a> {
    blocks: &'a [Block],
    /// Blocks without wildcards, indexed by concrete pattern.
    by_alias: HashMap<&'a str, Vec<usize>>,
    /// Blocks with wildcards (always evaluated).
    wildcard: Vec<usize>,
    memo: HashMap<HostKey, usize>,
    hosts: Vec<Host>,
    /// The (lowercase) alias each host in `hosts` was built for.
    aliases: Vec<String>,
    chains: Vec<ChainState>,
    limit_warned: bool,
}

impl<'a> Builder<'a> {
    fn new(b: &mut ImportBundle, blocks: &'a [Block]) -> Self {
        let mut by_alias: HashMap<&'a str, Vec<usize>> = HashMap::new();
        let mut wildcard = Vec::new();
        for (i, blk) in blocks.iter().enumerate() {
            if matches!(blk.kind, BlockKind::Ignored) {
                continue;
            }
            if blk.has_wildcards() {
                if wildcard.len() < MAX_WILDCARD_BLOCKS {
                    wildcard.push(i);
                } else if !blk.settings.is_empty() {
                    b.warn(
                        WarningKind::Limit,
                        Some(blk.line),
                        None,
                        format!("more than {MAX_WILDCARD_BLOCKS} wildcard blocks; ignored"),
                    );
                }
            } else if let BlockKind::Host(patterns) = &blk.kind {
                for p in patterns {
                    let list = by_alias.entry(p.as_str()).or_default();
                    if list.last() != Some(&i) {
                        list.push(i);
                    }
                }
            }
        }
        Self {
            blocks,
            by_alias,
            wildcard,
            memo: HashMap::new(),
            hosts: Vec::new(),
            aliases: Vec::new(),
            chains: Vec::new(),
            limit_warned: false,
        }
    }

    fn effective(&self, alias: &str) -> Effective {
        let mut idx: Vec<usize> = self
            .wildcard
            .iter()
            .copied()
            .filter(|i| self.blocks.get(*i).is_some_and(|blk| blk.applies_to(alias)))
            .collect();
        if let Some(list) = self.by_alias.get(alias) {
            idx.extend(
                list.iter()
                    .copied()
                    .filter(|i| self.blocks.get(*i).is_some_and(|blk| blk.applies_to(alias))),
            );
        }
        idx.sort_unstable();
        idx.dedup();
        let mut eff = Effective::default();
        for i in idx {
            if let Some(blk) = self.blocks.get(i) {
                eff.apply(blk);
            }
        }
        eff
    }

    /// Builds (or returns the already built) host for `alias` with optional user/port
    /// overrides from a `ProxyJump` hop. Returns the index into `self.hosts`.
    fn host(
        &mut self,
        b: &mut ImportBundle,
        alias: &str,
        label: &str,
        user: Option<String>,
        port: Option<u16>,
        depth: usize,
    ) -> Option<usize> {
        let alias = alias.to_ascii_lowercase();
        let key = (alias.clone(), user.clone(), port);
        if let Some(i) = self.memo.get(&key) {
            return Some(*i);
        }
        if self.hosts.len() >= MAX_HOSTS {
            if !self.limit_warned {
                self.limit_warned = true;
                b.warn(
                    WarningKind::Limit,
                    None,
                    None,
                    format!("more than {MAX_HOSTS} hosts; the rest was ignored"),
                );
            }
            return None;
        }
        let eff = self.effective(&alias);
        let mut config = eff.to_config(b, false);
        if let Some(p) = port {
            config.ssh.port = Some(p);
        }
        if let Some(u) = user {
            let keys = match config.ssh.credential.take() {
                Some(Credential::Inline { keys, .. }) => keys,
                _ => eff
                    .identity_files
                    .iter()
                    .map(|(p, _)| b.key_for_path(p, KeyFileFormat::OpenSsh))
                    .collect(),
            };
            config.ssh.credential = Some(Credential::Inline {
                username: Some(u),
                password: None,
                keys,
            });
        }
        let address = match &eff.hostname {
            Some((tpl, _)) => expand_hostname(tpl, &alias).unwrap_or_else(|tok| {
                b.warn(
                    WarningKind::Unsupported,
                    None,
                    Some(label),
                    format!("HostName token %{tok} is not supported"),
                );
                tpl.clone()
            }),
            None => alias.clone(),
        };
        let mut host = Host::new(label, address);
        host.config = config;
        let id = host.id;
        for f in &eff.forwards {
            let mut rule = PortForwardRule {
                id: RuleId::new(),
                label: String::new(),
                kind: f.kind,
                host: id,
                bind_address: f.bind_address.clone(),
                bind_port: f.bind_port,
                dest_host: f.dest.as_ref().map(|(h, _)| h.clone()),
                dest_port: f.dest.as_ref().map(|(_, p)| *p),
                auto_start: false,
                created_at: 0,
                updated_at: 0,
            };
            rule.label = format!("{label} {}", rule.openssh_flag());
            b.forwards.push(rule);
        }
        let idx = self.hosts.len();
        self.hosts.push(host);
        self.aliases.push(alias);
        self.chains.push(ChainState::InProgress);
        self.memo.insert(key, idx);

        let hops = eff.proxy_jump.map(|(h, _)| h).unwrap_or_default();
        let chain = self.chain(b, label, &hops, depth);
        if let Some(h) = self.hosts.get_mut(idx)
            && !chain.is_empty()
        {
            h.config.ssh.jump_chain = Some(chain.clone());
        }
        if let Some(c) = self.chains.get_mut(idx) {
            *c = ChainState::Done(chain);
        }
        Some(idx)
    }

    fn chain(
        &mut self,
        b: &mut ImportBundle,
        label: &str,
        hops: &[String],
        depth: usize,
    ) -> Vec<HostId> {
        if hops.is_empty() {
            return Vec::new();
        }
        if depth >= MAX_JUMP_DEPTH {
            b.warn(
                WarningKind::Limit,
                None,
                Some(label),
                format!("ProxyJump chain nested deeper than {MAX_JUMP_DEPTH}; removed"),
            );
            return Vec::new();
        }
        let mut prefix = Vec::new();
        let mut ids = Vec::new();
        for (n, hop) in hops.iter().enumerate() {
            let Some((user, host, port)) = parse_hop(hop) else {
                b.warn(
                    WarningKind::Invalid,
                    None,
                    Some(label),
                    format!("invalid ProxyJump hop {hop:?}; jump chain removed"),
                );
                return Vec::new();
            };
            let Some(i) = self.host(b, &host, hop, user, port, depth + 1) else {
                return Vec::new();
            };
            if n == 0 {
                let Some(ChainState::Done(c)) = self.chains.get(i) else {
                    b.warn(
                        WarningKind::Invalid,
                        None,
                        Some(label),
                        "ProxyJump hosts jump through each other in a cycle; jump chain removed",
                    );
                    return Vec::new();
                };
                prefix.clone_from(c);
            }
            if let Some(h) = self.hosts.get(i) {
                ids.push(h.id);
            }
        }
        prefix.extend(ids);
        prefix
    }
}

fn build(b: &mut ImportBundle, blocks: &[Block]) {
    let mut builder = Builder::new(b, blocks);

    // Concrete aliases in order of first appearance.
    let mut seen = HashSet::new();
    let mut aliases: Vec<&str> = Vec::new();
    for blk in blocks {
        if let BlockKind::Host(patterns) = &blk.kind {
            for p in patterns {
                if is_concrete(p) && blk.applies_to(p) && seen.insert(p.as_str()) {
                    aliases.push(p);
                }
            }
        }
    }
    for alias in aliases {
        builder.host(b, alias, alias, None, None, 0);
    }

    // Wildcard blocks that matched nothing were not imported.
    for blk in blocks {
        if let BlockKind::Host(p) = &blk.kind
            && blk.has_wildcards()
            && !blk.is_star()
            && !blk.settings.is_empty()
            && !builder.aliases.iter().any(|a| blk.applies_to(a))
        {
            b.warn(
                WarningKind::Unsupported,
                Some(blk.line),
                None,
                format!(
                    "Host {} matched no imported host; its options were not imported",
                    p.join(" ").chars().take(120).collect::<String>()
                ),
            );
        }
    }

    // Defaults from match-everything blocks.
    let mut defaults = Effective::default();
    for blk in blocks.iter().filter(|blk| blk.is_star()) {
        defaults.apply(blk);
    }
    let config = defaults.to_config(b, true);
    let mut hosts = builder.hosts;
    if !config.is_empty_config() {
        let mut g = Group::new(DEFAULTS_GROUP_LABEL);
        g.config = config;
        let gid: GroupId = g.id;
        b.groups.push(g);
        for h in &mut hosts {
            h.group = Some(gid);
        }
    }
    b.hosts.extend(hosts);
}

trait IsEmptyConfig {
    fn is_empty_config(&self) -> bool;
}

impl IsEmptyConfig for HostConfig {
    fn is_empty_config(&self) -> bool {
        self.ssh.is_empty()
            && self.mosh.is_empty()
            && self.telnet.is_empty()
            && self.terminal.is_empty()
            && self.sftp.is_empty()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use core_model::Credential;

    const FIXTURE: &str = r#"
# ~/.ssh/config
ServerAliveInterval 60

Host bastion
    HostName bastion.example.com
    User ops
    Port 2222

Host web1 web2
    HostName %h.internal.example.com
    ProxyJump bastion
    LocalForward 8080 localhost:80
    LocalForward [::1]:5433 db.internal:5432
    DynamicForward 1080
    RemoteForward 9000 127.0.0.1:3000
    SetEnv LANG=en_US.UTF-8 "EDITOR=vim -u NONE"
    ForwardAgent yes

Host *.prod
    StrictHostKeyChecking yes

Host db.prod
    User=postgres
    IdentityFile "~/.ssh/my key" # trailing comment
    ProxyJump admin@bastion:2200,web1

Match host foo exec "true"
    User nobody

Host legacy
    ProxyCommand ssh -W %h:%p jump
    SendEnv LANG LC_*
    ControlMaster auto
    ControlPath ~/.ssh/cm-%r@%h:%p

Host *
    User defaultuser
    IdentityFile ~/.ssh/id_ed25519
    Compression yes
"#;

    fn host<'a>(b: &'a ImportBundle, label: &str) -> &'a Host {
        b.hosts
            .iter()
            .find(|h| h.label == label)
            .unwrap_or_else(|| panic!("no host {label}: {:#?}", b.hosts))
    }

    fn user(h: &Host) -> Option<&str> {
        match &h.config.ssh.credential {
            Some(Credential::Inline { username, .. }) => username.as_deref(),
            _ => None,
        }
    }

    fn key_paths(b: &ImportBundle, cred: Option<&Credential>) -> Vec<String> {
        match cred {
            Some(Credential::Inline { keys, .. }) => keys
                .iter()
                .map(|id| {
                    b.keys
                        .iter()
                        .find(|k| k.id == *id)
                        .and_then(|k| k.source_path.clone())
                        .unwrap()
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn realistic_config() {
        let b = parse(FIXTURE).unwrap();
        let labels: Vec<&str> = b.hosts.iter().map(|h| h.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "bastion",
                "web1",
                "web2",
                "db.prod",
                "admin@bastion:2200",
                "legacy"
            ]
        );

        // Host * and global options form the defaults group.
        assert_eq!(b.groups.len(), 1);
        let g = &b.groups[0];
        assert_eq!(g.label, DEFAULTS_GROUP_LABEL);
        assert_eq!(g.config.ssh.keepalive_secs, Some(60));
        assert_eq!(g.config.ssh.compression, Some(true));
        assert_eq!(
            key_paths(&b, g.config.ssh.credential.as_ref()),
            ["~/.ssh/id_ed25519"]
        );
        assert!(b.hosts.iter().all(|h| h.group == Some(g.id)));

        let bastion = host(&b, "bastion");
        assert_eq!(bastion.address, "bastion.example.com");
        assert_eq!(bastion.config.ssh.port, Some(2222));
        assert_eq!(user(bastion), Some("ops"));
        // The credential is replaced as a whole, so the inherited key is carried along.
        assert_eq!(
            key_paths(&b, bastion.config.ssh.credential.as_ref()),
            ["~/.ssh/id_ed25519"]
        );
        assert_eq!(bastion.config.ssh.compression, None, "inherited from group");

        let web1 = host(&b, "web1");
        assert_eq!(web1.address, "web1.internal.example.com");
        assert_eq!(host(&b, "web2").address, "web2.internal.example.com");
        assert_eq!(web1.config.ssh.jump_chain, Some(vec![bastion.id]));
        assert_eq!(web1.config.ssh.agent_forwarding, Some(true));
        assert_eq!(
            web1.config.ssh.credential, None,
            "inherits the group credential"
        );
        let env = web1.config.ssh.env.as_ref().unwrap();
        assert_eq!(env.len(), 2);
        assert_eq!(env[1].name, "EDITOR");
        assert_eq!(env[1].value, "vim -u NONE");

        let fw: Vec<String> = b
            .forwards
            .iter()
            .filter(|f| f.host == web1.id)
            .map(PortForwardRule::openssh_flag)
            .collect();
        assert_eq!(
            fw,
            [
                "-L localhost:8080:localhost:80",
                "-L [::1]:5433:db.internal:5432",
                "-D localhost:1080",
                "-R localhost:9000:127.0.0.1:3000"
            ]
        );
        assert_eq!(b.forwards.len(), 8);

        let db = host(&b, "db.prod");
        assert_eq!(db.config.ssh.host_key_policy, Some(HostKeyPolicy::Strict));
        assert_eq!(user(db), Some("postgres"));
        assert_eq!(
            key_paths(&b, db.config.ssh.credential.as_ref()),
            ["~/.ssh/my key", "~/.ssh/id_ed25519"]
        );
        let admin = host(&b, "admin@bastion:2200");
        assert_eq!(admin.address, "bastion.example.com");
        assert_eq!(admin.config.ssh.port, Some(2200));
        assert_eq!(user(admin), Some("admin"));
        assert_eq!(db.config.ssh.jump_chain, Some(vec![admin.id, web1.id]));

        let legacy = host(&b, "legacy");
        assert_eq!(legacy.config.ssh.jump_chain, None);

        let msgs: Vec<String> = b.warnings.iter().map(ToString::to_string).collect();
        let has = |s: &str| msgs.iter().any(|m| m.contains(s));
        assert!(has("Match block"), "{msgs:#?}");
        assert!(has("ProxyCommand"));
        assert!(has("SendEnv"));
        assert!(has("\"controlmaster\""));
        assert!(has("\"controlpath\""));
        assert_eq!(b.keys.len(), 2);
        assert!(b.secrets.is_empty());
    }

    #[test]
    fn first_value_wins_including_host_star() {
        let b = parse(
            "Host *\n  Port 2200\nHost a\n  Port 22\n  User x\nHost a\n  User y\n  Port 23\n",
        )
        .unwrap();
        let a = host(&b, "a");
        // OpenSSH uses the first value obtained, which is Host *'s; it is inherited.
        assert_eq!(a.config.ssh.port, None);
        assert_eq!(b.groups[0].config.ssh.port, Some(2200));
        assert_eq!(user(a), Some("x"));
    }

    #[test]
    fn keyword_forms_and_quoting() {
        let b = parse(
            "HOST=a\nhostname = a.example.com\n\tPORT\t= 2022\n  user \"john doe\"\n\
             Host 'b'\n  HostName=b.example.com # comment\n  Port=bad\n  User \"unterminated\n",
        )
        .unwrap();
        let a = host(&b, "a");
        assert_eq!(a.address, "a.example.com");
        assert_eq!(a.config.ssh.port, Some(2022));
        assert_eq!(user(a), Some("john doe"));
        let bh = host(&b, "b");
        assert_eq!(bh.address, "b.example.com");
        assert_eq!(bh.config.ssh.port, None);
        assert!(b.warnings.iter().any(|w| w.line == Some(7)));
        assert!(b.warnings.iter().any(|w| w.line == Some(8)));
    }

    #[test]
    fn split_args_rules() {
        assert_eq!(
            split_args(r#"a "b c" 'd e' f\ g # x"#).unwrap(),
            ["a", "b c", "d e", "f g"]
        );
        assert_eq!(split_args(r#""a\"b""#).unwrap(), [r#"a"b"#]);
        assert_eq!(split_args(r"C:\keys\id").unwrap(), [r"C:\keys\id"]);
        assert!(split_args("\"open").is_err());
        assert_eq!(split_line("  # comment").unwrap(), None);
        assert_eq!(split_line("").unwrap(), None);
    }

    #[test]
    fn includes_are_resolved_in_place() {
        let main = "Include config.d/*\nHost main\n  HostName main.example.com\n  Include extra\n  Port 2201\n";
        let resolver = |p: &str| match p {
            "config.d/*" => Some("Host inc\n  HostName inc.example.com\n".to_owned()),
            "extra" => Some("User fromextra\nHost hidden\n  HostName hidden.example\n".to_owned()),
            _ => None,
        };
        assert_eq!(include_paths(main), ["config.d/*", "extra"]);
        let b = parse_with_includes(main, resolver).unwrap();
        let labels: Vec<&str> = b.hosts.iter().map(|h| h.label.as_str()).collect();
        // `hidden` is inside an include of `Host main`, so it can only match "main".
        assert_eq!(labels, ["inc", "main"]);
        let m = host(&b, "main");
        assert_eq!(user(m), Some("fromextra"));
        assert_eq!(
            m.config.ssh.port,
            Some(2201),
            "options after Include still apply"
        );
        assert!(b.warnings.is_empty(), "{:?}", b.warnings);
    }

    #[test]
    fn include_depth_and_unresolved() {
        let b = parse_with_includes("Include self\nHost a\n", |_| {
            Some("Include self\n".to_owned())
        })
        .unwrap();
        assert!(b.warnings.iter().any(|w| w.kind == WarningKind::Include));
        assert_eq!(b.hosts.len(), 1);
        let b = parse("Include ~/.ssh/other\nHost a\n").unwrap();
        assert_eq!(b.warnings.len(), 1);
        assert_eq!(b.warnings[0].kind, WarningKind::Include);
    }

    #[test]
    fn include_total_size_is_bounded() {
        let big = "#".repeat(MAX_INPUT_BYTES / 2 + 1);
        let b = parse_with_includes("Include a\nInclude b\nInclude c\nHost x\n", |_| {
            Some(big.clone())
        })
        .unwrap();
        assert!(b.warnings.iter().any(|w| w.kind == WarningKind::Limit));
    }

    #[test]
    fn jump_cycles_and_self_jumps() {
        let b = parse(
            "Host a\n  ProxyJump b\nHost b\n  ProxyJump a\n\
             Host bastion\nHost *\n  ProxyJump bastion\n",
        )
        .unwrap();
        assert_eq!(b.hosts.len(), 3);
        assert!(b.warnings.iter().any(|w| w.message.contains("cycle")));
        let bastion = host(&b, "bastion");
        assert_eq!(bastion.config.ssh.jump_chain, None);
    }

    #[test]
    fn hop_hosts_use_their_own_matching_settings() {
        let b = parse("Host *.corp\n  User corpuser\nHost app\n  ProxyJump ssh://gw.corp:2022/\n")
            .unwrap();
        let gw = host(&b, "ssh://gw.corp:2022/");
        assert_eq!(gw.address, "gw.corp");
        assert_eq!(gw.config.ssh.port, Some(2022));
        assert_eq!(user(gw), Some("corpuser"));
        assert_eq!(host(&b, "app").config.ssh.jump_chain, Some(vec![gw.id]));
    }

    #[test]
    fn unmatched_wildcards_duplicates_and_bad_values() {
        let b = parse(
            "Host *.nowhere\n  User x\nHost a b\n  HostName same.example\n\
             Host c\n  HostName %r.example\n\
             Host d\n  LocalForward /tmp/sock host:1\n  RemoteForward 8000\n  LocalForward 1 bad\n\
             Host *\n",
        )
        .unwrap();
        let labels: Vec<&str> = b.hosts.iter().map(|h| h.label.as_str()).collect();
        assert_eq!(labels, ["a", "d"]);
        let kinds: Vec<WarningKind> = b.warnings.iter().map(|w| w.kind).collect();
        assert!(kinds.contains(&WarningKind::Duplicate));
        assert!(b.warnings.iter().any(|w| w.message.contains("matched no")));
        assert!(b.warnings.iter().any(|w| w.message.contains("%r")));
        assert!(b.warnings.iter().any(|w| w.message.contains("Unix domain")));
        assert!(b.warnings.iter().any(|w| w.message.contains("SOCKS")));
        assert!(b.forwards.is_empty());
        assert!(b.groups.is_empty(), "empty Host * creates no group");
    }

    #[test]
    fn strict_host_key_variants() {
        let b = parse(
            "Host a\n StrictHostKeyChecking no\nHost b\n StrictHostKeyChecking accept-new\n\
             Host c\n StrictHostKeyChecking maybe\n",
        )
        .unwrap();
        assert_eq!(
            host(&b, "a").config.ssh.host_key_policy,
            Some(HostKeyPolicy::Ask)
        );
        assert_eq!(
            host(&b, "b").config.ssh.host_key_policy,
            Some(HostKeyPolicy::Ask)
        );
        assert_eq!(host(&b, "c").config.ssh.host_key_policy, None);
        assert_eq!(b.warnings.len(), 3);
    }

    #[test]
    fn listen_and_connect_specs() {
        assert_eq!(parse_listen("8080").unwrap(), ("localhost".into(), 8080));
        assert_eq!(parse_listen("*:8080").unwrap(), ("0.0.0.0".into(), 8080));
        assert_eq!(parse_listen(":8080").unwrap(), ("0.0.0.0".into(), 8080));
        assert_eq!(parse_listen("[::1]:80").unwrap(), ("::1".into(), 80));
        assert_eq!(parse_listen("::1/80").unwrap(), ("::1".into(), 80));
        assert!(parse_listen("0").is_err());
        assert_eq!(parse_connect("db:5432").unwrap(), ("db".into(), 5432));
        assert!(parse_connect(":5432").is_err());
        assert_eq!(
            parse_hop("u@h:2"),
            Some((Some("u".into()), "h".into(), Some(2)))
        );
        assert_eq!(parse_hop("@h"), None);
        assert_eq!(parse_hop("h:0"), None);
    }

    #[test]
    fn rejects_oversized_input() {
        let big = " ".repeat(MAX_INPUT_BYTES + 1);
        assert!(matches!(parse(&big), Err(ImportError::TooLarge { .. })));
    }
}
