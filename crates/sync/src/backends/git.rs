// SPDX-License-Identifier: GPL-3.0-or-later
//! Git repository backend (pure Rust via `gix`, no `git` binary, no C dependencies).
//!
//! # What is implemented here
//!
//! [`GitBackend`] stores objects as files in the tree of one branch
//! (`refs/heads/vaultport-sync` by default) of a **local** repository (bare or not; the
//! working tree is never touched). Every `put`/`delete` writes blobs and trees directly into
//! the object database and creates a commit, updating the branch with compare-and-swap
//! semantics. No worktree, index or user git configuration is involved.
//!
//! [`GitBackend::exchange`] synchronizes the branch with a remote through a [`GitTransport`]:
//! fetch, merge, push, retrying when the remote moved. Merging is a three-way merge of the
//! *set of files* (files added on either side are kept, files deleted on one side are
//! deleted). This is always conflict-free because segment files are immutable and uniquely
//! named; in the worst case a deleted (already compacted) segment is resurrected, which is
//! harmless.
//!
//! [`LocalRepoTransport`] implements the transport for a second repository reachable through
//! the file system (a mounted share, tests) by copying objects directly.
//!
//! # What the app must provide
//!
//! A [`GitTransport`] for the real remote, i.e. Git over SSH. The app owns the SSH engine
//! (russh, host-key verification, keys in the vault), so it implements `fetch`/`push` by
//! running `git-upload-pack` / `git-receive-pack` on an SSH channel and speaking the pack
//! protocol (for example with `gix-protocol`/`gix-transport` over the channel's stdio, or
//! `gix`'s `Remote` API with a custom transport). Shelling out to `git` is not an option on
//! mobile. The typical cycle is: `exchange` (pull) → `SyncEngine::sync` on the backend →
//! `exchange` (push).

use crate::backend::{ObjectInfo, Precondition, SyncBackend, validate_name, validate_prefix};
use crate::error::SyncError;
use async_trait::async_trait;
use gix::ObjectId;
use gix::objs::Write as _;
use gix::objs::tree::EntryKind;
use gix::refs::Target;
use gix::refs::transaction::PreviousValue;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Default branch used for sync data.
pub const DEFAULT_BRANCH: &str = "vaultport-sync";
const MAX_DEPTH: usize = 8;
const MAX_WALK: usize = 1_000_000;

fn git_err(e: impl std::fmt::Display) -> SyncError {
    SyncError::Backend(format!("git: {e}"))
}

fn open_repo(path: &Path) -> Result<gix::Repository, SyncError> {
    gix::open_opts(path, gix::open::Options::isolated()).map_err(git_err)
}

fn check_branch(branch: &str) -> Result<(), SyncError> {
    let ok = !branch.is_empty()
        && branch.len() <= 100
        && branch.split('/').all(|s| {
            !s.is_empty() && !s.starts_with('.') && !s.to_ascii_lowercase().ends_with(".lock")
        })
        && branch
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'/' | b'.'))
        && !branch.contains("..");
    if ok {
        Ok(())
    } else {
        Err(SyncError::Backend(format!(
            "invalid branch name {branch:?}"
        )))
    }
}

/// Moves a git operation to the blocking thread pool.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, SyncError> + Send + 'static,
) -> Result<T, SyncError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| SyncError::Backend(format!("git task failed: {e}")))?
}

/// Transport between the local repository and its remote. See the module docs.
#[async_trait]
pub trait GitTransport: Send + Sync {
    /// Downloads the history of `refname` from the remote into the object database of the
    /// local repository at `repo` and returns the remote tip (hex object id), or `None` if the
    /// remote branch does not exist. Must not change local references.
    async fn fetch(&self, repo: &Path, refname: &str) -> Result<Option<String>, SyncError>;

    /// Uploads the objects reachable from `new` and moves the remote `refname` from
    /// `expected` (`None`: must not exist) to `new` atomically. Returns `Ok(false)` if the
    /// remote branch no longer equals `expected` (the caller fetches and merges again).
    async fn push(
        &self,
        repo: &Path,
        refname: &str,
        expected: Option<&str>,
        new: &str,
    ) -> Result<bool, SyncError>;
}

/// Result of [`GitBackend::exchange`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExchangeReport {
    /// The remote had commits we did not have.
    pub received: bool,
    /// A merge commit was created.
    pub merged: bool,
    /// Local commits were pushed.
    pub pushed: bool,
}

