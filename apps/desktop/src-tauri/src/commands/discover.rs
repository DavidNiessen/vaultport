// SPDX-License-Identifier: GPL-3.0-or-later
//! What exists on the machine a terminal runs on, for autocomplete: the commands on `PATH`
//! and argument values (git branches, npm scripts, users, …).
//!
//! Everything is read through a [`FileSystem`]: the local disk for local terminals, and an
//! SFTP channel on the terminal's own SSH connection for remote ones. No command is run on the
//! remote host, so nothing shows up in its process list or shell history. All reads are small,
//! bounded and time-limited; a slow server just yields fewer suggestions.

use std::sync::Arc;
use std::time::Duration;

use sftp_engine::{EntryKind, FileSystem};
use term_intel::generators::{self, GitFiles, Source, Value};
use term_intel::{ArgKind, KnownCommand};
use tokio::io::AsyncReadExt;

/// Time allowed for discovering the commands of a machine.
const DISCOVER_TIMEOUT: Duration = Duration::from_secs(4);
/// Time allowed for reading the files of one generator.
pub const VALUES_TIMEOUT: Duration = Duration::from_millis(600);
/// Directory levels searched upwards for a git directory or a project file.
const MAX_UP: usize = 12;

/// Directories with commands besides `PATH`. On a remote host `PATH` is unknown; on macOS a
/// GUI app gets a minimal `PATH`, so Homebrew and per-user tool directories are added too.
const EXTRA_DIRS: &[&str] = &[
    "/usr/local/sbin",
    "/usr/local/bin",
    "/usr/sbin",
    "/usr/bin",
    "/sbin",
    "/bin",
    "/opt/homebrew/bin",
    "/opt/homebrew/sbin",
    "/opt/local/bin",
    "/snap/bin",
    "/usr/games",
    "/run/current-system/sw/bin",
    "~/.local/bin",
    "~/bin",
    "~/.cargo/bin",
    "~/go/bin",
    "~/.bun/bin",
    "~/.deno/bin",
    "~/.nix-profile/bin",
    "~/.volta/bin",
    "~/.pyenv/shims",
    "~/.rbenv/shims",
    "~/.asdf/shims",
    "~/.local/share/mise/shims",
];

fn expand_home(dir: &str, home: Option<&str>) -> Option<String> {
    match dir.strip_prefix("~/") {
        Some(rest) => home.map(|h| format!("{}/{rest}", h.trim_end_matches('/'))),
        None => Some(dir.to_owned()),
    }
}

/// The directories to search for commands. `path` is the local `PATH` (remote: `None`).
fn command_dirs(path: Option<&str>, home: Option<&str>) -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    if let Some(p) = path {
        let sep = if cfg!(windows) { ';' } else { ':' };
        dirs.extend(p.split(sep).filter(|d| !d.is_empty()).map(str::to_owned));
    }
    if !cfg!(windows) || path.is_none() {
        dirs.extend(EXTRA_DIRS.iter().filter_map(|d| expand_home(d, home)));
    }
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| seen.insert(d.trim_end_matches('/').to_owned()));
    dirs
}

/// The command name for a directory entry, or `None` if it is not a command.
fn command_name(
    name: &str,
    kind: EntryKind,
    permissions: Option<u32>,
    windows: bool,
) -> Option<String> {
    if name.starts_with('.') || name.is_empty() {
        return None;
    }
    match kind {
        EntryKind::File | EntryKind::Symlink => {}
        _ => return None,
    }
    if windows {
        let lower = name.to_ascii_lowercase();
        return [".exe", ".cmd", ".bat", ".com", ".ps1"]
            .iter()
            .find_map(|ext| {
                lower
                    .strip_suffix(ext)
                    .map(|_| name[..name.len() - ext.len()].to_owned())
            });
    }
    // Files without any execute bit are not commands (symlinks report the link's own mode).
    if kind == EntryKind::File && permissions.is_some_and(|p| p & 0o111 == 0) {
        return None;
    }
    Some(name.to_owned())
}

