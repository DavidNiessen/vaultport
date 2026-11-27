// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic risk rating for shell command lines.
//!
//! [`assess_risk`] tokenizes the line, splits it into simple commands, unwraps prefixes such
//! as `sudo`, `env`, `nohup`, `timeout` or `xargs`, looks into `sh -c '…'`, `ssh host '…'` and
//! `eval`, and applies a fixed rule set (destructive deletes, disk writes, formatting,
//! permission changes, power state, firewall flushes, SQL drops, force pushes, cluster and
//! infrastructure deletes, piping downloads into a shell, fork bombs, …).
//!
//! The rating is a *hint* for a confirmation prompt, not a security boundary: shell syntax is
//! too rich to analyze completely, so absence of findings does not prove a command is safe.

use crate::tokenize::parse_commands;
use serde::{Deserialize, Serialize};

/// Inputs longer than this are only analyzed up to this many bytes.
const MAX_INPUT: usize = 64 * 1024;
/// Maximum nesting of `sh -c`, `ssh host cmd` and `eval` that is followed.
const MAX_DEPTH: usize = 3;

/// Severity, ordered from harmless to catastrophic.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    #[default]
    Low,
    Medium,
    High,
    Critical,
}

/// Result of [`assess_risk`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Risk {
    /// Highest level among the findings, `Low` if there are none.
    pub level: RiskLevel,
    /// Human-readable findings, in order of discovery, without duplicates.
    pub reasons: Vec<String>,
}

impl Risk {
    fn add(&mut self, level: RiskLevel, reason: impl Into<String>) {
        let reason = reason.into();
        self.level = self.level.max(level);
        if !self.reasons.contains(&reason) {
            self.reasons.push(reason);
        }
    }
}

/// Rates `command`. See the [module docs](self).
#[must_use]
pub fn assess_risk(command: &str) -> Risk {
    let mut end = command.len().min(MAX_INPUT);
    while !command.is_char_boundary(end) {
        end -= 1;
    }
    let mut risk = Risk::default();
    assess_line(&command[..end], 0, &mut risk);
    risk
}

fn assess_line(line: &str, depth: usize, risk: &mut Risk) {
    use RiskLevel::{Critical, High};
    if has_fork_bomb(line) {
        risk.add(Critical, "fork bomb: exhausts the process table");
    }
    sql_rules(line, risk);
    let cmds = parse_commands(line);
    let mut prev_program: Option<String> = None;
    let mut has_download = false;
    let mut has_shell = false;
    for cmd in &cmds {
        for r in &cmd.redirections {
            redirect_rule(&r.op, &r.target, risk);
        }
        let words = unwrap_wrappers(&cmd.words);
        let Some(first) = words.first() else {
            prev_program = None;
            continue;
        };
        let program = basename(first);
        let args = &words[1..];
        if is_download(program) {
            has_download = true;
        }
        if is_shell(program) {
            has_shell = true;
            if cmd.piped_from_previous && prev_program.as_deref().is_some_and(is_download) {
                risk.add(
                    High,
                    "pipes a downloaded script straight into an interpreter",
                );
            }
        }
        program_rules(program, args, risk);
        if depth < MAX_DEPTH {
            for nested in nested_commands(program, args) {
                assess_line(&nested, depth + 1, risk);
            }
        }
        prev_program = Some(program.to_owned());
    }
    let substitution = line.contains("$(") || line.contains("<(") || line.contains('`');
    let fetches = has_download || line.contains("curl ") || line.contains("wget ");
    if has_shell && substitution && fetches {
        risk.add(High, "runs a downloaded script through a shell");
    }
}

fn basename(w: &str) -> &str {
    w.rsplit('/').next().unwrap_or(w)
}

fn is_download(p: &str) -> bool {
    matches!(p, "curl" | "wget" | "fetch")
}

fn is_shell(p: &str) -> bool {
    matches!(
        p,
        "sh" | "bash"
            | "zsh"
            | "dash"
            | "ksh"
            | "fish"
            | "python"
            | "python3"
            | "perl"
            | "ruby"
            | "node"
    )
}