/// Sync backend on a local git repository.
#[derive(Debug, Clone)]
pub struct GitBackend {
    path: PathBuf,
    refname: String,
    lock: Arc<tokio::sync::Mutex<()>>,
}

impl GitBackend {
    /// Opens an existing repository at `path`.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, SyncError> {
        let path = path.into();
        open_repo(&path)?;
        Ok(Self::with_path(path))
    }

    /// Opens the repository at `path`, creating a bare repository if there is none.
    pub fn init(path: impl Into<PathBuf>) -> Result<Self, SyncError> {
        let path = path.into();
        if open_repo(&path).is_err() {
            std::fs::create_dir_all(&path)?;
            gix::init_bare(&path).map_err(git_err)?;
        }
        Ok(Self::with_path(path))
    }

    fn with_path(path: PathBuf) -> Self {
        Self {
            path,
            refname: format!("refs/heads/{DEFAULT_BRANCH}"),
            lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Uses another branch (short name, e.g. `vault-sync`).
    pub fn with_branch(mut self, branch: &str) -> Result<Self, SyncError> {
        check_branch(branch)?;
        self.refname = format!("refs/heads/{branch}");
        Ok(self)
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The full reference name holding the sync data.
    #[must_use]
    pub fn refname(&self) -> &str {
        &self.refname
    }

    /// The current tip of the sync branch (hex), if any.
    pub async fn tip(&self) -> Result<Option<String>, SyncError> {
        let (path, refname) = (self.path.clone(), self.refname.clone());
        blocking(move || {
            let repo = open_repo(&path)?;
            Ok(tip(&repo, &refname)?.map(|id| id.to_string()))
        })
        .await
    }

    /// Fetches, merges and pushes the sync branch through `transport`.
    pub async fn exchange(
        &self,
        transport: &dyn GitTransport,
    ) -> Result<ExchangeReport, SyncError> {
        let _guard = self.lock.lock().await;
        let mut report = ExchangeReport::default();
        for _ in 0..8 {
            let remote = transport.fetch(&self.path, &self.refname).await?;
            let (path, refname, remote_c) =
                (self.path.clone(), self.refname.clone(), remote.clone());
            let (local, received, merged) =
                blocking(move || merge_remote(&path, &refname, remote_c.as_deref())).await?;
            report.received |= received;
            report.merged |= merged;
            let Some(local) = local else {
                return Ok(report);
            };
            if remote.as_deref() == Some(local.as_str()) {
                return Ok(report);
            }
            if transport
                .push(&self.path, &self.refname, remote.as_deref(), &local)
                .await?
            {
                report.pushed = true;
                return Ok(report);
            }
        }
        Err(SyncError::Backend(
            "git remote kept changing; try again later".into(),
        ))
    }
}

fn tip(repo: &gix::Repository, refname: &str) -> Result<Option<ObjectId>, SyncError> {
    let r = repo.try_find_reference(refname).map_err(git_err)?;
    Ok(r.and_then(|r| r.try_id().map(gix::Id::detach)))
}

fn tree_of(repo: &gix::Repository, commit: Option<ObjectId>) -> Result<ObjectId, SyncError> {
    match commit {
        None => Ok(ObjectId::empty_tree(repo.object_hash())),
        Some(c) => Ok(repo
            .find_commit(c)
            .map_err(git_err)?
            .tree_id()
            .map_err(git_err)?
            .detach()),
    }
}

/// All blob paths of a tree.
fn files(repo: &gix::Repository, tree: ObjectId) -> Result<HashMap<String, ObjectId>, SyncError> {
    let mut out = HashMap::new();
    let mut stack = vec![(tree, String::new(), 0usize)];
    while let Some((id, prefix, depth)) = stack.pop() {
        let tree = repo.find_tree(id).map_err(git_err)?;
        for entry in tree.iter() {
            let entry = entry.map_err(git_err)?;
            let Ok(file) = std::str::from_utf8(entry.filename()) else {
                continue;
            };
            let name = if prefix.is_empty() {
                file.to_owned()
            } else {
                format!("{prefix}/{file}")
            };
            let mode = entry.mode();
            if mode.is_tree() {
                if depth < MAX_DEPTH {
                    stack.push((entry.object_id(), name, depth + 1));
                }
            } else if mode.is_blob() && validate_name(&name).is_ok() {
                out.insert(name, entry.object_id());
            }
            if out.len() > MAX_WALK {
                return Err(SyncError::Backend("git tree too large".into()));
            }
        }
    }
    Ok(out)
}

fn lookup(
    repo: &gix::Repository,
    tree: ObjectId,
    name: &str,
) -> Result<Option<ObjectId>, SyncError> {
    let tree = repo.find_tree(tree).map_err(git_err)?;
    let entry = tree.lookup_entry_by_path(name).map_err(git_err)?;
    Ok(entry.filter(|e| e.mode().is_blob()).map(|e| e.object_id()))
}

fn commit(
    repo: &gix::Repository,
    refname: &str,
    tree: ObjectId,
    parents: Vec<ObjectId>,
    message: &str,
) -> Result<ObjectId, SyncError> {
    let sig = gix::actor::Signature {
        name: "Vaultport".into(),
        email: "sync@vaultport.invalid".into(),
        time: gix::date::Time::now_utc(),
    };
    let mut buf = gix::date::parse::TimeBuf::default();
    let sig = sig.to_ref(&mut buf);
    let id = repo
        .commit_as(sig, sig, refname, message, tree, parents)
        .map_err(git_err)?;
    Ok(id.detach())
}

fn edit(
    path: &Path,
    refname: &str,
    name: &str,
    change: Option<(Vec<u8>, Option<Precondition>)>,
) -> Result<String, SyncError> {
    let repo = open_repo(path)?;
    let parent = tip(&repo, refname)?;
    let tree = tree_of(&repo, parent)?;
    let existing = lookup(&repo, tree, name)?;
    let mut editor = repo.edit_tree(tree).map_err(git_err)?;
    let (etag, message) = if let Some((bytes, precondition)) = change {
        match (&precondition, existing) {
            (Some(Precondition::IfNoneMatch), Some(_)) => {
                return Err(SyncError::PreconditionFailed(name.to_owned()));
            }
            (Some(Precondition::IfMatch(e)), cur)
                if cur.map(|c| c.to_string()).as_deref() != Some(e.as_str()) =>
            {
                return Err(SyncError::PreconditionFailed(name.to_owned()));
            }
            _ => {}
        }
        let blob = repo.write_blob(&bytes).map_err(git_err)?.detach();
        if existing == Some(blob) {
            return Ok(blob.to_string());
        }
        editor
            .upsert(name, EntryKind::Blob, blob)
            .map_err(git_err)?;
        (blob.to_string(), format!("put {name}"))
    } else {
        if existing.is_none() {
            return Ok(String::new());
        }
        editor.remove(name).map_err(git_err)?;
        (String::new(), format!("delete {name}"))
    };
    let new_tree = editor.write().map_err(git_err)?.detach();
    commit(
        &repo,
        refname,
        new_tree,
        parent.into_iter().collect(),
        &message,
    )?;
    Ok(etag)
}

/// Whether `ancestor` is reachable from `descendant`.
fn is_ancestor(
    repo: &gix::Repository,
    ancestor: ObjectId,
    descendant: ObjectId,
) -> Result<bool, SyncError> {
    Ok(ancestors(repo, descendant)?.contains(&ancestor))
}

fn ancestors(repo: &gix::Repository, start: ObjectId) -> Result<HashSet<ObjectId>, SyncError> {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([start]);
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id) {
            continue;
        }
        if seen.len() > MAX_WALK {
            return Err(SyncError::Backend("git history too long".into()));
        }
        let c = repo.find_commit(id).map_err(git_err)?;
        queue.extend(c.parent_ids().map(gix::Id::detach));
    }
    Ok(seen)
}

