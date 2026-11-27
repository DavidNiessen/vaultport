// SPDX-License-Identifier: GPL-3.0-or-later
//! PuTTY registry export (`.reg`) importer.
//!
//! Export with `reg export HKCU\Software\SimonTatham\PuTTY putty.reg`. The file is usually
//! UTF-16LE with a BOM; [`crate::import`] decodes it, [`parse`] takes the decoded text.
//!
//! * Each `...\PuTTY\Sessions\<name>` key becomes a host (the name is URL-decoded, `%20` →
//!   space). `Default Settings` is skipped. Only `ssh` and `telnet` sessions are imported.
//! * `PortForwardings` (`L8080=localhost:80,R2222=localhost:22,D1080`, optionally prefixed with
//!   `4`/`6`) become port forward rules.
//! * `PublicKeyFile` becomes a `.ppk` key placeholder.
//! * `ProxyMethod` SOCKS5 / HTTP become a proxy (with `ProxyPassword` as a secret); SSH proxying
//!   (PuTTY 0.77+) becomes a jump host, resolving `ProxyHost` to a saved session when it names
//!   one. Other proxy methods are reported as unsupported.
//! * `SshHostKeys` entries (`rsa2@22:host`, `ssh-ed25519@22:host`, `ecdsa-sha2-nistp256@...`,
//!   `dss@...`) are converted from PuTTY's numeric format to known hosts.

use crate::bundle::{ImportBundle, KeyCache, KeyFileFormat, SecretKind, WarningKind};
use crate::common::{HostDraft, Protocol, split_host_port};
use crate::error::ImportError;
use crate::finalize::finalize;
use crate::text::check_size;
use crate::wire::{encode_blob, put_mpint, put_string, sha256_fingerprint};
use core_model::{
    ForwardKind, HostId, KnownHost, KnownHostId, PortForwardRule, ProxyConfig, ProxyKind, RuleId,
};
use std::collections::HashMap;
use zeroize::Zeroizing;

const SESSIONS_KEY: &str = r"\software\simontatham\putty\sessions\";
const HOST_KEYS_KEY: &str = r"\software\simontatham\putty\sshhostkeys";
/// Longest accepted hex number in a host key (16384-bit RSA modulus).
const MAX_HEX_DIGITS: usize = 4096;

#[derive(Debug)]
enum Value {
    Str(String),
    Dword(u32),
    Other,
}

#[derive(Debug, Default)]
struct Session {
    line: usize,
    values: Vec<(String, Value)>,
}

impl Session {
    fn get(&self, name: &str) -> Option<&Value> {
        self.values
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v)
    }

    fn string(&self, name: &str) -> Option<&str> {
        match self.get(name) {
            Some(Value::Str(s)) if !s.trim().is_empty() => Some(s.trim()),
            _ => None,
        }
    }

    fn dword(&self, name: &str) -> Option<u32> {
        match self.get(name) {
            Some(Value::Dword(d)) => Some(*d),
            _ => None,
        }
    }
}

enum Section {
    None,
    Session(usize),
    HostKeys,
}

/// Parses a PuTTY `.reg` export (already decoded to text).
pub fn parse(input: &str) -> Result<ImportBundle, ImportError> {
    check_size(input.len())?;
    let mut lines = logical_lines(input);
    let header = lines.find(|(_, l)| !l.trim().is_empty()).map(|(_, l)| l);
    let header = header.as_deref().map(str::trim).unwrap_or_default();
    if !(header.starts_with("Windows Registry Editor Version") || header == "REGEDIT4") {
        return Err(ImportError::Malformed {
            format: "PuTTY registry",
            reason: "missing \"Windows Registry Editor\" header".into(),
        });
    }

    let mut b = ImportBundle::default();
    let mut names: Vec<String> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut sessions: Vec<Session> = Vec::new();
    let mut host_keys: Vec<(usize, String, String)> = Vec::new();
    let mut section = Section::None;
    for (line_no, line) in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if let Some(path) = line.strip_prefix('[') {
            let path = path.strip_suffix(']').unwrap_or(path);
            section = classify(path, &mut names, &mut index, &mut sessions, line_no);
            continue;
        }
        let Some((name, value)) = parse_value_line(line) else {
            if !matches!(section, Section::None) {
                b.warn(
                    WarningKind::Invalid,
                    Some(line_no),
                    None,
                    "malformed line skipped",
                );
            }
            continue;
        };
        match &section {
            Section::None => {}
            Section::Session(i) => {
                if let Some(s) = sessions.get_mut(*i) {
                    s.values.push((name, value));
                }
            }
            Section::HostKeys => {
                if let Value::Str(v) = value {
                    host_keys.push((line_no, name, v));
                }
            }
        }
    }

    build_sessions(&mut b, &names, &sessions);
    for (line, name, value) in host_keys {
        match convert_host_key(&name, &value) {
            Ok(kh) => b.known_hosts.push(kh),
            Err(e) => b.warn(
                WarningKind::Unsupported,
                Some(line),
                Some(&name),
                format!("host key not imported: {e}"),
            ),
        }
    }
    Ok(finalize(b))
}