/// Lists the commands on `fs`: executables in the command directories (the first directory
/// that has a name wins, as on `PATH`) and shell builtins.
pub async fn commands(fs: Arc<dyn FileSystem>, local: bool) -> Vec<KnownCommand> {
    let home = fs.home().await.ok();
    let path = if local {
        std::env::var("PATH").ok()
    } else {
        None
    };
    let windows = local && cfg!(windows);
    let dirs = command_dirs(path.as_deref(), home.as_deref());
    let listings = futures_join(dirs.iter().map(|d| {
        let fs = fs.clone();
        let d = d.clone();
        async move { (d.clone(), fs.list(&d).await.ok()) }
    }));
    let listings = tokio::time::timeout(DISCOVER_TIMEOUT, listings)
        .await
        .unwrap_or_default();
    let mut out: Vec<KnownCommand> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (dir, entries) in listings {
        for e in entries.into_iter().flatten() {
            if let Some(name) = command_name(&e.name, e.kind, e.permissions, windows)
                && seen.insert(name.clone())
            {
                out.push(KnownCommand {
                    name,
                    description: Some(format!("{dir}/{}", e.name)),
                });
            }
        }
    }
    for (name, desc) in generators::BUILTINS {
        if seen.insert((*name).to_owned()) {
            out.push(KnownCommand {
                name: (*name).to_owned(),
                description: Some(format!("shell builtin · {desc}")),
            });
        }
    }
    out
}

/// Runs futures concurrently and collects their results in order.
async fn futures_join<F, T>(futs: impl IntoIterator<Item = F>) -> Vec<T>
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let handles: Vec<_> = futs.into_iter().map(tokio::spawn).collect();
    let mut out = Vec::with_capacity(handles.len());
    for h in handles {
        if let Ok(v) = h.await {
            out.push(v);
        }
    }
    out
}

/// Reads at most [`generators::MAX_FILE_BYTES`] of a text file.
pub async fn read_text(fs: &dyn FileSystem, path: &str) -> Option<String> {
    let reader = fs.open_read(path, 0).await.ok()?;
    let mut buf = Vec::new();
    reader
        .take(generators::MAX_FILE_BYTES)
        .read_to_end(&mut buf)
        .await
        .ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn parent(dir: &str) -> Option<String> {
    let t = dir.trim_end_matches('/');
    if t.is_empty() {
        return None;
    }
    let i = t.rfind('/')?;
    Some(if i == 0 {
        "/".to_owned()
    } else {
        t[..i].to_owned()
    })
}

fn join(dir: &str, name: &str) -> String {
    format!("{}/{name}", dir.trim_end_matches('/'))
}

/// `dir` and its ancestors, closest first.
fn ancestors(dir: &str) -> Vec<String> {
    let mut out = vec![dir.trim_end_matches('/').to_owned()];
    if out[0].is_empty() {
        out[0] = "/".into();
    }
    while out.len() < MAX_UP {
        match out.last().and_then(|d| parent(d)) {
            Some(p) => out.push(p),
            None => break,
        }
    }
    out
}

/// Finds `name` in `cwd` or the closest ancestor that has it.
async fn find_up(fs: &dyn FileSystem, cwd: &str, name: &str) -> Option<(String, EntryKind)> {
    for dir in ancestors(cwd) {
        let path = join(&dir, name);
        if let Ok(e) = fs.stat(&path).await {
            return Some((path, e.kind));
        }
    }
    None
}

/// Names under `dir`, recursing `depth` levels into subdirectories (`a/b`).
async fn names_under(fs: &dyn FileSystem, dir: &str, depth: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_owned(), String::new(), depth)];
    while let Some((d, prefix, depth)) = stack.pop() {
        let Ok(entries) = fs.list(&d).await else {
            continue;
        };
        for e in entries {
            let name = format!("{prefix}{}", e.name);
            if e.kind == EntryKind::Dir {
                if depth > 0 {
                    stack.push((join(&d, &e.name), format!("{name}/"), depth - 1));
                }
            } else {
                out.push(name);
            }
            if out.len() >= generators::MAX_VALUES {
                return out;
            }
        }
    }
    out
}

