// SPDX-License-Identifier: GPL-3.0-or-later
//! Dynamic argument values from files the app can read on the machine the shell runs on.
//!
//! Completion often needs values that only exist on that machine: git branches, npm scripts,
//! make targets, users, SSH hosts, systemd units. Running commands for them would need an
//! exec channel (and show up in the server's logs), so instead the app reads a few small files
//! over the same file system it lists directories with (the local disk or SFTP) and these
//! pure functions turn their contents into [`Value`]s. Every parser is bounded, never panics
//! and ignores what it does not understand.
//!
//! [`sources_for`] says which files are worth reading for an argument kind and command.

use crate::spec::ArgKind;
use serde::{Deserialize, Serialize};

/// Most values a generator returns.
pub const MAX_VALUES: usize = 2000;
/// Largest file the app should read for a generator, in bytes.
pub const MAX_FILE_BYTES: u64 = 512 * 1024;

/// A completion value with an optional description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Value {
    pub value: String,
    pub description: Option<String>,
}

impl Value {
    fn new(value: impl Into<String>, description: Option<String>) -> Self {
        Self {
            value: value.into(),
            description,
        }
    }
}

fn dedup(mut values: Vec<Value>) -> Vec<Value> {
    let mut seen = std::collections::HashSet::new();
    values.retain(|v| !v.value.is_empty() && seen.insert(v.value.clone()));
    values.truncate(MAX_VALUES);
    values
}

/// What the app should read to produce values (paths are relative to the place they name).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Source {
    /// The git directory of the repository containing the working directory: `HEAD`,
    /// `packed-refs`, `config`, and the listings of `refs/heads`, `refs/remotes/*` and
    /// `refs/tags`.
    GitRepo,
    /// A file in the working directory (or its closest ancestor that has it).
    ProjectFile { name: String },
    /// An absolute file.
    File { path: String },
    /// A file in the home directory.
    HomeFile { path: String },
    /// The names in an absolute directory.
    Dir { path: String },
    /// Built-in values that need no I/O.
    Static,
}

/// The files that hold values of `kind` for the command `path` (canonical command and
/// subcommand names, e.g. `["npm", "run"]`). Empty if there is nothing to read.
#[must_use]
pub fn sources_for(kind: ArgKind, path: &[String]) -> Vec<Source> {
    let root = path.first().map(String::as_str).unwrap_or_default();
    let file = |p: &str| Source::File { path: p.to_owned() };
    let project = |n: &str| Source::ProjectFile { name: n.to_owned() };
    match kind {
        ArgKind::Branch | ArgKind::Ref | ArgKind::Remote => vec![Source::GitRepo],
        ArgKind::Script => match root {
            "make" | "gmake" => vec![
                project("Makefile"),
                project("makefile"),
                project("GNUmakefile"),
            ],
            "just" => vec![
                project("justfile"),
                project("Justfile"),
                project(".justfile"),
            ],
            "cargo" => vec![project("Cargo.toml")],
            "deno" => vec![project("deno.json"), project("deno.jsonc")],
            "composer" => vec![project("composer.json")],
            _ => vec![project("package.json")],
        },
        ArgKind::User => vec![file("/etc/passwd")],
        ArgKind::Group => vec![file("/etc/group")],
        ArgKind::Host => vec![
            Source::HomeFile {
                path: ".ssh/config".into(),
            },
            Source::HomeFile {
                path: ".ssh/known_hosts".into(),
            },
            file("/etc/hosts"),
        ],
        ArgKind::Service => vec![
            Source::Dir {
                path: "/etc/systemd/system".into(),
            },
            Source::Dir {
                path: "/lib/systemd/system".into(),
            },
            Source::Dir {
                path: "/usr/lib/systemd/system".into(),
            },
            Source::Dir {
                path: "/etc/init.d".into(),
            },
            Source::Dir {
                path: "/Library/LaunchDaemons".into(),
            },
            Source::Dir {
                path: "/Library/LaunchAgents".into(),
            },
        ],
        ArgKind::Interface => vec![Source::Dir {
            path: "/sys/class/net".into(),
        }],
        ArgKind::Signal => vec![Source::Static],
        _ => Vec::new(),
    }
}

