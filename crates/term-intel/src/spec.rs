// SPDX-License-Identifier: GPL-3.0-or-later
//! Command specifications for completion.
//!
//! The built-in specs live in `specs/commands.json` (embedded with `include_str!`; JSON has no
//! comments, so its license is declared here: the file is part of this crate and licensed
//! GPL-3.0-or-later). The format is deliberately compact:
//!
//! ```json
//! { "version": 1, "commands": [
//!   { "name": "git", "aliases": ["g"], "desc": "…", "wrapper": false,
//!     "opts": [ { "name": "-C", "desc": "…", "arg": "directory", "repeat": false } ],
//!     "args": [ "path..." ],
//!     "subs": [ { "name": "remove rm", "desc": "…", "args": ["remote"] } ] } ] }
//! ```
//!
//! * `name` of options and subcommands is a space-separated list of spellings; the first one
//!   is canonical (`"-m --message"`, `"remove rm"`). Top-level commands may also list
//!   `aliases`.
//! * Argument specs (`args` entries and option `arg`) are strings: an [`ArgKind`] name such
//!   as `path`, `directory`, `branch`, `service`, `process`; a trailing `...` makes it
//!   variadic (`"file..."`); `enum:a,b,c` lists fixed values.
//! * `wrapper: true` marks commands such as `sudo` whose `command` argument starts a new
//!   command line that is completed with that command's spec.
//!
//! Users can load additional specs with [`SpecSet::from_json`] and [`SpecSet::merge`].

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;

/// Semantic type of an argument, so the app can plug in dynamic generators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ArgKind {
    /// Any file system path.
    Path,
    /// A file (paths, but files are preferred).
    File,
    /// A directory only.
    Directory,
    /// A process id.
    Process,
    /// A system service / unit.
    Service,
    /// A git branch.
    Branch,
    /// Any git ref (branch, tag, commit).
    Ref,
    /// A git remote.
    Remote,
    /// A host name or `user@host`.
    Host,
    User,
    Group,
    /// An installable or installed package.
    Package,
    /// A container name or id.
    Container,
    /// A container image.
    Image,
    /// A Kubernetes pod.
    Pod,
    /// A Kubernetes resource type or `type/name`.
    KubeResource,
    /// A Kubernetes namespace.
    KubeNamespace,
    /// A command name (starts a nested command line).
    Command,
    /// A signal name or number.
    Signal,
    /// A network interface.
    Interface,
    Url,
    Port,
    Number,
    /// Free text: nothing to complete.
    Text,
    /// One of a fixed set of values.
    Enum,
    /// A database name.
    Database,
    /// A terminal multiplexer session.
    Session,
    /// A project script or build target (npm scripts, make targets).
    Script,
}

impl ArgKind {
    /// Parses a kind name as used in spec files.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "path" => Self::Path,
            "file" => Self::File,
            "directory" => Self::Directory,
            "process" => Self::Process,
            "service" => Self::Service,
            "branch" => Self::Branch,
            "ref" => Self::Ref,
            "remote" => Self::Remote,
            "host" => Self::Host,
            "user" => Self::User,
            "group" => Self::Group,
            "package" => Self::Package,
            "container" => Self::Container,
            "image" => Self::Image,
            "pod" => Self::Pod,
            "kube_resource" => Self::KubeResource,
            "kube_namespace" => Self::KubeNamespace,
            "command" => Self::Command,
            "signal" => Self::Signal,
            "interface" => Self::Interface,
            "url" => Self::Url,
            "port" => Self::Port,
            "number" => Self::Number,
            "text" => Self::Text,
            "enum" => Self::Enum,
            "database" => Self::Database,
            "session" => Self::Session,
            "script" => Self::Script,
            _ => return None,
        })
    }

    /// Whether values of this kind are file system paths.
    #[must_use]
    pub const fn is_path(self) -> bool {
        matches!(self, Self::Path | Self::File | Self::Directory)
    }
}

/// An argument slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgSpec {
    pub kind: ArgKind,
    /// Accepts any number of values.
    pub variadic: bool,
    /// Fixed suggestions (for [`ArgKind::Enum`]).
    pub values: Vec<String>,
}

impl TryFrom<String> for ArgSpec {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        if let Some(list) = s.strip_prefix("enum:") {
            let values: Vec<String> = list
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .collect();
            return Ok(Self {
                kind: ArgKind::Enum,
                variadic: false,
                values,
            });
        }
        let (name, variadic) = match s.strip_suffix("...") {
            Some(n) => (n, true),
            None => (s.as_str(), false),
        };
        let kind =
            ArgKind::from_name(name).ok_or_else(|| format!("unknown argument kind `{s}`"))?;
        Ok(Self {
            kind,
            variadic,
            values: Vec::new(),
        })
    }
}