fn is_assignment(w: &str) -> bool {
    match w.split_once('=') {
        Some((name, _)) => {
            !name.is_empty()
                && name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        None => false,
    }
}

/// Strips leading assignments and wrapper commands (`sudo -u x`, `env A=1`, `nohup`, …).
fn unwrap_wrappers(words: &[String]) -> &[String] {
    let mut w = words;
    for _ in 0..8 {
        while w.first().is_some_and(|s| is_assignment(s)) {
            w = &w[1..];
        }
        let Some(first) = w.first() else { return w };
        let (opts_with_arg, positional): (&[&str], usize) = match basename(first) {
            "sudo" | "doas" => (&["-u", "-g", "-h", "-p", "-C", "-D", "-r", "-t", "-U"], 0),
            "nice" => (&["-n"], 0),
            "ionice" => (&["-c", "-n", "-p"], 0),
            "timeout" => (&["-s", "-k", "--signal", "--kill-after"], 1),
            "stdbuf" | "nohup" | "time" | "exec" | "command" | "builtin" | "env" | "xargs"
            | "watch" | "unbuffer" => (&["-n", "-d", "-I", "-L", "-P", "-s", "-u"], 0),
            _ => return w,
        };
        w = &w[1..];
        while let Some(s) = w.first() {
            if s == "--" {
                w = &w[1..];
                break;
            }
            if !s.starts_with('-') || s == "-" {
                break;
            }
            let takes_arg = opts_with_arg.contains(&s.as_str());
            w = &w[1..];
            if takes_arg && !w.is_empty() {
                w = &w[1..];
            }
        }
        if positional > 0 && !w.is_empty() {
            w = &w[positional.min(w.len())..];
        }
    }
    w
}

/// Command strings executed by `sh -c`, `ssh host …`, `su -c` and `eval`.
fn nested_commands(program: &str, args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    if is_shell(program) || program == "su" {
        if let Some(i) = args.iter().position(|a| a == "-c" || a == "--command")
            && let Some(s) = args.get(i + 1)
        {
            out.push(s.clone());
        }
    } else if program == "eval" {
        out.push(args.join(" "));
    } else if matches!(program, "ssh" | "mosh") {
        const WITH_ARG: &[&str] = &[
            "-b", "-c", "-D", "-E", "-e", "-F", "-I", "-i", "-J", "-L", "-l", "-m", "-O", "-o",
            "-p", "-Q", "-R", "-S", "-W", "-w", "-B",
        ];
        let mut i = 0;
        while i < args.len() {
            let a = args[i].as_str();
            if a.starts_with('-') {
                i += if WITH_ARG.contains(&a) { 2 } else { 1 };
                continue;
            }
            // `a` is the destination; the rest is the remote command.
            if i + 1 < args.len() {
                out.push(args[i + 1..].join(" "));
            }
            break;
        }
    }
    out
}

fn short_flags(args: &[String]) -> impl Iterator<Item = char> + '_ {
    args.iter()
        .take_while(|a| *a != "--")
        .filter(|a| a.starts_with('-') && !a.starts_with("--"))
        .flat_map(|a| a.chars().skip(1))
}

fn has_flag(args: &[String], short: char, long: &str) -> bool {
    short_flags(args).any(|c| c == short) || args.iter().any(|a| a == long)
}

fn positionals(args: &[String]) -> Vec<&str> {
    let mut out = Vec::new();
    let mut after_dd = false;
    for a in args {
        if !after_dd && a == "--" {
            after_dd = true;
        } else if after_dd || !a.starts_with('-') || a == "-" {
            out.push(a.as_str());
        }
    }
    out
}

const SYSTEM_DIRS: &[&str] = &[
    "/bin", "/boot", "/dev", "/etc", "/home", "/lib", "/lib32", "/lib64", "/opt", "/proc", "/root",
    "/sbin", "/srv", "/sys", "/usr", "/var", "/Users", "/System", "/Library",
];