/// Signal names (without `SIG`) with what they usually mean.
pub const SIGNALS: &[(&str, &str)] = &[
    ("TERM", "Terminate gracefully (default)"),
    ("KILL", "Kill immediately (cannot be caught)"),
    ("INT", "Interrupt, like Ctrl+C"),
    ("HUP", "Hang up; many daemons reload their configuration"),
    ("QUIT", "Quit and dump core"),
    ("STOP", "Pause (cannot be caught)"),
    ("CONT", "Continue a paused process"),
    ("TSTP", "Pause from the terminal, like Ctrl+Z"),
    ("USR1", "User-defined signal 1"),
    ("USR2", "User-defined signal 2"),
    ("WINCH", "Terminal window size changed"),
    ("ALRM", "Alarm clock"),
    ("CHLD", "Child process changed state"),
    ("PIPE", "Broken pipe"),
    ("ABRT", "Abort"),
];

/// [`SIGNALS`] as values.
#[must_use]
pub fn signals() -> Vec<Value> {
    SIGNALS
        .iter()
        .map(|(n, d)| Value::new(*n, Some((*d).to_owned())))
        .collect()
}

/// The files of a git directory that [`git_refs`] reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitFiles {
    /// Contents of `HEAD`.
    pub head: String,
    /// Contents of `packed-refs`.
    pub packed_refs: String,
    /// Contents of `config`.
    pub config: String,
    /// Names under `refs/heads` (nested directories joined with `/`).
    pub heads: Vec<String>,
    /// Names under `refs/remotes`, as `remote/branch`.
    pub remotes: Vec<String>,
    /// Names under `refs/tags`.
    pub tags: Vec<String>,
}

/// Branches, remote branches, tags and remotes of a repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitRefs {
    /// The checked-out branch, if any.
    pub current: Option<String>,
    pub branches: Vec<String>,
    /// `remote/branch`.
    pub remote_branches: Vec<String>,
    pub tags: Vec<String>,
    pub remotes: Vec<String>,
}

/// Parses the files of a git directory. See [`GitFiles`].
#[must_use]
pub fn git_refs(files: &GitFiles) -> GitRefs {
    let mut refs = GitRefs {
        current: files
            .head
            .trim()
            .strip_prefix("ref: refs/heads/")
            .map(str::to_owned),
        branches: files.heads.clone(),
        remote_branches: files.remotes.clone(),
        tags: files.tags.clone(),
        remotes: Vec::new(),
    };
    for line in files.packed_refs.lines().take(100_000) {
        if line.starts_with('#') || line.starts_with('^') {
            continue;
        }
        let Some((_, name)) = line.split_once(' ') else {
            continue;
        };
        if let Some(b) = name.strip_prefix("refs/heads/") {
            refs.branches.push(b.to_owned());
        } else if let Some(r) = name.strip_prefix("refs/remotes/") {
            refs.remote_branches.push(r.to_owned());
        } else if let Some(t) = name.strip_prefix("refs/tags/") {
            refs.tags.push(t.to_owned());
        }
    }
    for line in files.config.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("[remote \"")
            && let Some(name) = rest.strip_suffix("\"]")
        {
            refs.remotes.push(name.to_owned());
        }
    }
    let clean = |v: &mut Vec<String>| {
        v.retain(|s| !s.is_empty() && !s.ends_with("/HEAD") && s != "HEAD");
        v.sort();
        v.dedup();
        v.truncate(MAX_VALUES);
    };
    clean(&mut refs.branches);
    clean(&mut refs.remote_branches);
    clean(&mut refs.tags);
    clean(&mut refs.remotes);
    refs
}