/// Reads the git directory of the repository containing `cwd`.
async fn git_files(fs: &dyn FileSystem, cwd: &str) -> Option<GitFiles> {
    let (path, kind) = find_up(fs, cwd, ".git").await?;
    let git_dir = if kind == EntryKind::Dir {
        path
    } else {
        // A worktree or submodule: `.git` is a file with `gitdir: <path>`.
        let text = read_text(fs, &path).await?;
        let target = text.trim().strip_prefix("gitdir:")?.trim().to_owned();
        if target.starts_with('/') {
            target
        } else {
            join(&parent(&path)?, &target)
        }
    };
    // Worktrees keep refs in the common directory.
    let common = match read_text(fs, &join(&git_dir, "commondir")).await {
        Some(c) if c.trim().starts_with('/') => c.trim().to_owned(),
        Some(c) if !c.trim().is_empty() => join(&git_dir, c.trim()),
        _ => git_dir.clone(),
    };
    let refs = join(&common, "refs");
    let mut remotes = Vec::new();
    if let Ok(entries) = fs.list(&join(&refs, "remotes")).await {
        for e in entries
            .into_iter()
            .filter(|e| e.kind == EntryKind::Dir)
            .take(16)
        {
            for b in names_under(fs, &join(&join(&refs, "remotes"), &e.name), 3).await {
                remotes.push(format!("{}/{b}", e.name));
            }
        }
    }
    Some(GitFiles {
        head: read_text(fs, &join(&git_dir, "HEAD"))
            .await
            .unwrap_or_default(),
        packed_refs: read_text(fs, &join(&common, "packed-refs"))
            .await
            .unwrap_or_default(),
        config: read_text(fs, &join(&common, "config"))
            .await
            .unwrap_or_default(),
        heads: names_under(fs, &join(&refs, "heads"), 4).await,
        remotes,
        tags: names_under(fs, &join(&refs, "tags"), 2).await,
    })
}