/// Yields `(line_number, line)`, joining lines that end with a `\` continuation.
fn logical_lines(input: &str) -> impl Iterator<Item = (usize, String)> + '_ {
    let mut it = input.lines().enumerate();
    std::iter::from_fn(move || {
        let (idx, first) = it.next()?;
        let mut line = first.trim_end().to_owned();
        while line.ends_with('\\') && !line.trim_start().starts_with('[') {
            line.pop();
            match it.next() {
                Some((_, next)) => line.push_str(next.trim()),
                None => break,
            }
        }
        Some((idx + 1, line))
    })
}

fn classify(
    path: &str,
    names: &mut Vec<String>,
    index: &mut HashMap<String, usize>,
    sessions: &mut Vec<Session>,
    line: usize,
) -> Section {
    if path.starts_with('-') {
        return Section::None;
    }
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(HOST_KEYS_KEY) {
        return Section::HostKeys;
    }
    let Some(pos) = lower.find(SESSIONS_KEY) else {
        return Section::None;
    };
    // `find` on the ASCII-lowercased copy gives a byte offset valid for `path` as well.
    let Some(raw_name) = path.get(pos + SESSIONS_KEY.len()..) else {
        return Section::None;
    };
    if raw_name.is_empty() || raw_name.contains('\\') {
        return Section::None;
    }
    if let Some(i) = index.get(raw_name) {
        return Section::Session(*i);
    }
    index.insert(raw_name.to_owned(), names.len());
    names.push(raw_name.to_owned());
    sessions.push(Session {
        line,
        values: Vec::new(),
    });
    Section::Session(sessions.len() - 1)
}

/// Parses `"Name"=value`.
fn parse_value_line(line: &str) -> Option<(String, Value)> {
    let rest = line.strip_prefix('"')?;
    let (name, rest) = parse_quoted(rest)?;
    let value = rest.trim_start().strip_prefix('=')?.trim();
    let value = if let Some(q) = value.strip_prefix('"') {
        Value::Str(parse_quoted(q)?.0)
    } else if let Some(hex) = value.strip_prefix("dword:") {
        Value::Dword(u32::from_str_radix(hex.trim(), 16).ok()?)
    } else {
        Value::Other
    };
    Some((name, value))
}

/// Parses the body of a quoted registry string (after the opening quote); returns the
/// unescaped string and the text after the closing quote.
fn parse_quoted(s: &str) -> Option<(String, &str)> {
    let mut out = String::new();
    let mut chars = s.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some((_, n)) => out.push(n),
                None => return None,
            },
            '"' => return Some((out, s.get(i + 1..).unwrap_or_default())),
            c => out.push(c),
        }
    }
    None
}