/// The values of `kind` (branch, ref or remote) in `refs`. The current branch comes first.
#[must_use]
pub fn git_values(refs: &GitRefs, kind: ArgKind) -> Vec<Value> {
    let mut out = Vec::new();
    match kind {
        ArgKind::Remote => {
            for r in &refs.remotes {
                out.push(Value::new(r.clone(), Some("remote".into())));
            }
        }
        ArgKind::Branch | ArgKind::Ref => {
            if let Some(c) = &refs.current {
                out.push(Value::new(c.clone(), Some("current branch".into())));
            }
            for b in &refs.branches {
                out.push(Value::new(b.clone(), Some("branch".into())));
            }
            for b in &refs.remote_branches {
                out.push(Value::new(b.clone(), Some("remote branch".into())));
            }
            if kind == ArgKind::Ref {
                for t in &refs.tags {
                    out.push(Value::new(t.clone(), Some("tag".into())));
                }
                for h in ["HEAD", "HEAD~1", "ORIG_HEAD", "FETCH_HEAD"] {
                    out.push(Value::new(h, Some("ref".into())));
                }
            }
        }
        _ => {}
    }
    dedup(out)
}

/// `scripts` of a `package.json` (name, command).
#[must_use]
pub fn package_scripts(json: &str) -> Vec<Value> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(scripts) = v.get("scripts").and_then(|s| s.as_object()) {
        for (name, cmd) in scripts {
            out.push(Value::new(
                name.clone(),
                cmd.as_str().map(|c| shorten(c, 120)),
            ));
        }
    }
    dedup(out)
}

/// `tasks` of a `deno.json`.
#[must_use]
pub fn deno_tasks(json: &str) -> Vec<Value> {
    // deno.jsonc may contain comments: drop whole-line ones, which is the common case.
    let stripped: String = json
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&stripped) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(tasks) = v.get("tasks").and_then(|s| s.as_object()) {
        for (name, cmd) in tasks {
            let desc = cmd
                .as_str()
                .or_else(|| cmd.get("command").and_then(|c| c.as_str()));
            out.push(Value::new(name.clone(), desc.map(|c| shorten(c, 120))));
        }
    }
    dedup(out)
}

/// `scripts` of a `composer.json`.
#[must_use]
pub fn composer_scripts(json: &str) -> Vec<Value> {
    package_scripts(json)
}

/// Explicit targets of a Makefile (pattern rules and special targets are left out). A
/// `## comment` after the target, a common convention, becomes its description.
#[must_use]
pub fn make_targets(makefile: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for line in makefile.lines().take(50_000) {
        if line.starts_with(['\t', ' ', '#', '.']) {
            continue;
        }
        let Some((head, rest)) = line.split_once(':') else {
            continue;
        };
        // `VAR := value`, `VAR ::= value`
        if rest.starts_with('=') || rest.starts_with(":=") {
            continue;
        }
        if head.contains(['=', '%', '$', '(', ')']) {
            continue;
        }
        let desc = rest
            .split_once("##")
            .map(|(_, d)| d.trim().to_owned())
            .filter(|d| !d.is_empty());
        for target in head.split_whitespace() {
            out.push(Value::new(target, desc.clone()));
        }
    }
    dedup(out)
}

/// Recipes of a justfile. A `# comment` line right above a recipe becomes its description.
#[must_use]
pub fn just_recipes(justfile: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let mut comment: Option<String> = None;
    for line in justfile.lines().take(50_000) {
        if let Some(c) = line.strip_prefix('#') {
            comment = Some(c.trim().to_owned()).filter(|c| !c.is_empty() && !c.starts_with('!'));
            continue;
        }
        if line.starts_with([' ', '\t']) || line.trim().is_empty() {
            comment = None;
            continue;
        }
        let head = line.trim_start_matches('@');
        let Some((sig, rest)) = head.split_once(':') else {
            comment = None;
            continue;
        };
        if rest.starts_with('=') || sig.starts_with(['[', '!']) || sig.contains(":=") {
            comment = None;
            continue;
        }
        let mut words = sig.split_whitespace();
        let name = words.next().unwrap_or_default();
        if matches!(name, "set" | "alias" | "export" | "import" | "mod")
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            comment = None;
            continue;
        }
        out.push(Value::new(name, comment.take()));
    }
    dedup(out)
}