/// Whether `t` names the root, home, current/parent directory or a top-level system dir.
fn is_sweeping_target(t: &str) -> bool {
    let t = t.trim_end_matches('*');
    let t = if t.len() > 1 {
        t.trim_end_matches('/')
    } else {
        t
    };
    matches!(
        t,
        "/" | "~" | "$HOME" | "${HOME}" | "." | ".." | "./" | "../" | ".*" | "/."
    ) || SYSTEM_DIRS.contains(&t)
}

fn has_glob(t: &str) -> bool {
    t.contains(['*', '?', '['])
}

fn is_block_device(t: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "/dev/sd",
        "/dev/hd",
        "/dev/vd",
        "/dev/xvd",
        "/dev/nvme",
        "/dev/mmcblk",
        "/dev/disk",
        "/dev/mapper/",
        "/dev/md",
        "/dev/dm-",
        "/dev/loop",
        "/dev/rdisk",
    ];
    PREFIXES.iter().any(|p| t.starts_with(p))
}

fn redirect_rule(op: &str, target: &str, risk: &mut Risk) {
    use RiskLevel::{Critical, High, Medium};
    if !op.contains('>') {
        return;
    }
    if is_block_device(target) {
        risk.add(Critical, format!("overwrites the block device {target}"));
    } else if matches!(
        target,
        "/etc/passwd" | "/etc/shadow" | "/etc/sudoers" | "/etc/fstab" | "/etc/group"
    ) || target.starts_with("/boot/")
    {
        risk.add(
            High,
            format!("overwrites the critical system file {target}"),
        );
    } else if target.starts_with("/etc/") && !op.contains(">>") {
        risk.add(Medium, format!("truncates the system file {target}"));
    }
}