impl<'de> Deserialize<'de> for ArgSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::try_from(s).map_err(serde::de::Error::custom)
    }
}

fn split_names(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

/// An option (flag).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionSpec {
    /// All spellings, canonical first (`["-m", "--message"]`).
    pub names: Vec<String>,
    pub description: String,
    /// The option's value, if it takes one.
    pub arg: Option<ArgSpec>,
    /// May be given more than once.
    pub repeat: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOption {
    name: String,
    #[serde(default)]
    desc: String,
    #[serde(default)]
    arg: Option<ArgSpec>,
    #[serde(default)]
    repeat: bool,
}

impl<'de> Deserialize<'de> for OptionSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let r = RawOption::deserialize(d)?;
        let names = split_names(&r.name);
        if names.is_empty() {
            return Err(serde::de::Error::custom("option without a name"));
        }
        Ok(Self {
            names,
            description: r.desc,
            arg: r.arg,
            repeat: r.repeat,
        })
    }
}

/// A command or subcommand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    /// All spellings, canonical first.
    pub names: Vec<String>,
    pub description: String,
    pub options: Vec<OptionSpec>,
    pub subcommands: Vec<CommandSpec>,
    pub args: Vec<ArgSpec>,
    /// The `command` argument starts a nested command line (`sudo`, `env`, …).
    pub wrapper: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCommand {
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    desc: String,
    #[serde(default)]
    opts: Vec<OptionSpec>,
    #[serde(default)]
    subs: Vec<CommandSpec>,
    #[serde(default)]
    args: Vec<ArgSpec>,
    #[serde(default)]
    wrapper: bool,
}

impl<'de> Deserialize<'de> for CommandSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let r = RawCommand::deserialize(d)?;
        let mut names = split_names(&r.name);
        if names.is_empty() {
            return Err(serde::de::Error::custom("command without a name"));
        }
        names.extend(r.aliases);
        Ok(Self {
            names,
            description: r.desc,
            options: r.opts,
            subcommands: r.subs,
            args: r.args,
            wrapper: r.wrapper,
        })
    }
}

impl CommandSpec {
    /// The canonical name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.names[0]
    }

    /// Finds a subcommand by any of its spellings.
    #[must_use]
    pub fn subcommand(&self, word: &str) -> Option<&CommandSpec> {
        self.subcommands
            .iter()
            .find(|s| s.names.iter().any(|n| n == word))
    }

    /// Finds an option by any of its spellings.
    #[must_use]
    pub fn option(&self, word: &str) -> Option<&OptionSpec> {
        self.options
            .iter()
            .find(|o| o.names.iter().any(|n| n == word))
    }

    /// The argument slot for the `index`-th positional (0-based) argument.
    #[must_use]
    pub fn arg_at(&self, index: usize) -> Option<&ArgSpec> {
        self.args
            .get(index)
            .or_else(|| self.args.last().filter(|a| a.variadic))
    }
}

#[derive(Deserialize)]
struct RawSpecFile {
    #[serde(default)]
    commands: Vec<CommandSpec>,
}

/// A set of command specs indexed by every command name and alias.
#[derive(Debug, Clone, Default)]
pub struct SpecSet {
    commands: Vec<CommandSpec>,
    index: HashMap<String, usize>,
}

static BUILTIN_JSON: &str = include_str!("../specs/commands.json");

static BUILTIN: LazyLock<SpecSet> =
    LazyLock::new(|| SpecSet::from_json(BUILTIN_JSON).unwrap_or_default());