/// `[alias]` entries of a `.cargo/config.toml` are not read; the targets that matter for
/// completion are the `[[bin]]` and `[[example]]` names and workspace `members` of a
/// `Cargo.toml`, offered for `cargo run --bin`, `-p` and similar.
#[must_use]
pub fn cargo_targets(toml: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let mut section = String::new();
    let mut in_members = false;
    for raw in toml.lines().take(20_000) {
        let line = raw.trim();
        if line.starts_with('[') {
            line.trim_matches(|c| c == '[' || c == ']')
                .clone_into(&mut section);
            in_members = false;
            continue;
        }
        if let Some(v) = line.strip_prefix("name")
            && let Some(v) = v.trim_start().strip_prefix('=')
        {
            let name = v.trim().trim_matches('"');
            let desc = match section.as_str() {
                "package" => "package",
                "bin" => "binary",
                "example" => "example",
                _ => continue,
            };
            out.push(Value::new(name, Some(desc.into())));
        }
        if section == "workspace" && line.starts_with("members") {
            in_members = true;
        }
        if in_members {
            for part in line.split('"').skip(1).step_by(2) {
                let name = part.rsplit('/').next().unwrap_or(part);
                if !name.contains('*') {
                    out.push(Value::new(name, Some("workspace member".into())));
                }
            }
            if line.contains(']') {
                in_members = false;
            }
        }
    }
    dedup(out)
}

/// User names from `/etc/passwd` (with the full name as description). System accounts with a
/// `nologin` / `false` shell come after the people.
#[must_use]
pub fn passwd_users(passwd: &str) -> Vec<Value> {
    let mut people = Vec::new();
    let mut system = Vec::new();
    for line in passwd.lines().take(50_000) {
        if line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split(':').collect();
        let (Some(name), Some(gecos), Some(shell)) = (fields.first(), fields.get(4), fields.get(6))
        else {
            continue;
        };
        let full = gecos.split(',').next().unwrap_or_default().trim();
        let desc = (!full.is_empty()).then(|| full.to_owned());
        if shell.ends_with("nologin") || shell.ends_with("false") {
            system.push(Value::new(*name, desc));
        } else {
            people.push(Value::new(*name, desc));
        }
    }
    people.extend(system);
    dedup(people)
}

/// Group names from `/etc/group`.
#[must_use]
pub fn group_names(group: &str) -> Vec<Value> {
    dedup(
        group
            .lines()
            .take(50_000)
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| l.split(':').next())
            .map(|n| Value::new(n, None))
            .collect(),
    )
}

/// Host aliases of an OpenSSH `ssh_config` (`Host` lines without wildcards), with the
/// `HostName` as description.
#[must_use]
pub fn ssh_config_hosts(config: &str) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    for line in config.lines().take(50_000) {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let (key, value) = line
            .split_once(|c: char| c.is_whitespace() || c == '=')
            .map_or((line, ""), |(k, v)| {
                (k, v.trim().trim_start_matches('=').trim())
            });
        if key.eq_ignore_ascii_case("host") {
            current.clear();
            for h in value.split_whitespace() {
                if h.contains(['*', '?', '!']) {
                    continue;
                }
                current.push(out.len());
                out.push(Value::new(h, None));
            }
        } else if key.eq_ignore_ascii_case("match") {
            current.clear();
        } else if key.eq_ignore_ascii_case("hostname") {
            for &i in &current {
                if let Some(v) = out.get_mut(i) {
                    v.description = Some(value.to_owned());
                }
            }
        }
    }
    dedup(out)
}