/// Decodes PuTTY's `%XX` escaping of session names.
fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&c) = bytes.get(i) {
        if c == b'%'
            && let Some(hex) = bytes.get(i + 1..i + 3)
            && let Ok(hex) = std::str::from_utf8(hex)
            && let Ok(v) = u8::from_str_radix(hex, 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(c);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

struct PendingJump {
    host_index: usize,
    target: String,
    port: Option<u16>,
    user: Option<String>,
}

fn build_sessions(b: &mut ImportBundle, names: &[String], sessions: &[Session]) {
    let mut by_name: HashMap<String, HostId> = HashMap::new();
    let mut pending: Vec<PendingJump> = Vec::new();
    let mut keys = KeyCache::default();
    for (raw, s) in names.iter().zip(sessions) {
        let name = url_decode(raw);
        if name == "Default Settings" {
            continue;
        }
        let line = Some(s.line);
        let item = Some(name.as_str());
        let protocol = match s.string("Protocol").unwrap_or("ssh") {
            "ssh" => Protocol::Ssh,
            "telnet" => Protocol::Telnet,
            other => {
                b.warn(
                    WarningKind::Unsupported,
                    line,
                    item,
                    format!("{other} sessions are not supported; skipped"),
                );
                continue;
            }
        };
        let Some(host_name) = s.string("HostName") else {
            b.warn(
                WarningKind::Invalid,
                line,
                item,
                "session without host name skipped",
            );
            continue;
        };
        let (user_in_host, address) = match host_name.rsplit_once('@') {
            Some((u, h)) => (Some(u), h),
            None => (None, host_name),
        };
        let mut d = HostDraft::new(&name, address, protocol);
        d.port = s
            .dword("PortNumber")
            .and_then(|p| u16::try_from(p).ok())
            .filter(|p| *p != 0);
        d.username(s.string("UserName").or(user_in_host).unwrap_or_default());
        if protocol == Protocol::Ssh {
            if let Some(path) = s.string("PublicKeyFile") {
                d.keys.push(keys.key(b, path, KeyFileFormat::Ppk));
            }
            proxy(b, s, &mut d, &name, &mut pending);
        }
        let forwards = s.string("PortForwardings").map(str::to_owned);
        let host = d.build(b);
        let id = host.id;
        by_name.insert(name.clone(), id);
        if let Some(pf) = forwards {
            port_forwards(b, &pf, id, &name, s.line);
        }
        if let Some(p) = pending.last_mut()
            && p.host_index == usize::MAX
        {
            p.host_index = b.hosts.len();
        }
        b.hosts.push(host);
    }
    resolve_jumps(b, &by_name, pending);
}

fn proxy(
    b: &mut ImportBundle,
    s: &Session,
    d: &mut HostDraft,
    name: &str,
    pending: &mut Vec<PendingJump>,
) {
    let method = s.dword("ProxyMethod").unwrap_or(0);
    if method == 0 {
        return;
    }
    let line = Some(s.line);
    let Some(host) = s.string("ProxyHost") else {
        b.warn(
            WarningKind::Invalid,
            line,
            Some(name),
            "proxy without host ignored",
        );
        return;
    };
    let port = s
        .dword("ProxyPort")
        .and_then(|p| u16::try_from(p).ok())
        .filter(|p| *p != 0);
    let user = s.string("ProxyUsername").map(str::to_owned);
    let kind = match method {
        2 => ProxyKind::Socks5,
        3 => ProxyKind::Http,
        6 => {
            pending.push(PendingJump {
                host_index: usize::MAX,
                target: host.to_owned(),
                port,
                user,
            });
            return;
        }
        1 => {
            b.warn(
                WarningKind::Unsupported,
                line,
                Some(name),
                "SOCKS4 proxies are not supported; proxy ignored",
            );
            return;
        }
        _ => {
            b.warn(
                WarningKind::Unsupported,
                line,
                Some(name),
                "this proxy type (telnet, local command or SSH exec) is not supported; ignored",
            );
            return;
        }
    };
    let password = s.string("ProxyPassword").map(|p| {
        b.add_secret(
            SecretKind::ProxyPassword,
            format!("proxy password for {name}"),
            Zeroizing::new(p.as_bytes().to_vec()),
        )
    });
    d.proxy = Some(ProxyConfig {
        kind,
        host: host.to_owned(),
        port: port.unwrap_or(ProxyConfig::default_port(kind)),
        username: user,
        password,
    });
}

/// Resolves PuTTY 0.77+ "SSH to proxy and use port forwarding" settings to jump hosts.
fn resolve_jumps(
    b: &mut ImportBundle,
    by_name: &HashMap<String, HostId>,
    pending: Vec<PendingJump>,
) {
    let mut created: HashMap<(String, Option<u16>, Option<String>), HostId> = HashMap::new();
    for p in pending {
        let hop = if let (Some(id), None, None) = (by_name.get(&p.target), p.port, &p.user) {
            *id
        } else if let Some(id) = created.get(&(p.target.clone(), p.port, p.user.clone())) {
            *id
        } else {
            let mut d = HostDraft::new(&p.target, &p.target, Protocol::Ssh);
            d.port = p.port;
            d.username(p.user.as_deref().unwrap_or_default());
            let host = d.build(b);
            let id = host.id;
            b.hosts.push(host);
            created.insert((p.target, p.port, p.user), id);
            id
        };
        if let Some(h) = b.hosts.get_mut(p.host_index) {
            if h.id == hop {
                continue;
            }
            h.config.ssh.jump_chain = Some(vec![hop]);
        }
    }
}

fn port_forwards(b: &mut ImportBundle, spec: &str, host: HostId, name: &str, line: usize) {
    for entry in spec.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        match parse_forward(entry) {
            Some((kind, bind_address, bind_port, dest)) => {
                let mut rule = PortForwardRule {
                    id: RuleId::new(),
                    label: String::new(),
                    kind,
                    host,
                    bind_address,
                    bind_port,
                    dest_host: dest.as_ref().map(|(h, _)| h.clone()),
                    dest_port: dest.as_ref().map(|(_, p)| *p),
                    auto_start: false,
                    created_at: 0,
                    updated_at: 0,
                };
                rule.label = format!("{name} {}", rule.openssh_flag());
                b.forwards.push(rule);
            }
            None => b.warn(
                WarningKind::Invalid,
                Some(line),
                Some(name),
                "invalid port forwarding entry skipped",
            ),
        }
    }
}

type ForwardParts = (ForwardKind, String, u16, Option<(String, u16)>);

/// Parses one PuTTY forwarding entry such as `L8080=localhost:80`, `4R127.0.0.1:22=h:22` or
/// `D1080`.
fn parse_forward(entry: &str) -> Option<ForwardParts> {
    let entry = entry.trim_start_matches(['4', '6']);
    let mut chars = entry.chars();
    let kind = match chars.next()? {
        'L' => ForwardKind::Local,
        'R' => ForwardKind::Remote,
        'D' => ForwardKind::Dynamic,
        _ => return None,
    };
    let rest = chars.as_str();
    let (listen, dest) = match rest.split_once('=') {
        Some((l, d)) => (l, Some(d)),
        None => (rest, None),
    };
    let (bind, port) = match crate::common::parse_port(listen) {
        Some(p) => ("localhost".to_owned(), p),
        None => host_and_port(listen)?,
    };
    let dest = if kind == ForwardKind::Dynamic {
        None
    } else {
        Some(host_and_port(dest?)?)
    };
    Some((kind, bind, port, dest))
}

/// Splits `host:port` or `[v6]:port` (PuTTY also writes bare `v6:port`).
fn host_and_port(s: &str) -> Option<(String, u16)> {
    let (h, p) = s.rsplit_once(':')?;
    let h = h.trim_start_matches('[').trim_end_matches(']');
    if h.is_empty() {
        return None;
    }
    Some((h.to_owned(), crate::common::parse_port(p)?))
}

/// Converts a PuTTY host key cache entry (`type@port:host` = numbers) to a known host.
fn convert_host_key(name: &str, value: &str) -> Result<KnownHost, &'static str> {
    let (ptype, rest) = name.split_once('@').ok_or("malformed entry name")?;
    let (port, host) = rest.split_once(':').ok_or("malformed entry name")?;
    let port: u16 = port.parse().map_err(|_| "invalid port")?;
    if host.is_empty() || host.chars().any(|c| c.is_whitespace() || c == ',') {
        return Err("invalid host name");
    }
    split_host_port(host).ok_or("invalid host name")?;
    let nums: Vec<&str> = value.split(',').map(str::trim).collect();
    let nums: Vec<&str> = match nums.first() {
        // Old PuTTY versions prefix ECDSA entries with the curve name.
        Some(first) if !first.starts_with("0x") => nums.get(1..).unwrap_or_default().to_vec(),
        _ => nums,
    };
    let nums: Vec<Vec<u8>> = nums
        .iter()
        .map(|n| parse_hex(n))
        .collect::<Option<_>>()
        .ok_or("malformed key value")?;
    let (key_type, blob) = match (ptype, nums.as_slice()) {
        ("rsa2", [e, n]) => {
            let mut blob = Vec::new();
            put_string(&mut blob, b"ssh-rsa");
            put_mpint(&mut blob, e);
            put_mpint(&mut blob, n);
            ("ssh-rsa", blob)
        }
        ("dss", [p, q, g, y]) => {
            let mut blob = Vec::new();
            put_string(&mut blob, b"ssh-dss");
            for v in [p, q, g, y] {
                put_mpint(&mut blob, v);
            }
            ("ssh-dss", blob)
        }
        ("ssh-ed25519", [x, y]) => ("ssh-ed25519", ed25519_blob(x, y)?),
        ("ecdsa-sha2-nistp256", [x, y]) => ecdsa_blob("ecdsa-sha2-nistp256", "nistp256", 32, x, y)?,
        ("ecdsa-sha2-nistp384", [x, y]) => ecdsa_blob("ecdsa-sha2-nistp384", "nistp384", 48, x, y)?,
        ("ecdsa-sha2-nistp521", [x, y]) => ecdsa_blob("ecdsa-sha2-nistp521", "nistp521", 66, x, y)?,
        (t, _)
            if matches!(t, "rsa2" | "dss" | "ssh-ed25519") || t.starts_with("ecdsa-sha2-nistp") =>
        {
            return Err("unexpected number of key components");
        }
        _ => return Err("key type cannot be converted"),
    };
    let pattern = if port == 22 {
        host.to_owned()
    } else {
        format!("[{host}]:{port}")
    };
    Ok(KnownHost {
        id: KnownHostId::new(),
        pattern,
        key_type: key_type.to_owned(),
        public_key: encode_blob(&blob),
        fingerprint: sha256_fingerprint(&blob),
        marker: None,
        comment: Some("imported from PuTTY".to_owned()),
        added_at: 0,
    })
}