fn program_rules(program: &str, args: &[String], risk: &mut Risk) {
    use RiskLevel::{Critical, High, Low, Medium};
    let sub = positionals(args);
    let sub0 = sub.first().copied().unwrap_or("");
    match program {
        "rm" => rm_rule(args, risk),
        "dd" => {
            if let Some(of) = args.iter().find_map(|a| a.strip_prefix("of=")) {
                if of.starts_with("/dev/") && of != "/dev/null" {
                    risk.add(Critical, format!("dd writes raw data to the device {of}"));
                } else {
                    risk.add(Low, "dd writes raw data to a file");
                }
            }
        }
        p if p.starts_with("mkfs")
            || matches!(p, "mke2fs" | "mkswap" | "wipefs" | "blkdiscard") =>
        {
            risk.add(Critical, format!("{p} destroys the data on a device"));
        }
        "fdisk" | "sfdisk" | "cfdisk" | "parted" | "gdisk" | "sgdisk" => {
            risk.add(High, format!("{program} edits a partition table"));
        }
        "shred" => {
            if sub.iter().any(|t| t.starts_with("/dev/")) {
                risk.add(Critical, "shred overwrites a device");
            } else {
                risk.add(High, "shred irrecoverably destroys files");
            }
        }
        "chmod" => {
            let recursive = has_flag(args, 'R', "--recursive");
            let world = sub0.trim_start_matches('0') == "777"
                || matches!(sub0, "a+rwx" | "ugo+rwx" | "o+w" | "a+w");
            let targets = sub.get(1..).unwrap_or_default();
            if recursive && targets.iter().any(|t| is_sweeping_target(t)) {
                risk.add(Critical, "recursively changes permissions of a system tree");
            } else if recursive && world {
                risk.add(High, "recursively makes files world-writable");
            } else if world {
                risk.add(Medium, "makes files world-writable");
            }
        }
        "chown" | "chgrp" => {
            let targets = sub.get(1..).unwrap_or_default();
            if has_flag(args, 'R', "--recursive") && targets.iter().any(|t| is_sweeping_target(t)) {
                risk.add(Critical, "recursively changes ownership of a system tree");
            }
        }
        "shutdown" | "reboot" | "poweroff" | "halt" => {
            risk.add(High, format!("{program} takes the host offline"));
        }
        "init" | "telinit" if matches!(sub0, "0" | "6") => {
            risk.add(High, "changes the runlevel to halt or reboot");
        }
        "systemctl" => systemctl_rule(&sub, risk),
        "iptables" | "ip6tables" => {
            if args
                .iter()
                .any(|a| matches!(a.as_str(), "-F" | "--flush" | "-X" | "--delete-chain"))
            {
                risk.add(High, "flushes firewall rules");
            }
            if let Some(i) = args.iter().position(|a| a == "-P" || a == "--policy")
                && args
                    .get(i + 2)
                    .is_some_and(|p| p == "DROP" || p == "REJECT")
            {
                risk.add(High, "sets a default DROP policy (may lock you out)");
            }
        }
        "nft" if sub0 == "flush" => risk.add(High, "flushes nftables rules"),
        "ufw" => match sub0 {
            "disable" | "reset" => {
                risk.add(High, format!("ufw {sub0} removes firewall protection"));
            }
            "default" if sub.contains(&"deny") => {
                risk.add(
                    Medium,
                    "changes the default firewall policy (may lock you out)",
                );
            }
            _ => {}
        },
        "git" => git_rule(&sub, args, risk),
        "kubectl" | "oc" => match sub0 {
            "delete" => {
                let sweeping = sub.get(1).is_some_and(|k| {
                    matches!(*k, "ns" | "namespace" | "namespaces" | "pv" | "crd")
                }) || args
                    .iter()
                    .any(|a| a == "--all" || a == "-A" || a == "--all-namespaces");
                if sweeping {
                    risk.add(Critical, "deletes namespaces or all resources in a cluster");
                } else {
                    risk.add(High, "deletes cluster resources");
                }
            }
            "drain" => risk.add(High, "evicts all workloads from a node"),
            "scale" if args.iter().any(|a| a == "--replicas=0") => {
                risk.add(Medium, "scales a workload to zero");
            }
            _ => {}
        },
        "helm" if matches!(sub0, "uninstall" | "delete" | "del" | "un") => {
            risk.add(High, "uninstalls a Helm release");
        }
        "terraform" | "tofu" | "terragrunt" => match sub0 {
            "destroy" => risk.add(Critical, "destroys all managed infrastructure"),
            "apply" if args.iter().any(|a| a.starts_with("-auto-approve")) => {
                risk.add(Medium, "applies infrastructure changes without review");
            }
            "state" if sub.get(1) == Some(&"rm") => {
                risk.add(Medium, "removes resources from Terraform state");
            }
            _ => {}
        },
        "docker" | "podman" => docker_rule(&sub, args, risk),
        "kill" => {
            let nine = args
                .iter()
                .any(|a| matches!(a.as_str(), "-9" | "-KILL" | "-SIGKILL" | "-s"));
            let all = args.len() >= 2 && args.last().is_some_and(|a| a == "-1");
            if all || sub.contains(&"-1") || sub.contains(&"1") {
                risk.add(Critical, "kills init or every process you own");
            } else if nine {
                risk.add(Low, "force-kills a process");
            }
        }
        "killall" | "pkill" => risk.add(Medium, format!("{program} kills processes by name")),
        "crontab" if has_flag(args, 'r', "--remove") => {
            risk.add(High, "removes the whole crontab without confirmation");
        }
        "mv" if sub.last() == Some(&"/dev/null") => risk.add(High, "moves files into /dev/null"),
        "find" => {
            let root = sub.first().is_some_and(|t| is_sweeping_target(t));
            if args.iter().any(|a| a == "-delete") {
                risk.add(
                    if root { Critical } else { High },
                    "find deletes matching files",
                );
            } else if args.iter().any(|a| a == "-exec" || a == "-execdir")
                && args.iter().any(|a| a == "rm")
            {
                risk.add(Medium, "find removes matching files");
            }
        }
        "dropdb" => risk.add(Critical, "drops a PostgreSQL database"),
        "mysqladmin" if sub.contains(&"drop") => risk.add(Critical, "drops a MySQL database"),
        "redis-cli" => {
            if args
                .iter()
                .any(|a| a.eq_ignore_ascii_case("flushall") || a.eq_ignore_ascii_case("flushdb"))
            {
                risk.add(High, "deletes all keys in Redis");
            }
        }
        "aws" | "gcloud" | "az" => {
            if args.iter().any(|a| {
                a.starts_with("delete")
                    || a.starts_with("terminate")
                    || matches!(a.as_str(), "rb" | "rm")
            }) {
                let recursive = args.iter().any(|a| a == "--recursive" || a == "--force");
                risk.add(
                    if recursive { Critical } else { High },
                    "deletes cloud resources",
                );
            }
        }
        "userdel" | "deluser" => risk.add(Medium, "deletes a user account"),
        "rsync" if args.iter().any(|a| a.starts_with("--delete")) => {
            risk.add(Medium, "rsync deletes files at the destination");
        }
        "apt" | "apt-get" | "dnf" | "yum" | "zypper"
            if matches!(sub0, "remove" | "purge" | "autoremove" | "erase") =>
        {
            risk.add(Medium, "removes installed packages");
        }
        "pacman" if args.iter().any(|a| a.starts_with("-R")) => {
            risk.add(Medium, "removes installed packages");
        }
        _ => {}
    }
}