/// Host names from `known_hosts` (hashed entries cannot be read and are skipped).
#[must_use]
pub fn known_hosts_names(known_hosts: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for line in known_hosts.lines().take(100_000) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('|') {
            continue;
        }
        let first = line.split_whitespace().next().unwrap_or_default();
        let first = if first.starts_with('@') {
            line.split_whitespace().nth(1).unwrap_or_default()
        } else {
            first
        };
        for h in first.split(',') {
            let h = h
                .strip_prefix('[')
                .and_then(|r| r.split_once("]:").map(|(host, _)| host))
                .unwrap_or(h);
            if !h.is_empty() && !h.starts_with('|') && !h.contains('*') {
                out.push(Value::new(h, Some("known host".into())));
            }
        }
    }
    dedup(out)
}

/// Names from `/etc/hosts` (loopback names are left out).
#[must_use]
pub fn etc_hosts(hosts: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for line in hosts.lines().take(50_000) {
        let line = line.split('#').next().unwrap_or_default();
        let mut parts = line.split_whitespace();
        let Some(addr) = parts.next() else { continue };
        for name in parts {
            if matches!(
                name,
                "localhost" | "ip6-localhost" | "ip6-loopback" | "broadcasthost"
            ) || name.starts_with("ip6-")
            {
                continue;
            }
            out.push(Value::new(name, Some(addr.to_owned())));
        }
    }
    dedup(out)
}

/// Unit names (`*.service`, `*.timer`, `*.socket`, …) from systemd unit directory listings,
/// init scripts from `/etc/init.d` and launchd labels (`*.plist`).
#[must_use]
pub fn service_names<S: AsRef<str>>(entries: impl IntoIterator<Item = S>) -> Vec<Value> {
    const UNIT: &[&str] = &[
        ".service", ".timer", ".socket", ".target", ".mount", ".path",
    ];
    let mut out = Vec::new();
    for e in entries {
        let e = e.as_ref();
        if e.starts_with('.') || e.contains('@') && e.ends_with("@.service") {
            continue;
        }
        if let Some(label) = e.strip_suffix(".plist") {
            out.push(Value::new(label, Some("launchd".into())));
        } else if UNIT.iter().any(|u| e.ends_with(u)) {
            let name = e.strip_suffix(".service").unwrap_or(e);
            let kind = e.rsplit('.').next().unwrap_or("unit");
            out.push(Value::new(name, Some(kind.to_owned())));
        } else if !e.contains('.') && !matches!(e, "README" | "rc" | "rcS" | "skeleton") {
            out.push(Value::new(e, Some("init script".into())));
        }
    }
    out.sort_by(|a, b| a.value.cmp(&b.value));
    dedup(out)
}

/// Common environment variables, for `$` completion where the real environment is unknown.
pub const COMMON_ENV: &[(&str, &str)] = &[
    ("HOME", "Home directory"),
    ("PATH", "Directories searched for commands"),
    ("PWD", "Working directory"),
    ("OLDPWD", "Previous working directory"),
    ("USER", "User name"),
    ("LOGNAME", "Login name"),
    ("SHELL", "Login shell"),
    ("HOSTNAME", "Host name"),
    ("LANG", "Locale"),
    ("LC_ALL", "Locale override"),
    ("TERM", "Terminal type"),
    ("EDITOR", "Preferred editor"),
    ("VISUAL", "Preferred visual editor"),
    ("PAGER", "Preferred pager"),
    ("TMPDIR", "Temporary directory"),
    ("XDG_CONFIG_HOME", "User configuration directory"),
    ("XDG_DATA_HOME", "User data directory"),
    ("XDG_RUNTIME_DIR", "Runtime directory"),
    ("SSH_AUTH_SOCK", "SSH agent socket"),
    (
        "SSH_CONNECTION",
        "Client and server address of this SSH session",
    ),
    ("HISTFILE", "Shell history file"),
    ("PS1", "Prompt"),
    ("?", "Exit status of the last command"),
    ("$", "Process id of the shell"),
    ("!", "Process id of the last background job"),
    ("RANDOM", "A random number"),
];