/// Parses `0x<hex>` into big-endian bytes.
fn parse_hex(s: &str) -> Option<Vec<u8>> {
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
    if hex.is_empty() || hex.len() > MAX_HEX_DIGITS {
        return None;
    }
    let digits: Vec<u8> = hex
        .chars()
        .map(|c| c.to_digit(16).and_then(|d| u8::try_from(d).ok()))
        .collect::<Option<_>>()?;
    let mut out = Vec::with_capacity(digits.len().div_ceil(2));
    let mut iter = digits.iter();
    if digits.len() % 2 == 1 {
        out.push(*iter.next()?);
    }
    while let (Some(hi), Some(lo)) = (iter.next(), iter.next()) {
        out.push((hi << 4) | lo);
    }
    Some(out)
}

/// Left-pads (or strips leading zeros of) a big-endian number to exactly `len` bytes.
fn fixed_be(v: &[u8], len: usize) -> Option<Vec<u8>> {
    let start = v.iter().position(|b| *b != 0).unwrap_or(v.len());
    let digits = v.get(start..)?;
    if digits.len() > len {
        return None;
    }
    let mut out = vec![0u8; len - digits.len()];
    out.extend_from_slice(digits);
    Some(out)
}

/// Encodes an Ed25519 point given as affine `(x, y)` (RFC 8032 §5.1.2).
fn ed25519_blob(x: &[u8], y: &[u8]) -> Result<Vec<u8>, &'static str> {
    let mut enc = fixed_be(y, 32).ok_or("invalid Ed25519 point")?;
    enc.reverse();
    let last = enc.last_mut().ok_or("invalid Ed25519 point")?;
    if *last & 0x80 != 0 {
        return Err("invalid Ed25519 point");
    }
    if x.last().is_some_and(|b| b & 1 == 1) {
        *last |= 0x80;
    }
    let mut blob = Vec::new();
    put_string(&mut blob, b"ssh-ed25519");
    put_string(&mut blob, &enc);
    Ok(blob)
}

