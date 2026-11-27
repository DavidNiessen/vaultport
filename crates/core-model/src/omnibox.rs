// SPDX-License-Identifier: GPL-3.0-or-later
//! Parses ad-hoc connection strings typed into the host search box.
//!
//! Accepted forms (a subset of the OpenSSH command line):
//!
//! - `host`, `user@host`, `user@host:2222`, `[::1]:2222`
//! - `ssh user@host -p 2222`, `ssh -p 2222 -l user host`
//! - `ssh -J bastion user@host`, `ssh://user@host:2222`

use crate::error::ModelError;
use crate::validate::{validate_hostname, validate_port};
use serde::{Deserialize, Serialize};

/// Result of parsing a quick-connect string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct QuickConnect {
    pub username: Option<String>,
    pub host: String,
    pub port: Option<u16>,
    /// `-J` jump specification, as typed (`user@bastion:22,other`).
    pub jump: Option<String>,
    /// Remaining command to execute remotely, if any.
    pub command: Option<String>,
}

/// Parses `input` as a quick-connect string. Returns `Ok(None)` when the input does not look
/// like a connection string (e.g. a plain search query with spaces).
pub fn parse_quick_connect(input: &str) -> Result<Option<QuickConnect>, ModelError> {
    let s = input.trim();
    if s.is_empty() {
        return Ok(None);
    }
    if let Some(rest) = s.strip_prefix("ssh://") {
        return parse_target(rest.trim_end_matches('/')).map(Some);
    }
    let tokens = shell_split(s)?;
    let (first, args) = match tokens.split_first() {
        Some((f, a)) => (f.as_str(), a),
        None => return Ok(None),
    };
    if first != "ssh" {
        // A single token that looks like user@host / host:port.
        if tokens.len() == 1 && (first.contains('@') || first.contains(':') || first.contains('.'))
        {
            return parse_target(first).map(Some).or(Ok(None));
        }
        return Ok(None);
    }

    let mut port = None;
    let mut login = None;
    let mut jump = None;
    let mut target = None;
    let mut command = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        // Like OpenSSH, options may follow the destination until the command starts.
        if target.is_some() && (!command.is_empty() || !a.starts_with('-')) {
            command.push(a.to_owned());
            i += 1;
            continue;
        }
        match a {
            "-p" | "-l" | "-J" | "-i" | "-o" | "-F" | "-L" | "-R" | "-D" | "-W" | "-b" | "-c"
            | "-m" | "-E" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| ModelError::Parse(format!("{a} needs a value")))?;
                match a {
                    "-p" => port = Some(parse_port(v)?),
                    "-l" => login = Some(v.clone()),
                    "-J" => jump = Some(v.clone()),
                    _ => {}
                }
                i += 2;
            }
            flag if flag.starts_with("-p") && flag.len() > 2 => {
                port = Some(parse_port(&flag[2..])?);
                i += 1;
            }
            flag if flag.starts_with('-') => {
                i += 1;
            }
            other => {
                target = Some(other.to_owned());
                i += 1;
            }
        }
    }
    let target = target.ok_or_else(|| ModelError::Parse("missing destination".into()))?;
    let mut qc = parse_target(&target)?;
    if qc.username.is_none() {
        qc.username = login;
    }
    if port.is_some() {
        qc.port = port;
    }
    qc.jump = jump;
    if !command.is_empty() {
        qc.command = Some(command.join(" "));
    }
    Ok(Some(qc))
}

fn parse_port(s: &str) -> Result<u16, ModelError> {
    let p: u16 = s
        .parse()
        .map_err(|_| ModelError::InvalidPort(s.to_owned()))?;
    validate_port(p)?;
    Ok(p)
}

fn parse_target(t: &str) -> Result<QuickConnect, ModelError> {
    let (user, hostport) = match t.rsplit_once('@') {
        Some((u, h)) if !u.is_empty() => (Some(u.to_owned()), h),
        Some((_, h)) => (None, h),
        None => (None, t),
    };
    let (host, port) = if let Some(rest) = hostport.strip_prefix('[') {
        let (h, after) = rest
            .split_once(']')
            .ok_or_else(|| ModelError::Parse("unterminated IPv6 address".into()))?;
        let port = match after.strip_prefix(':') {
            Some(p) => Some(parse_port(p)?),
            None if after.is_empty() => None,
            None => return Err(ModelError::Parse(format!("unexpected {after:?}"))),
        };
        (h.to_owned(), port)
    } else if hostport.matches(':').count() == 1 {
        let (h, p) = hostport.split_once(':').unwrap_or((hostport, ""));
        (h.to_owned(), Some(parse_port(p)?))
    } else {
        (hostport.to_owned(), None)
    };
    validate_hostname(&host)?;
    Ok(QuickConnect {
        username: user,
        host,
        port,
        jump: None,
        command: None,
    })
}

/// Minimal POSIX-like word splitting supporting single and double quotes and backslashes.
fn shell_split(s: &str) -> Result<Vec<String>, ModelError> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(x) => cur.push(x),
                        None => return Err(ModelError::Parse("unterminated quote".into())),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => {
                            if let Some(x) = chars.next() {
                                cur.push(x);
                            }
                        }
                        Some(x) => cur.push(x),
                        None => return Err(ModelError::Parse("unterminated quote".into())),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(x) = chars.next() {
                    cur.push(x);
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    out.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                cur.push(c);
            }
        }
    }
    if in_word {
        out.push(cur);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qc(s: &str) -> QuickConnect {
        parse_quick_connect(s)
            .unwrap_or_else(|e| panic!("{s}: {e}"))
            .unwrap_or_else(|| panic!("{s}: not a connection string"))
    }

    #[test]
    fn parses_common_forms() {
        let a = qc("ssh root@10.0.0.1 -p 2222");
        assert_eq!(a.username.as_deref(), Some("root"));
        assert_eq!(a.host, "10.0.0.1");
        assert_eq!(a.port, Some(2222));

        let b = qc("ssh -p2200 -l admin -J bastion example.com uptime -a");
        assert_eq!(b.username.as_deref(), Some("admin"));
        assert_eq!(b.port, Some(2200));
        assert_eq!(b.jump.as_deref(), Some("bastion"));
        assert_eq!(b.command.as_deref(), Some("uptime -a"));

        let c = qc("deploy@[fd00::1]:22");
        assert_eq!(c.host, "fd00::1");
        assert_eq!(c.port, Some(22));

        let d = qc("ssh://me@host.example:2022/");
        assert_eq!(d.port, Some(2022));
    }

    #[test]
    fn plain_queries_are_not_connections() {
        assert_eq!(parse_quick_connect("prod db").ok().flatten(), None);
        assert_eq!(parse_quick_connect("web").ok().flatten(), None);
        assert_eq!(parse_quick_connect("").ok().flatten(), None);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_quick_connect("ssh -p").is_err());
        assert!(parse_quick_connect("ssh -p 99999 host").is_err());
        assert!(parse_quick_connect("ssh 'unterminated").is_err());
    }
}