impl SpecSet {
    /// The built-in specs (about 300 common commands).
    #[must_use]
    pub fn builtin() -> &'static SpecSet {
        &BUILTIN
    }

    /// Parses a spec file in the format described in the [module docs](self).
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let raw: RawSpecFile = serde_json::from_str(json)?;
        let mut set = Self::default();
        for c in raw.commands {
            set.insert(c);
        }
        Ok(set)
    }

    /// Adds or replaces a command spec (all of its names point to it afterwards).
    pub fn insert(&mut self, spec: CommandSpec) {
        let existing = self.index.get(spec.name()).copied();
        let i = existing.unwrap_or(self.commands.len());
        for n in &spec.names {
            self.index.insert(n.clone(), i);
        }
        if existing.is_some() {
            self.commands[i] = spec;
        } else {
            self.commands.push(spec);
        }
    }

    /// Adds all specs of `other`, replacing commands with the same names.
    pub fn merge(&mut self, other: SpecSet) {
        for c in other.commands {
            self.insert(c);
        }
    }

    /// Looks up a command by name or alias.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&CommandSpec> {
        self.index.get(name).map(|&i| &self.commands[i])
    }

    /// All command names and aliases with the spec they belong to, sorted by name.
    #[must_use]
    pub fn names(&self) -> Vec<(&str, &CommandSpec)> {
        let mut v: Vec<_> = self
            .index
            .iter()
            .map(|(n, &i)| (n.as_str(), &self.commands[i]))
            .collect();
        v.sort_by(|a, b| a.0.cmp(b.0));
        v
    }

    /// Number of distinct command specs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Whether the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn builtin_specs_parse_and_cover_common_commands() {
        let set = SpecSet::from_json(BUILTIN_JSON).unwrap();
        assert!(set.len() >= 300, "{}", set.len());
        for name in [
            "git",
            "docker",
            "kubectl",
            "systemctl",
            "journalctl",
            "apt",
            "apt-get",
            "dnf",
            "yum",
            "pacman",
            "brew",
            "npm",
            "pnpm",
            "yarn",
            "cargo",
            "ls",
            "cd",
            "cp",
            "mv",
            "rm",
            "mkdir",
            "chmod",
            "chown",
            "tar",
            "grep",
            "find",
            "ssh",
            "scp",
            "rsync",
            "curl",
            "wget",
            "tail",
            "less",
            "cat",
            "ps",
            "kill",
            "top",
            "htop",
            "df",
            "du",
            "free",
            "ip",
            "ss",
            "netstat",
            "ping",
            "tmux",
            "screen",
            "vim",
            "nano",
            "sudo",
            "crontab",
            "ufw",
            "iptables",
            "nginx",
            "psql",
            "mysql",
            "redis-cli",
            "python",
            "pip",
            "make",
            "terraform",
            "helm",
            "aws",
            "python3",
            "vi",
            "podman",
            "head",
            "docker-compose",
            "gh",
            "rg",
            "jq",
            "ffmpeg",
            "uv",
            "go",
            "rustup",
            "bun",
            "deno",
            "xargs",
            "timeout",
            "openssl",
            "open",
            "defaults",
            "launchctl",
            "diskutil",
            "pbcopy",
            "caffeinate",
            "winget",
        ] {
            assert!(set.get(name).is_some(), "missing spec for {name}");
        }
        assert_eq!(SpecSet::builtin().len(), set.len());
    }

    #[test]
    fn spec_details() {
        let git = SpecSet::builtin().get("git").unwrap();
        let commit = git.subcommand("commit").unwrap();
        let m = commit.option("--message").unwrap();
        assert_eq!(m.names, ["-m", "--message"]);
        assert_eq!(m.arg.as_ref().unwrap().kind, ArgKind::Text);
        let rm = git.subcommand("remote").unwrap().subcommand("rm").unwrap();
        assert_eq!(rm.name(), "remove");
        let add = git.subcommand("add").unwrap();
        assert_eq!(add.arg_at(5).unwrap().kind, ArgKind::Path);
        let push = git.subcommand("push").unwrap();
        assert_eq!(push.arg_at(1).unwrap().kind, ArgKind::Branch);
        assert!(push.arg_at(2).is_none());
        assert!(SpecSet::builtin().get("sudo").unwrap().wrapper);
    }

    #[test]
    fn bad_specs_are_rejected() {
        assert!(SpecSet::from_json(r#"{"commands":[{"name":"x","args":["nope"]}]}"#).is_err());
        assert!(SpecSet::from_json(r#"{"commands":[{"name":" "}]}"#).is_err());
        assert!(SpecSet::from_json(r#"{"commands":[{"name":"x","bogus":1}]}"#).is_err());
        let s = SpecSet::from_json(r#"{"commands":[{"name":"x","args":["enum:a, b"]}]}"#).unwrap();
        assert_eq!(s.get("x").unwrap().args[0].values, ["a", "b"]);
    }

    #[test]
    fn merge_overrides() {
        let mut set = SpecSet::builtin().clone();
        let mine = SpecSet::from_json(r#"{"commands":[{"name":"git","desc":"mine"}]}"#).unwrap();
        set.merge(mine);
        assert_eq!(set.get("git").unwrap().description, "mine");
    }
}