/// A common ancestor of `a` and `b` (the first one found breadth-first from `b`). Any
/// common ancestor gives a correct (possibly resurrecting) set merge.
fn merge_base(
    repo: &gix::Repository,
    a: ObjectId,
    b: ObjectId,
) -> Result<Option<ObjectId>, SyncError> {
    let of_a = ancestors(repo, a)?;
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([b]);
    while let Some(id) = queue.pop_front() {
        if of_a.contains(&id) {
            return Ok(Some(id));
        }
        if !seen.insert(id) || seen.len() > MAX_WALK {
            continue;
        }
        let c = repo.find_commit(id).map_err(git_err)?;
        queue.extend(c.parent_ids().map(gix::Id::detach));
    }
    Ok(None)
}

/// Merges the fetched remote tip into the local branch. Returns `(local tip, received,
/// merged)`.
fn merge_remote(
    path: &Path,
    refname: &str,
    remote: Option<&str>,
) -> Result<(Option<String>, bool, bool), SyncError> {
    let repo = open_repo(path)?;
    let local = tip(&repo, refname)?;
    let Some(remote) = remote else {
        return Ok((local.map(|l| l.to_string()), false, false));
    };
    let remote = ObjectId::from_hex(remote.as_bytes()).map_err(git_err)?;
    if !repo.has_object(remote) {
        return Err(SyncError::Backend(
            "git transport did not fetch the remote objects".into(),
        ));
    }
    let Some(local) = local else {
        repo.reference(
            refname,
            remote,
            PreviousValue::MustNotExist,
            "sync: initial fetch",
        )
        .map_err(git_err)?;
        return Ok((Some(remote.to_string()), true, false));
    };
    if local == remote || is_ancestor(&repo, remote, local)? {
        return Ok((Some(local.to_string()), false, false));
    }
    if is_ancestor(&repo, local, remote)? {
        repo.reference(
            refname,
            remote,
            PreviousValue::MustExistAndMatch(Target::Object(local)),
            "sync: fast-forward",
        )
        .map_err(git_err)?;
        return Ok((Some(remote.to_string()), true, false));
    }
    // Diverged: three-way merge of the file sets.
    let base = merge_base(&repo, local, remote)?;
    let l = files(&repo, tree_of(&repo, Some(local))?)?;
    let r = files(&repo, tree_of(&repo, Some(remote))?)?;
    let b = match base {
        Some(b) => files(&repo, tree_of(&repo, Some(b))?)?,
        None => HashMap::new(),
    };
    let mut result: Vec<(&String, &ObjectId)> = Vec::new();
    for (p, id) in &l {
        if r.contains_key(p) || !b.contains_key(p) {
            result.push((p, id));
        }
    }
    for (p, id) in &r {
        if !l.contains_key(p) && !b.contains_key(p) {
            result.push((p, id));
        }
    }
    let mut editor = repo
        .edit_tree(ObjectId::empty_tree(repo.object_hash()))
        .map_err(git_err)?;
    for (p, id) in result {
        editor
            .upsert(p.as_str(), EntryKind::Blob, *id)
            .map_err(git_err)?;
    }
    let tree = editor.write().map_err(git_err)?.detach();
    let merged = commit(&repo, refname, tree, vec![local, remote], "sync: merge")?;
    Ok((Some(merged.to_string()), true, true))
}