/// Environment variables completing `prefix` (the name typed after `$` or `${`), from
/// `names` (with values as descriptions, when given) and [`COMMON_ENV`].
#[must_use]
pub fn env_values<'a>(names: impl IntoIterator<Item = (&'a str, Option<&'a str>)>) -> Vec<Value> {
    let mut out: Vec<Value> = names
        .into_iter()
        .filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        .map(|(n, v)| Value::new(n, v.map(|v| shorten(v, 80))))
        .collect();
    for (n, d) in COMMON_ENV {
        if !out.iter().any(|v| v.value == *n) {
            out.push(Value::new(*n, Some((*d).to_owned())));
        }
    }
    dedup(out)
}

/// Shell builtins and keywords with a short description, so they are recognised as commands
/// even though there is no file for them on `PATH`.
pub const BUILTINS: &[(&str, &str)] = &[
    ("alias", "Define or list aliases"),
    ("bg", "Resume a job in the background"),
    ("bind", "Set readline key bindings"),
    ("break", "Exit a loop"),
    ("builtin", "Run a shell builtin"),
    ("case", "Match a value against patterns"),
    ("cd", "Change the working directory"),
    ("command", "Run a command, bypassing functions"),
    ("compgen", "List possible completions"),
    ("complete", "Define a completion"),
    ("continue", "Next loop iteration"),
    ("declare", "Declare variables and attributes"),
    ("dirs", "List the directory stack"),
    ("disown", "Remove a job from the job table"),
    ("echo", "Print arguments"),
    ("enable", "Enable or disable builtins"),
    ("eval", "Run arguments as a command"),
    ("exec", "Replace the shell with a command"),
    ("exit", "Exit the shell"),
    ("export", "Set environment variables"),
    ("false", "Do nothing, unsuccessfully"),
    ("fc", "Edit and rerun history entries"),
    ("fg", "Bring a job to the foreground"),
    ("for", "Loop over a list"),
    ("function", "Define a function"),
    ("getopts", "Parse options in scripts"),
    ("hash", "Remember command locations"),
    ("help", "Help on builtins"),
    ("history", "Show the command history"),
    ("if", "Run commands conditionally"),
    ("jobs", "List jobs"),
    ("kill", "Send a signal to a process"),
    ("let", "Evaluate arithmetic"),
    ("local", "Declare function-local variables"),
    ("logout", "Exit a login shell"),
    ("popd", "Pop from the directory stack"),
    ("printf", "Print formatted output"),
    ("pushd", "Push onto the directory stack"),
    ("pwd", "Print the working directory"),
    ("read", "Read a line from input"),
    ("readonly", "Mark variables read-only"),
    ("return", "Return from a function"),
    ("set", "Set shell options and positional parameters"),
    ("shift", "Shift positional parameters"),
    ("shopt", "Set shell options"),
    ("source", "Run a file in this shell"),
    ("test", "Evaluate a condition"),
    ("time", "Time a command"),
    ("times", "Show process times"),
    ("trap", "Run commands on signals"),
    ("true", "Do nothing, successfully"),
    ("type", "Describe a command"),
    ("typeset", "Declare variables"),
    ("ulimit", "Get or set resource limits"),
    ("umask", "Get or set the file creation mask"),
    ("unalias", "Remove aliases"),
    ("unset", "Remove variables or functions"),
    ("until", "Loop until a condition holds"),
    ("wait", "Wait for jobs"),
    ("while", "Loop while a condition holds"),
];