fn ecdsa_blob(
    key_type: &'static str,
    curve: &str,
    size: usize,
    x: &[u8],
    y: &[u8],
) -> Result<(&'static str, Vec<u8>), &'static str> {
    let mut point = vec![0x04];
    point.extend(fixed_be(x, size).ok_or("invalid ECDSA point")?);
    point.extend(fixed_be(y, size).ok_or("invalid ECDSA point")?);
    let mut blob = Vec::new();
    put_string(&mut blob, key_type.as_bytes());
    put_string(&mut blob, curve.as_bytes());
    put_string(&mut blob, &point);
    Ok((key_type, blob))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::text::decode_text;
    use core_model::Credential;

    const FIXTURE: &str = r#"Windows Registry Editor Version 5.00

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions]

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\Default%20Settings]
"HostName"=""
"Protocol"="ssh"

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\My%20Web%20Server]
"Present"=dword:00000001
"HostName"="web.example.com"
"LogFileName"="putty.log"
"Protocol"="ssh"
"PortNumber"=dword:00000016
"UserName"="deploy"
"PortForwardings"="L8080=localhost:80,4R2222=localhost:22,D1080,L127.0.0.1:5432=db.internal:5432,Xbad"
"PublicKeyFile"="C:\\Users\\me\\.ssh\\deploy.ppk"
"ProxyMethod"=dword:00000002
"ProxyHost"="proxy.example.com"
"ProxyPort"=dword:00000438
"ProxyUsername"="puser"
"ProxyPassword"="proxy-secret"
"Colour0"="187,187,187"
"Wordness0"=hex:00,00,00,00,00,00,00,00,00,00,00,00,00,00,00,00,00,00,00,00,\
  00,00,00,00,00,00,00,00,00,00,00,00

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\bastion]
"HostName"="ops@bastion.example.com"
"PortNumber"=dword:00000801
"Protocol"="ssh"

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\internal]
"HostName"="10.1.2.3"
"Protocol"="ssh"
"ProxyMethod"=dword:00000006
"ProxyHost"="bastion"

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\router]
"HostName"="192.0.2.1"
"Protocol"="telnet"
"PortNumber"=dword:00000017

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\com1]
"HostName"=""
"Protocol"="serial"

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\socks4]
"HostName"="old.example.com"
"ProxyMethod"=dword:00000001
"ProxyHost"="p"

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\SshHostKeys]
"ssh-ed25519@22:github.com"="0x7c63b118b878f3c2b9606a129009fe407d4a80a7724639bbd691b54c2fb2ad60,0x65a2187d69871d2d5a24da0bf9016e1abb282aea50bad149b4b9ce1579aa2ae3"
"rsa2@2222:rsa.example.com"="0x10001,0xca3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a35"
"ssh-ed448@22:new.example.com"="0x01,0x02"
"rsa2@22:bad.example.com"="0xZZ,0x01"
"#;

    #[test]
    fn realistic_export() {
        let b = parse(FIXTURE).unwrap();
        let labels: Vec<&str> = b.hosts.iter().map(|h| h.label.as_str()).collect();
        assert_eq!(
            labels,
            ["My Web Server", "bastion", "internal", "router", "socks4"]
        );
        let web = &b.hosts[0];
        assert_eq!(web.address, "web.example.com");
        assert_eq!(web.config.ssh.port, Some(22));
        let Some(Credential::Inline { username, keys, .. }) = &web.config.ssh.credential else {
            panic!()
        };
        assert_eq!(username.as_deref(), Some("deploy"));
        assert_eq!(keys.len(), 1);
        assert_eq!(b.keys[0].format, KeyFileFormat::Ppk);
        assert_eq!(
            b.keys[0].source_path.as_deref(),
            Some(r"C:\Users\me\.ssh\deploy.ppk")
        );
        assert_eq!(b.keys[0].label, "deploy.ppk");
        let proxy = web.config.ssh.proxy.as_ref().unwrap();
        assert_eq!(proxy.kind, ProxyKind::Socks5);
        assert_eq!(proxy.host, "proxy.example.com");
        assert_eq!(proxy.port, 1080);
        assert_eq!(proxy.username.as_deref(), Some("puser"));
        assert_eq!(b.secrets.len(), 1);
        assert_eq!(Some(b.secrets[0].slot), proxy.password);
        assert_eq!(b.secrets[0].value.as_slice(), b"proxy-secret");
        assert_eq!(b.secrets[0].kind, SecretKind::ProxyPassword);

        let flags: Vec<String> = b
            .forwards
            .iter()
            .map(PortForwardRule::openssh_flag)
            .collect();
        assert_eq!(
            flags,
            [
                "-L localhost:8080:localhost:80",
                "-R localhost:2222:localhost:22",
                "-D localhost:1080",
                "-L 127.0.0.1:5432:db.internal:5432"
            ]
        );

        let bastion = &b.hosts[1];
        assert_eq!(bastion.address, "bastion.example.com");
        assert_eq!(bastion.config.ssh.port, Some(2049));
        let internal = &b.hosts[2];
        assert_eq!(internal.config.ssh.jump_chain, Some(vec![bastion.id]));
        let router = &b.hosts[3];
        assert_eq!(router.config.telnet.enabled, Some(true));
        assert_eq!(router.config.telnet.port, Some(23));
        assert_eq!(b.hosts[4].config.ssh.proxy, None);

        assert_eq!(b.known_hosts.len(), 2);
        let gh = &b.known_hosts[0];
        assert_eq!(gh.pattern, "github.com");
        assert_eq!(gh.key_type, "ssh-ed25519");
        assert_eq!(
            gh.public_key,
            "AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl"
        );
        assert_eq!(
            gh.fingerprint,
            "SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU"
        );
        let rsa = &b.known_hosts[1];
        assert_eq!(rsa.pattern, "[rsa.example.com]:2222");
        assert_eq!(
            rsa.fingerprint,
            "SHA256:IfDHOrMcBP+1j8mm946xBIgRRzoukoMZuyGVE9aBo5M"
        );

        let msgs: Vec<String> = b.warnings.iter().map(ToString::to_string).collect();
        let has = |s: &str| msgs.iter().any(|m| m.contains(s));
        assert!(has("invalid port forwarding entry"), "{msgs:#?}");
        assert!(has("serial sessions"));
        assert!(has("SOCKS4"));
        assert!(has("ssh-ed448@22:new.example.com"));
        assert!(has("malformed key value"));
        assert!(!format!("{b:?}").contains("proxy-secret"));
    }

    #[test]
    fn utf16_export_through_decoder() {
        let mut bytes = vec![0xFF, 0xFE];
        let text = "Windows Registry Editor Version 5.00\r\n\r\n\
            [HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\caf%C3%A9]\r\n\
            \"HostName\"=\"cafe.example.com\"\r\n";
        for u in text.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        let decoded = decode_text(&bytes).unwrap();
        let b = parse(&decoded.text).unwrap();
        assert_eq!(b.hosts.len(), 1);
        assert_eq!(b.hosts[0].label, "café");
    }

    #[test]
    fn rejects_non_registry_files() {
        assert!(parse("Host foo\n").is_err());
        assert!(parse("").is_err());
        assert!(parse("REGEDIT4\n").unwrap().is_empty());
    }

    #[test]
    fn helpers() {
        assert_eq!(url_decode("a%20b%2"), "a b%2");
        assert_eq!(url_decode("%zz%41"), "%zzA");
        assert_eq!(parse_hex("0x123").unwrap(), [0x01, 0x23]);
        assert!(parse_hex("123").is_none());
        assert!(parse_hex("0x").is_none());
        assert_eq!(
            parse_value_line(r#""a\"b"="c\\d""#)
                .map(|(n, v)| (n, matches!(v, Value::Str(s) if s == r"c\d"))),
            Some((r#"a"b"#.to_owned(), true))
        );
        assert!(parse_value_line(r#""unterminated=1"#).is_none());
        assert!(parse_value_line(r#""x"=dword:zz"#).is_none());
        let p = parse_forward("6L[::1]:8080=[fe80::1]:80").unwrap();
        assert_eq!(p.1, "::1");
        assert_eq!(p.3, Some(("fe80::1".to_owned(), 80)));
        assert!(parse_forward("L8080").is_none());
        assert!(parse_forward("D0").is_none());
    }

    #[test]
    fn ecdsa_and_dss_host_keys() {
        let kh = convert_host_key("ecdsa-sha2-nistp256@22:h.example", "0x01,0x02").unwrap();
        assert_eq!(kh.key_type, "ecdsa-sha2-nistp256");
        let blob = crate::wire::decode_blob(&kh.public_key).unwrap();
        assert_eq!(blob.len(), 4 + 19 + 4 + 8 + 4 + 65);
        let kh = convert_host_key("ecdsa-sha2-nistp384@22:h", "nistp384,0x01,0x02").unwrap();
        assert_eq!(kh.key_type, "ecdsa-sha2-nistp384");
        let kh = convert_host_key("dss@22:h", "0x1,0x2,0x3,0x4").unwrap();
        assert_eq!(kh.key_type, "ssh-dss");
        assert!(
            convert_host_key("ssh-ed25519@22:h", &format!("0x1,0x8{}", "0".repeat(63))).is_err()
        );
        assert!(convert_host_key("rsa2@x:h", "0x1,0x2").is_err());
        assert!(convert_host_key("rsa2@22:", "0x1,0x2").is_err());
        assert!(convert_host_key("rsa2@22:h", "0x1").is_err());
    }
}