#[async_trait]
impl SyncBackend for GitBackend {
    async fn list(&self, prefix: &str) -> Result<Vec<ObjectInfo>, SyncError> {
        validate_prefix(prefix)?;
        let (path, refname, prefix) = (self.path.clone(), self.refname.clone(), prefix.to_owned());
        blocking(move || {
            let repo = open_repo(&path)?;
            let tree = tree_of(&repo, tip(&repo, &refname)?)?;
            let mut out = Vec::new();
            for (name, id) in files(&repo, tree)? {
                if name.starts_with(&prefix) {
                    let size = repo.find_header(id).map_err(git_err)?.size();
                    out.push(ObjectInfo {
                        name,
                        size,
                        etag: Some(id.to_string()),
                    });
                }
            }
            out.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(out)
        })
        .await
    }

    async fn get(&self, name: &str) -> Result<Vec<u8>, SyncError> {
        validate_name(name)?;
        let (path, refname, name) = (self.path.clone(), self.refname.clone(), name.to_owned());
        blocking(move || {
            let repo = open_repo(&path)?;
            let tree = tree_of(&repo, tip(&repo, &refname)?)?;
            let id = lookup(&repo, tree, &name)?.ok_or(SyncError::NotFound(name))?;
            Ok(repo.find_blob(id).map_err(git_err)?.take_data())
        })
        .await
    }

    async fn put(
        &self,
        name: &str,
        bytes: &[u8],
        precondition: Option<Precondition>,
    ) -> Result<String, SyncError> {
        validate_name(name)?;
        let _guard = self.lock.lock().await;
        let (path, refname, name, bytes) = (
            self.path.clone(),
            self.refname.clone(),
            name.to_owned(),
            bytes.to_vec(),
        );
        blocking(move || edit(&path, &refname, &name, Some((bytes, precondition)))).await
    }