fn shorten(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[Value]) -> Vec<&str> {
        v.iter().map(|v| v.value.as_str()).collect()
    }

    #[test]
    fn git_refs_from_loose_and_packed() {
        let files = GitFiles {
            head: "ref: refs/heads/feature/x\n".into(),
            packed_refs: "# pack-refs with: peeled fully-peeled sorted\n\
                          1111 refs/heads/main\n\
                          2222 refs/remotes/origin/main\n\
                          2223 refs/remotes/origin/HEAD\n\
                          3333 refs/tags/v1.0\n\
                          ^4444\n"
                .into(),
            config: "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = x\n[remote \"up\"]\n"
                .into(),
            heads: vec!["feature/x".into(), "main".into()],
            remotes: vec!["origin/dev".into()],
            tags: vec![],
        };
        let refs = git_refs(&files);
        assert_eq!(refs.current.as_deref(), Some("feature/x"));
        assert_eq!(refs.branches, ["feature/x", "main"]);
        assert_eq!(refs.remote_branches, ["origin/dev", "origin/main"]);
        assert_eq!(refs.tags, ["v1.0"]);
        assert_eq!(refs.remotes, ["origin", "up"]);
        let branches = git_values(&refs, ArgKind::Branch);
        assert_eq!(names(&branches)[..3], ["feature/x", "main", "origin/dev"]);
        assert!(names(&git_values(&refs, ArgKind::Ref)).contains(&"v1.0"));
        assert_eq!(names(&git_values(&refs, ArgKind::Remote)), ["origin", "up"]);
        // Detached HEAD
        let refs = git_refs(&GitFiles {
            head: "abcdef\n".into(),
            ..GitFiles::default()
        });
        assert_eq!(refs.current, None);
    }

    #[test]
    fn package_json_scripts() {
        let v = package_scripts(r#"{"name":"x","scripts":{"dev":"vite","test":"vitest run"}}"#);
        assert_eq!(names(&v), ["dev", "test"]);
        assert_eq!(v[1].description.as_deref(), Some("vitest run"));
        assert!(package_scripts("not json").is_empty());
        assert!(package_scripts(r#"{"scripts":[1]}"#).is_empty());
    }

    #[test]
    fn deno_tasks_with_comments() {
        let v = deno_tasks(
            "{\n  // tasks\n  \"tasks\": {\"dev\": \"deno run main.ts\", \"x\": {\"command\": \"y\"}}\n}",
        );
        assert_eq!(names(&v), ["dev", "x"]);
        assert_eq!(v[1].description.as_deref(), Some("y"));
    }

    #[test]
    fn makefile_targets() {
        let mk = "CC := gcc\nVERSION = 1\n.PHONY: all test\nall: build ## Build everything\n\
                  build test: deps\n\t$(CC) -o x\n%.o: %.c\n\tcc\n$(OUT): x\n# comment: no\n";
        let v = make_targets(mk);
        assert_eq!(names(&v), ["all", "build", "test"]);
        assert_eq!(v[0].description.as_deref(), Some("Build everything"));
    }

    #[test]
    fn justfile_recipes() {
        let j = "set shell := [\"bash\"]\nalias b := build\n# Build it\nbuild target='x':\n  cargo build\n\n@test *args: build\n  cargo test\nversion := \"1\"\n[private]\nhidden:\n  true\n";
        let v = just_recipes(j);
        assert_eq!(names(&v), ["build", "test", "hidden"]);
        assert_eq!(v[0].description.as_deref(), Some("Build it"));
    }

    #[test]
    fn cargo_toml_targets() {
        let t = "[package]\nname = \"app\"\n[[bin]]\nname = \"cli\"\n[workspace]\nmembers = [\n  \"crates/core\",\n  \"crates/*\",\n]\n[dependencies]\nname = \"nope\"\n";
        assert_eq!(names(&cargo_targets(t)), ["app", "cli", "core"]);
    }

    #[test]
    fn users_and_groups() {
        let p = "root:x:0:0:root:/root:/bin/bash\n\
                 daemon:x:1:1:daemon:/usr/sbin:/usr/sbin/nologin\n\
                 dana:x:1000:1000:Dana Scully,,,:/home/dana:/bin/zsh\n\
                 broken\n";
        let v = passwd_users(p);
        assert_eq!(names(&v), ["root", "dana", "daemon"]);
        assert_eq!(v[1].description.as_deref(), Some("Dana Scully"));
        assert_eq!(
            names(&group_names("root:x:0:\nsudo:x:27:dana\n")),
            ["root", "sudo"]
        );
    }

    #[test]
    fn ssh_hosts() {
        let cfg = "Host *\n  ServerAliveInterval 30\nHost web web-alias\n  HostName 10.0.0.5\n\
                   Host db\n  Hostname=db.internal\nMatch host x\n  HostName nope\nHost !bad *.corp\n";
        let v = ssh_config_hosts(cfg);
        assert_eq!(names(&v), ["web", "web-alias", "db"]);
        assert_eq!(v[1].description.as_deref(), Some("10.0.0.5"));
        assert_eq!(v[2].description.as_deref(), Some("db.internal"));
        let kh = "github.com,140.82.121.4 ssh-ed25519 AAAA\n[git.example]:2222 ssh-rsa AAAA\n\
                  |1|hash= ssh-rsa AAAA\n@cert-authority *.corp ssh-rsa AAAA\n";
        assert_eq!(
            names(&known_hosts_names(kh)),
            ["github.com", "140.82.121.4", "git.example"]
        );
        let hosts =
            "127.0.0.1 localhost\n::1 ip6-localhost ip6-loopback\n10.0.0.2 nas nas.lan # home\n";
        let v = etc_hosts(hosts);
        assert_eq!(names(&v), ["nas", "nas.lan"]);
        assert_eq!(v[0].description.as_deref(), Some("10.0.0.2"));
    }

    #[test]
    fn services() {
        let v = service_names([
            "nginx.service",
            "getty@.service",
            "apt-daily.timer",
            "multi-user.target.wants",
            "ssh",
            "README",
            "com.example.agent.plist",
            ".hidden",
        ]);
        assert_eq!(
            names(&v),
            ["apt-daily.timer", "com.example.agent", "nginx", "ssh"]
        );
    }

    #[test]
    fn env_and_signals() {
        let v = env_values([
            ("MY_VAR", Some("1")),
            ("bad-name", None),
            ("HOME", Some("/h")),
        ]);
        assert_eq!(names(&v)[..2], ["MY_VAR", "HOME"]);
        assert_eq!(v[1].description.as_deref(), Some("/h"));
        assert!(names(&v).contains(&"PATH"));
        assert!(names(&signals()).contains(&"KILL"));
    }

    #[test]
    fn sources_depend_on_kind_and_command() {
        let p = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            sources_for(ArgKind::Script, &p(&["npm", "run"])),
            [Source::ProjectFile {
                name: "package.json".into()
            }]
        );
        assert!(matches!(
            sources_for(ArgKind::Script, &p(&["make"]))[0],
            Source::ProjectFile { ref name } if name == "Makefile"
        ));
        assert_eq!(
            sources_for(ArgKind::Branch, &p(&["git"])),
            [Source::GitRepo]
        );
        assert!(sources_for(ArgKind::Text, &p(&["echo"])).is_empty());
    }

    #[test]
    fn parsers_survive_garbage() {
        let junk = "\u{0}\u{ffff}::::\n\t\t:\n[remote \"\n=:=\n##\n";
        let _ = make_targets(junk);
        let _ = just_recipes(junk);
        let _ = cargo_targets(junk);
        let _ = passwd_users(junk);
        let _ = ssh_config_hosts(junk);
        let _ = known_hosts_names(junk);
        let _ = etc_hosts(junk);
        let _ = git_refs(&GitFiles {
            head: junk.into(),
            packed_refs: junk.into(),
            config: junk.into(),
            ..GitFiles::default()
        });
        assert_eq!(shorten("abcdef", 4), "abc…");
    }
}