/// Values of `kind` for the command `path` in `cwd`, read from `fs`.
pub async fn values(
    fs: &dyn FileSystem,
    cwd: Option<&str>,
    kind: ArgKind,
    path: &[String],
) -> Vec<Value> {
    let mut out = Vec::new();
    let home = || async { fs.home().await.ok() };
    for source in generators::sources_for(kind, path) {
        match source {
            Source::Static => {
                if kind == ArgKind::Signal {
                    out.extend(generators::signals());
                }
            }
            Source::GitRepo => {
                let Some(cwd) = cwd else { continue };
                if let Some(files) = git_files(fs, cwd).await {
                    out.extend(generators::git_values(&generators::git_refs(&files), kind));
                }
            }
            Source::ProjectFile { name } => {
                let Some(cwd) = cwd else { continue };
                let Some((file, _)) = find_up(fs, cwd, &name).await else {
                    continue;
                };
                let Some(text) = read_text(fs, &file).await else {
                    continue;
                };
                let values = match name.as_str() {
                    "package.json" => generators::package_scripts(&text),
                    "composer.json" => generators::composer_scripts(&text),
                    "deno.json" | "deno.jsonc" => generators::deno_tasks(&text),
                    "Cargo.toml" => generators::cargo_targets(&text),
                    "justfile" | "Justfile" | ".justfile" => generators::just_recipes(&text),
                    _ => generators::make_targets(&text),
                };
                out.extend(values);
                // The closest project file wins (e.g. Makefile over makefile).
                if !out.is_empty() {
                    break;
                }
            }
            Source::File { path } => {
                let Some(text) = read_text(fs, &path).await else {
                    continue;
                };
                out.extend(match path.as_str() {
                    "/etc/passwd" => generators::passwd_users(&text),
                    "/etc/group" => generators::group_names(&text),
                    "/etc/hosts" => generators::etc_hosts(&text),
                    _ => Vec::new(),
                });
            }
            Source::HomeFile { path } => {
                let Some(home) = home().await else { continue };
                let Some(text) = read_text(fs, &join(&home, &path)).await else {
                    continue;
                };
                out.extend(if path.ends_with("known_hosts") {
                    generators::known_hosts_names(&text)
                } else {
                    generators::ssh_config_hosts(&text)
                });
            }
            Source::Dir { path } => {
                let Ok(entries) = fs.list(&path).await else {
                    continue;
                };
                let names = entries.into_iter().map(|e| e.name);
                out.extend(if kind == ArgKind::Interface {
                    names
                        .map(|n| Value {
                            value: n,
                            description: Some("network interface".into()),
                        })
                        .collect()
                } else {
                    generators::service_names(names)
                });
            }
        }
        if out.len() >= generators::MAX_VALUES {
            break;
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|v| seen.insert(v.value.clone()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_dirs_follow_path_then_extras() {
        let dirs = command_dirs(Some("/a:/b::/a/"), Some("/home/u"));
        if cfg!(windows) {
            return;
        }
        assert_eq!(dirs[..2], ["/a", "/b"]);
        assert!(dirs.contains(&"/home/u/.cargo/bin".to_owned()));
        assert!(dirs.contains(&"/opt/homebrew/bin".to_owned()));
        // Remote: no PATH, no home → only absolute extras.
        let remote = command_dirs(None, None);
        assert!(remote.iter().all(|d| d.starts_with('/')));
        assert!(remote.contains(&"/usr/bin".to_owned()));
    }

    #[test]
    fn executables_only() {
        assert_eq!(
            command_name("ls", EntryKind::File, Some(0o755), false).as_deref(),
            Some("ls")
        );
        assert_eq!(
            command_name("README", EntryKind::File, Some(0o644), false),
            None
        );
        assert_eq!(
            command_name("python3", EntryKind::Symlink, Some(0o777), false).as_deref(),
            Some("python3")
        );
        assert_eq!(
            command_name(".hidden", EntryKind::File, Some(0o755), false),
            None
        );
        assert_eq!(command_name("lib", EntryKind::Dir, None, false), None);
        assert_eq!(
            command_name("Git.EXE", EntryKind::File, None, true).as_deref(),
            Some("Git")
        );
        assert_eq!(command_name("notes.txt", EntryKind::File, None, true), None);
    }

    #[test]
    fn ancestors_walk_up_to_root() {
        assert_eq!(ancestors("/a/b/"), ["/a/b", "/a", "/"]);
        assert_eq!(ancestors("/"), ["/"]);
        assert_eq!(parent("/a"), Some("/".into()));
        assert_eq!(join("/a/", "b"), "/a/b");
    }

    #[tokio::test]
    async fn reads_values_from_the_local_disk() {
        let dir = std::env::temp_dir().join(format!("vp-discover-{}", std::process::id()));
        let sub = dir.join("pkg/src");
        std::fs::create_dir_all(&sub).unwrap_or_default();
        std::fs::write(
            dir.join("pkg/package.json"),
            r#"{"scripts":{"build":"tsc","dev":"vite"}}"#,
        )
        .unwrap_or_default();
        std::fs::write(dir.join("Makefile"), "all:\n\techo\nclean: ## Remove\n")
            .unwrap_or_default();
        let git = dir.join(".git");
        std::fs::create_dir_all(git.join("refs/heads/feat")).unwrap_or_default();
        std::fs::create_dir_all(git.join("refs/remotes/origin")).unwrap_or_default();
        std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap_or_default();
        std::fs::write(git.join("refs/heads/main"), "x").unwrap_or_default();
        std::fs::write(git.join("refs/heads/feat/one"), "x").unwrap_or_default();
        std::fs::write(git.join("refs/remotes/origin/main"), "x").unwrap_or_default();
        std::fs::write(git.join("config"), "[remote \"origin\"]\n").unwrap_or_default();

        let fs = sftp_engine::Local;
        let cwd = sub.to_string_lossy().into_owned();
        let p = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let names = |v: Vec<Value>| v.into_iter().map(|v| v.value).collect::<Vec<_>>();

        let scripts = values(&fs, Some(&cwd), ArgKind::Script, &p(&["npm", "run"])).await;
        assert_eq!(names(scripts), ["build", "dev"]);
        let targets = values(&fs, Some(&cwd), ArgKind::Script, &p(&["make"])).await;
        assert_eq!(names(targets), ["all", "clean"]);
        let branches = values(&fs, Some(&cwd), ArgKind::Branch, &p(&["git", "switch"])).await;
        assert_eq!(names(branches), ["main", "feat/one", "origin/main"]);
        let remotes = values(&fs, Some(&cwd), ArgKind::Remote, &p(&["git", "push"])).await;
        assert_eq!(names(remotes), ["origin"]);
        assert!(
            values(&fs, None, ArgKind::Branch, &p(&["git"]))
                .await
                .is_empty()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