    async fn delete(&self, name: &str) -> Result<(), SyncError> {
        validate_name(name)?;
        let _guard = self.lock.lock().await;
        let (path, refname, name) = (self.path.clone(), self.refname.clone(), name.to_owned());
        blocking(move || edit(&path, &refname, &name, None).map(|_| ())).await
    }
}

/// Copies every object reachable from `tip` that `dst` lacks, children before parents, so
/// that an object present in `dst` always implies its whole closure is present.
fn copy_objects(
    src: &gix::Repository,
    dst: &gix::Repository,
    tip: ObjectId,
) -> Result<(), SyncError> {
    let mut seen = HashSet::new();
    let mut stack = vec![(tip, false)];
    while let Some((id, expanded)) = stack.pop() {
        if expanded {
            let obj = src.find_object(id).map_err(git_err)?;
            dst.objects
                .write_buf(obj.kind, &obj.data)
                .map_err(git_err)?;
            continue;
        }
        if !seen.insert(id) || dst.has_object(id) {
            continue;
        }
        if seen.len() > MAX_WALK {
            return Err(SyncError::Backend("too many objects".into()));
        }
        stack.push((id, true));
        let obj = src.find_object(id).map_err(git_err)?;
        match obj.kind {
            gix::objs::Kind::Commit => {
                let c = obj.into_commit();
                stack.push((c.tree_id().map_err(git_err)?.detach(), false));
                stack.extend(c.parent_ids().map(|p| (p.detach(), false)));
            }
            gix::objs::Kind::Tree => {
                let t = obj.into_tree();
                for e in t.iter() {
                    let e = e.map_err(git_err)?;
                    if !e.mode().is_commit() {
                        stack.push((e.object_id(), false));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// A [`GitTransport`] to another repository on the local file system (a mounted share, a
/// USB stick, tests). Objects are copied directly between the object databases.
#[derive(Debug, Clone)]
pub struct LocalRepoTransport {
    remote: PathBuf,
}

impl LocalRepoTransport {
    /// Uses the repository at `remote`, creating a bare repository if there is none.
    pub fn new(remote: impl Into<PathBuf>) -> Result<Self, SyncError> {
        let remote = remote.into();
        if open_repo(&remote).is_err() {
            std::fs::create_dir_all(&remote)?;
            gix::init_bare(&remote).map_err(git_err)?;
        }
        Ok(Self { remote })
    }
}

#[async_trait]
impl GitTransport for LocalRepoTransport {
    async fn fetch(&self, repo: &Path, refname: &str) -> Result<Option<String>, SyncError> {
        let (remote, local, refname) = (self.remote.clone(), repo.to_owned(), refname.to_owned());
        blocking(move || {
            let src = open_repo(&remote)?;
            let dst = open_repo(&local)?;
            let Some(t) = tip(&src, &refname)? else {
                return Ok(None);
            };
            copy_objects(&src, &dst, t)?;
            Ok(Some(t.to_string()))
        })
        .await
    }

    async fn push(
        &self,
        repo: &Path,
        refname: &str,
        expected: Option<&str>,
        new: &str,
    ) -> Result<bool, SyncError> {
        let (remote, local, refname) = (self.remote.clone(), repo.to_owned(), refname.to_owned());
        let expected = expected
            .map(|e| ObjectId::from_hex(e.as_bytes()).map_err(git_err))
            .transpose()?;
        let new = ObjectId::from_hex(new.as_bytes()).map_err(git_err)?;
        blocking(move || {
            let src = open_repo(&local)?;
            let dst = open_repo(&remote)?;
            if tip(&dst, &refname)? != expected {
                return Ok(false);
            }
            copy_objects(&src, &dst, new)?;
            let constraint = match expected {
                Some(e) => PreviousValue::MustExistAndMatch(Target::Object(e)),
                None => PreviousValue::MustNotExist,
            };
            match dst.reference(refname.as_str(), new, constraint, "sync: push") {
                Ok(_) => Ok(true),
                Err(e) => {
                    if tip(&dst, &refname)? == expected {
                        Err(git_err(e))
                    } else {
                        Ok(false)
                    }
                }
            }
        })
        .await
    }
}