fn rm_rule(args: &[String], risk: &mut Risk) {
    use RiskLevel::{Critical, High, Low, Medium};
    let recursive = has_flag(args, 'r', "--recursive") || has_flag(args, 'R', "--recursive");
    let force = has_flag(args, 'f', "--force");
    if args.iter().any(|a| a == "--no-preserve-root") {
        risk.add(
            Critical,
            "rm --no-preserve-root disables the root safeguard",
        );
    }
    let targets = positionals(args);
    if recursive {
        if let Some(t) = targets.iter().find(|t| is_sweeping_target(t)) {
            risk.add(Critical, format!("recursively deletes {t}"));
        } else if targets.is_empty() {
            risk.add(High, "recursively deletes paths taken from input");
        } else if force && targets.iter().any(|t| has_glob(t)) {
            risk.add(High, "force-deletes a glob recursively");
        } else {
            risk.add(Medium, "deletes directories recursively");
        }
    } else if targets.iter().any(|t| has_glob(t)) {
        risk.add(Medium, "deletes files matching a glob");
    } else {
        risk.add(Low, "deletes files");
    }
}

fn systemctl_rule(sub: &[&str], risk: &mut Risk) {
    use RiskLevel::{High, Medium};
    let Some(&verb) = sub.first() else { return };
    match verb {
        "poweroff" | "reboot" | "halt" | "kexec" | "suspend" | "hibernate" | "rescue"
        | "emergency" => risk.add(High, format!("systemctl {verb} takes the host offline")),
        "stop" | "disable" | "mask" | "kill" | "restart" => {
            let units = &sub[1..];
            let lifeline = units.iter().any(|u| {
                let u = u.trim_end_matches(".service");
                matches!(
                    u,
                    "ssh"
                        | "sshd"
                        | "networking"
                        | "network"
                        | "NetworkManager"
                        | "systemd-networkd"
                        | "systemd-resolved"
                        | "dbus"
                )
            });
            if lifeline && verb != "restart" {
                risk.add(High, format!("systemctl {verb} may cut off remote access"));
            } else if lifeline {
                risk.add(
                    Medium,
                    "restarting a network or SSH service may drop the session",
                );
            } else if verb != "restart" {
                risk.add(Medium, format!("systemctl {verb} affects running services"));
            }
        }
        "isolate" => risk.add(Medium, "switches the system target"),
        _ => {}
    }
}

fn git_rule(sub: &[&str], args: &[String], risk: &mut Risk) {
    use RiskLevel::{High, Medium};
    match sub.first().copied().unwrap_or("") {
        "push" => {
            let lease = args.iter().any(|a| a.starts_with("--force-with-lease"));
            let force = args.iter().any(|a| {
                a == "--force"
                    || a == "-f"
                    || (a.starts_with('-') && !a.starts_with("--") && a.contains('f'))
            }) || sub.iter().skip(2).any(|r| r.starts_with('+'));
            if force {
                risk.add(High, "force push rewrites remote history");
            } else if lease {
                risk.add(Medium, "force push (with lease) rewrites remote history");
            }
            if args.iter().any(|a| a == "--delete" || a == "-d")
                || sub.iter().skip(2).any(|r| r.starts_with(':'))
            {
                risk.add(Medium, "deletes a remote branch or tag");
            }
        }
        "reset" if args.iter().any(|a| a == "--hard") => {
            risk.add(Medium, "git reset --hard discards local changes");
        }
        "clean" if has_flag(args, 'f', "--force") => {
            risk.add(Medium, "git clean deletes untracked files");
        }
        "branch" if args.iter().any(|a| a == "-D") => {
            risk.add(Medium, "force-deletes a branch");
        }
        "filter-branch" | "filter-repo" => risk.add(Medium, "rewrites repository history"),
        _ => {}
    }
}

fn docker_rule(sub: &[&str], args: &[String], risk: &mut Risk) {
    use RiskLevel::{High, Medium};
    let all = args
        .iter()
        .any(|a| a == "-a" || a == "--all" || a == "--volumes");
    match (sub.first().copied(), sub.get(1).copied()) {
        (Some("system"), Some("prune")) => {
            risk.add(if all { High } else { Medium }, "prunes Docker data");
        }
        (Some("volume"), Some("rm" | "prune")) => risk.add(High, "deletes Docker volumes (data)"),
        (Some("rm" | "rmi"), _) if has_flag(args, 'f', "--force") => {
            risk.add(Medium, "force-removes containers or images");
        }
        (Some("compose"), Some("down")) if args.iter().any(|a| a == "-v" || a == "--volumes") => {
            risk.add(High, "removes compose volumes (data)");
        }
        _ => {}
    }
}

/// Detects `name(){ name|name& };name` with any function name, ignoring whitespace.
fn has_fork_bomb(line: &str) -> bool {
    let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    let mut search = 0;
    while let Some(pos) = compact[search..].find("(){") {
        let at = search + pos;
        let name_start = compact[..at]
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_alphanumeric() || matches!(c, '_' | ':' | '.' | '-'))
            .last()
            .map_or(at, |(i, _)| i);
        let name = &compact[name_start..at];
        if !name.is_empty() {
            let body = &compact[at + 3..];
            if body.contains(&format!("{name}|{name}&")) {
                return true;
            }
        }
        search = at + 3;
    }
    false
}

fn sql_rules(line: &str, risk: &mut Risk) {
    use RiskLevel::{Critical, High};
    // Cheap pre-filter: most lines contain no SQL.
    let lower = line.to_ascii_lowercase();
    if !["drop", "truncate", "delete", "flushall"]
        .iter()
        .any(|k| lower.contains(k))
    {
        return;
    }
    let norm: String = lower
        .split(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '`')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let norm = format!(" {norm} ");
    if norm.contains(" drop database ") || norm.contains(" drop schema ") {
        risk.add(
            Critical,
            "SQL DROP DATABASE/SCHEMA deletes a whole database",
        );
    }
    if norm.contains(" drop table ") {
        risk.add(High, "SQL DROP TABLE deletes a table");
    }
    if norm.contains(" truncate ") {
        risk.add(High, "SQL TRUNCATE deletes all rows");
    }
    if let Some(i) = norm.find(" delete from ") {
        let rest = &norm[i..];
        let stmt = rest.split(';').next().unwrap_or(rest);
        if !stmt.contains(" where ") {
            risk.add(High, "SQL DELETE without WHERE deletes all rows");
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use RiskLevel::*;
    use proptest::prelude::*;

    fn level(c: &str) -> RiskLevel {
        assess_risk(c).level
    }

    #[test]
    fn critical_commands() {
        for c in [
            "rm -rf /",
            "rm -rf /*",
            "sudo rm -fr ~",
            "rm -r -f $HOME",
            "rm -rf --no-preserve-root /x",
            "rm -Rf /etc/",
            "rm -rf .",
            "dd if=/dev/zero of=/dev/sda bs=1M",
            "sudo mkfs.ext4 /dev/nvme0n1p1",
            "mkswap /dev/sdb2",
            ":(){ :|:& };:",
            "bomb() { bomb | bomb & }; bomb",
            "echo x > /dev/sda",
            "cat image.iso >/dev/disk2",
            "chmod -R 777 /",
            "chown -R nobody /usr",
            "psql -c 'DROP DATABASE prod'",
            "mysql -e \"drop   database  app\"",
            "kubectl delete ns production",
            "kubectl delete pods --all -A",
            "terraform destroy -auto-approve",
            "kill -9 -1",
            "find / -name '*.log' -delete",
            "sh -c 'rm -rf /'",
            "ssh -p 22 root@db 'rm -rf /var'",
            "env FOO=1 nohup timeout 10 rm -rf /",
            "aws s3 rm s3://bucket --recursive",
        ] {
            assert_eq!(level(c), Critical, "{c}: {:?}", assess_risk(c));
        }
    }

    #[test]
    fn high_commands() {
        for c in [
            "rm -rf build/*",
            "rm -rf *",
            "find . -name '*.tmp' | xargs rm -rf",
            "curl -fsSL https://example.com/install.sh | sh",
            "wget -qO- https://x | sudo bash -s",
            "bash <(curl -s https://x)",
            "sh -c \"$(curl -fsSL https://x)\"",
            "chmod -R 777 ./uploads",
            "sudo shutdown -h now",
            "reboot",
            "systemctl poweroff",
            "sudo systemctl stop sshd",
            "iptables -F",
            "iptables -P INPUT DROP",
            "ufw disable",
            "git push --force origin main",
            "git push -f",
            "git push origin +main",
            "kubectl delete deployment web",
            "helm uninstall web",
            "redis-cli FLUSHALL",
            "psql -c 'drop table users'",
            "mysql -e 'DELETE FROM users'",
            "crontab -r",
            "docker volume prune",
            "echo root::0:0::/:/bin/sh > /etc/passwd",
        ] {
            assert_eq!(level(c), High, "{c}: {:?}", assess_risk(c));
        }
    }

    #[test]
    fn medium_commands() {
        for c in [
            "rm -r old",
            "rm -rf node_modules",
            "rm *.log",
            "chmod 777 file",
            "git push --force-with-lease",
            "git reset --hard HEAD~1",
            "git clean -fdx",
            "systemctl stop nginx",
            "sudo apt purge nginx",
            "mysql -e 'DELETE FROM users WHERE id = 3'; pkill node",
            "terraform apply -auto-approve",
            "docker system prune",
            "> /etc/motd",
        ] {
            assert_eq!(level(c), Medium, "{c}: {:?}", assess_risk(c));
        }
    }

    #[test]
    fn low_commands() {
        for c in [
            "ls -la",
            "git push origin main",
            "git status",
            "rm file.txt",
            "dd if=/dev/zero of=./disk.img bs=1M count=10",
            "echo hello > /dev/null",
            "kubectl get pods -A",
            "terraform plan",
            "curl https://example.com -o page.html",
            "cat /etc/passwd",
            "echo 'rm -rf /' # just text in quotes",
            "grep -r drop src/",
            "systemctl status sshd",
            "kill -1 1234",
            "systemctl restart nginx",
            "",
        ] {
            let r = assess_risk(c);
            assert_eq!(r.level, Low, "{c}: {r:?}");
        }
        // Low but with an informative reason.
        assert_eq!(assess_risk("rm file.txt").reasons, ["deletes files"]);
        assert!(assess_risk("ls").reasons.is_empty());
    }

    #[test]
    fn reasons_are_deduplicated_and_level_is_max() {
        let r = assess_risk("rm -rf / ; rm -rf / ; reboot");
        assert_eq!(r.level, Critical);
        assert_eq!(r.reasons.len(), 2);
    }

    #[test]
    fn serde_shape() {
        let r = assess_risk("reboot");
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(
            json,
            r#"{"level":"high","reasons":["reboot takes the host offline"]}"#
        );
    }

    proptest! {
        #[test]
        fn never_panics(s in "\\PC{0,120}") {
            let _ = assess_risk(&s);
        }
    }
}
