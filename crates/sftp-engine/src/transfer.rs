// SPDX-License-Identifier: GPL-3.0-or-later
//! A transfer queue: concurrency limit, progress, pause/resume/cancel, resumable files,
//! conflict policies and directory recursion between any two [`FileSystem`]s (local↔remote,
//! remote↔remote streamed through this device, local↔local).
//!
//! # How a file is copied
//!
//! 1. Conflict check on the final name (policy or [`ConflictResolver`]).
//! 2. Data is written to `<name>.sshc-part`. With `resume`, an existing part file whose tail
//!    (last 64 KiB) matches the source at the same offset is continued from its size;
//!    otherwise it is rewritten.
//! 3. A reader task streams `chunk_size` chunks into a bounded channel
//!    (`chunks_in_flight` deep) consumed by the writer, so both sides run concurrently; the
//!    SFTP client additionally pipelines up to 16 requests per direction.
//! 4. Size check, then the part file is renamed over the destination (atomically with
//!    `posix-rename@openssh.com`), then mtime/permissions are copied when requested.

use crate::error::FsError;
use crate::fs::{Entry, EntryKind, FileSystem, WriteMode};
use crate::path::numbered_name;
use async_trait::async_trait;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Semaphore, mpsc, watch};

/// Suffix of in-progress files.
pub const PART_SUFFIX: &str = ".sshc-part";
const VERIFY_WINDOW: u64 = 64 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Identifier of a queued transfer.
pub type TransferId = u64;

/// What to do when the destination already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictPolicy {
    /// Ask the [`ConflictResolver`] (skip when none is configured).
    Ask,
    Overwrite,
    Skip,
    /// Write to `name (1).ext`, `name (2).ext`, …
    KeepBoth,
}

/// Answer of a [`ConflictResolver`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictDecision {
    Overwrite,
    Skip,
    KeepBoth,
    /// Cancel the whole transfer.
    Cancel,
}

/// A destination that already exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub transfer: TransferId,
    pub source: Entry,
    pub destination: Entry,
}

/// Decides conflicts for [`ConflictPolicy::Ask`] (implemented by the app's dialog).
#[async_trait]
pub trait ConflictResolver: Send + Sync {
    /// Returns the decision and whether to apply it to all further conflicts of the same
    /// transfer.
    async fn resolve(&self, conflict: &Conflict) -> (ConflictDecision, bool);
}

/// Per-transfer options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferOptions {
    pub conflict: ConflictPolicy,
    /// Continue `*.sshc-part` files left by an interrupted transfer.
    pub resume: bool,
    pub preserve_mtime: bool,
    pub preserve_permissions: bool,
    /// Bytes per read/write (clamped to 4 KiB – 4 MiB).
    pub chunk_size: usize,
    /// Chunks buffered between reader and writer.
    pub chunks_in_flight: usize,
    /// Follow symlinks inside directories (off: they are skipped and reported).
    pub follow_symlinks: bool,
    /// Safety limits for directory recursion.
    pub max_entries: usize,
    pub max_depth: usize,
}

impl Default for TransferOptions {
    fn default() -> Self {
        Self {
            conflict: ConflictPolicy::Ask,
            resume: true,
            preserve_mtime: true,
            preserve_permissions: false,
            chunk_size: 256 * 1024,
            chunks_in_flight: 4,
            follow_symlinks: false,
            max_entries: 1_000_000,
            max_depth: 128,
        }
    }
}

/// A file or directory to copy.
#[derive(Clone)]
pub struct TransferRequest {
    pub source: Arc<dyn FileSystem>,
    pub source_path: String,
    pub destination: Arc<dyn FileSystem>,
    /// Full destination path of the copied item (not its parent directory).
    pub destination_path: String,
    pub options: TransferOptions,
}

impl std::fmt::Debug for TransferRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransferRequest")
            .field("source", &self.source.kind())
            .field("source_path", &self.source_path)
            .field("destination", &self.destination.kind())
            .field("destination_path", &self.destination_path)
            .field("options", &self.options)
            .finish()
    }
}

/// Lifecycle of a transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferState {
    Queued,
    Running,
    Paused,
    Completed,
    Failed(String),
    Cancelled,
}

impl TransferState {
    #[must_use]
    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed(_) | Self::Cancelled)
    }
}

/// Progress snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct TransferProgress {
    pub id: TransferId,
    /// Destination path of the file being copied.
    pub current_file: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
    /// Smoothed over the last few seconds.
    pub bytes_per_second: f64,
    pub eta: Option<Duration>,
}

/// Events emitted by the queue.
#[derive(Debug, Clone, PartialEq)]
pub enum TransferEvent {
    Queued {
        id: TransferId,
        source_path: String,
        destination_path: String,
    },
    Started {
        id: TransferId,
        bytes_total: u64,
        files_total: u64,
    },
    Progress(TransferProgress),
    /// A file was continued from a part file.
    FileResumed {
        id: TransferId,
        path: String,
        offset: u64,
    },
    FileCompleted {
        id: TransferId,
        path: String,
    },
    FileSkipped {
        id: TransferId,
        path: String,
        reason: String,
    },
    Paused {
        id: TransferId,
    },
    Resumed {
        id: TransferId,
    },
    Completed {
        id: TransferId,
        bytes: u64,
        files: u64,
        skipped: u64,
    },
    Failed {
        id: TransferId,
        error: String,
    },
    Cancelled {
        id: TransferId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    Run,
    Pause,
    Cancel,
}

struct Job {
    control: watch::Sender<Control>,
    state: watch::Sender<TransferState>,
}

struct QueueInner {
    slots: Arc<Semaphore>,
    jobs: Mutex<HashMap<TransferId, Job>>,
    next_id: AtomicU64,
    events: mpsc::UnboundedSender<TransferEvent>,
    resolver: Option<Arc<dyn ConflictResolver>>,
}

/// Runs transfers with a concurrency limit. Cheap to clone.
#[derive(Clone)]
pub struct TransferQueue {
    inner: Arc<QueueInner>,
}

impl std::fmt::Debug for TransferQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransferQueue")
            .field("free_slots", &self.inner.slots.available_permits())
            .finish_non_exhaustive()
    }
}

impl TransferQueue {
    /// Creates a queue running at most `max_concurrent` transfers at once. Events are
    /// delivered on the returned receiver (dropping it is fine).
    #[must_use]
    pub fn new(
        max_concurrent: usize,
        resolver: Option<Arc<dyn ConflictResolver>>,
    ) -> (Self, mpsc::UnboundedReceiver<TransferEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                inner: Arc::new(QueueInner {
                    slots: Arc::new(Semaphore::new(max_concurrent.max(1))),
                    jobs: Mutex::new(HashMap::new()),
                    next_id: AtomicU64::new(1),
                    events: tx,
                    resolver,
                }),
            },
            rx,
        )
    }

    /// Queues a transfer; it starts as soon as a slot is free. Must be called inside a tokio
    /// runtime.
    pub fn enqueue(&self, req: TransferRequest) -> TransferId {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (control, control_rx) = watch::channel(Control::Run);
        let (state, _) = watch::channel(TransferState::Queued);
        if let Ok(mut jobs) = self.inner.jobs.lock() {
            jobs.insert(id, Job { control, state });
        }
        let _ = self.inner.events.send(TransferEvent::Queued {
            id,
            source_path: req.source_path.clone(),
            destination_path: req.destination_path.clone(),
        });
        let inner = self.inner.clone();
        tokio::spawn(async move { run_job(inner, id, req, control_rx).await });
        id
    }

    fn set_control(&self, id: TransferId, c: Control) -> bool {
        let Ok(jobs) = self.inner.jobs.lock() else {
            return false;
        };
        match jobs.get(&id) {
            Some(j) if !j.state.borrow().is_finished() => {
                j.control.send_replace(c);
                true
            }
            _ => false,
        }
    }

    /// Pauses a queued or running transfer (between chunks). Returns `false` if unknown or
    /// finished.
    pub fn pause(&self, id: TransferId) -> bool {
        self.set_control(id, Control::Pause)
    }

    /// Resumes a paused transfer.
    pub fn resume(&self, id: TransferId) -> bool {
        self.set_control(id, Control::Run)
    }

    /// Cancels a transfer. The part file is kept so the transfer can be resumed later.
    pub fn cancel(&self, id: TransferId) -> bool {
        self.set_control(id, Control::Cancel)
    }

    /// Current state.
    pub fn state(&self, id: TransferId) -> Option<TransferState> {
        self.inner
            .jobs
            .lock()
            .ok()?
            .get(&id)
            .map(|j| j.state.borrow().clone())
    }

    /// Waits until the transfer finished and returns its final state.
    pub async fn wait(&self, id: TransferId) -> Option<TransferState> {
        let mut rx = self.inner.jobs.lock().ok()?.get(&id)?.state.subscribe();
        loop {
            let s = rx.borrow_and_update().clone();
            if s.is_finished() {
                return Some(s);
            }
            if rx.changed().await.is_err() {
                return None;
            }
        }
    }

    /// Forgets finished transfers.
    pub fn prune_finished(&self) {
        if let Ok(mut jobs) = self.inner.jobs.lock() {
            jobs.retain(|_, j| !j.state.borrow().is_finished());
        }
    }
}

/// Moving-window rate meter.
struct RateMeter {
    samples: VecDeque<(Instant, u64)>,
    window: Duration,
}

impl RateMeter {
    fn new() -> Self {
        Self {
            samples: VecDeque::new(),
            window: Duration::from_secs(5),
        }
    }

    fn add(&mut self, now: Instant, total: u64) -> f64 {
        self.samples.push_back((now, total));
        while self.samples.len() > 2
            && self
                .samples
                .front()
                .is_some_and(|(t, _)| now.duration_since(*t) > self.window)
        {
            self.samples.pop_front();
        }
        match (self.samples.front(), self.samples.back()) {
            (Some((t0, b0)), Some((t1, b1))) if t1 > t0 => {
                (b1 - b0) as f64 / t1.duration_since(*t0).as_secs_f64()
            }
            _ => 0.0,
        }
    }
}

struct Ctx {
    inner: Arc<QueueInner>,
    id: TransferId,
    control: watch::Receiver<Control>,
    opts: TransferOptions,
    src: Arc<dyn FileSystem>,
    dst: Arc<dyn FileSystem>,
    bytes_total: u64,
    files_total: u64,
    bytes_done: u64,
    files_done: u64,
    skipped: u64,
    sticky_decision: Option<ConflictDecision>,
    meter: RateMeter,
    last_progress: Instant,
}

enum Flow {
    Continue,
    Cancelled,
}

impl Ctx {
    fn emit(&self, e: TransferEvent) {
        let _ = self.inner.events.send(e);
    }

    fn set_state(&self, s: TransferState) {
        if let Ok(jobs) = self.inner.jobs.lock()
            && let Some(j) = jobs.get(&self.id)
        {
            j.state.send_replace(s);
        }
    }

    /// Honors pause/cancel. Returns `Cancelled` when the transfer must stop.
    async fn checkpoint(&mut self) -> Flow {
        loop {
            let c = *self.control.borrow_and_update();
            match c {
                Control::Run => return Flow::Continue,
                Control::Cancel => return Flow::Cancelled,
                Control::Pause => {
                    self.set_state(TransferState::Paused);
                    self.emit(TransferEvent::Paused { id: self.id });
                    loop {
                        if self.control.changed().await.is_err() {
                            return Flow::Cancelled;
                        }
                        let c = *self.control.borrow_and_update();
                        if c != Control::Pause {
                            if c == Control::Run {
                                self.set_state(TransferState::Running);
                                self.emit(TransferEvent::Resumed { id: self.id });
                            }
                            break;
                        }
                    }
                }
            }
        }
    }

    fn progress(&mut self, current: &str, force: bool) {
        let now = Instant::now();
        if !force && now.duration_since(self.last_progress) < PROGRESS_INTERVAL {
            return;
        }
        self.last_progress = now;
        let rate = self.meter.add(now, self.bytes_done);
        let remaining = self.bytes_total.saturating_sub(self.bytes_done);
        let eta = (rate > 0.0).then(|| Duration::from_secs_f64(remaining as f64 / rate));
        self.emit(TransferEvent::Progress(TransferProgress {
            id: self.id,
            current_file: current.to_owned(),
            bytes_done: self.bytes_done,
            bytes_total: self.bytes_total,
            files_done: self.files_done,
            files_total: self.files_total,
            bytes_per_second: rate,
            eta,
        }));
    }

    fn skip(&mut self, path: &str, reason: impl Into<String>) {
        self.skipped += 1;
        self.emit(TransferEvent::FileSkipped {
            id: self.id,
            path: path.to_owned(),
            reason: reason.into(),
        });
    }
}

async fn run_job(
    inner: Arc<QueueInner>,
    id: TransferId,
    req: TransferRequest,
    mut control: watch::Receiver<Control>,
) {
    // Wait for a slot (cancellable).
    let permit = loop {
        tokio::select! {
            p = inner.slots.clone().acquire_owned() => break p.ok(),
            changed = control.changed() => {
                if changed.is_err() || *control.borrow() == Control::Cancel {
                    break None;
                }
            }
        }
    };
    let mut ctx = Ctx {
        inner: inner.clone(),
        id,
        control,
        opts: req.options.clone(),
        src: req.source.clone(),
        dst: req.destination.clone(),
        bytes_total: 0,
        files_total: 0,
        bytes_done: 0,
        files_done: 0,
        skipped: 0,
        sticky_decision: None,
        meter: RateMeter::new(),
        last_progress: Instant::now(),
    };
    if permit.is_none() {
        ctx.set_state(TransferState::Cancelled);
        ctx.emit(TransferEvent::Cancelled { id });
        return;
    }
    ctx.set_state(TransferState::Running);
    let result = execute(&mut ctx, &req).await;
    drop(permit);
    match result {
        Ok(Flow::Continue) => {
            ctx.set_state(TransferState::Completed);
            ctx.emit(TransferEvent::Completed {
                id,
                bytes: ctx.bytes_done,
                files: ctx.files_done,
                skipped: ctx.skipped,
            });
        }
        Ok(Flow::Cancelled) | Err(FsError::Cancelled) => {
            ctx.set_state(TransferState::Cancelled);
            ctx.emit(TransferEvent::Cancelled { id });
        }
        Err(e) => {
            let msg = e.to_string();
            ctx.set_state(TransferState::Failed(msg.clone()));
            ctx.emit(TransferEvent::Failed { id, error: msg });
        }
    }
}

/// One item of a transfer plan.
enum Item {
    Dir {
        dst: String,
    },
    File {
        src: Entry,
        src_path: String,
        dst: String,
    },
}

async fn execute(ctx: &mut Ctx, req: &TransferRequest) -> Result<Flow, FsError> {
    let root = ctx.src.stat(&req.source_path).await?;
    let mut plan = Vec::new();
    match root.kind {
        EntryKind::Dir => {
            let mut dst_root = req.destination_path.clone();
            if let Ok(existing) = ctx.dst.lstat(&dst_root).await {
                if !existing.is_dir_like() {
                    return Err(FsError::NotADirectory(dst_root));
                }
                match ctx.opts.conflict {
                    ConflictPolicy::Skip => {
                        ctx.skip(&dst_root, "destination directory exists");
                        return Ok(Flow::Continue);
                    }
                    ConflictPolicy::KeepBoth => {
                        dst_root = unique_path(&*ctx.dst, &dst_root).await?;
                    }
                    // Merge; files are checked one by one.
                    ConflictPolicy::Overwrite | ConflictPolicy::Ask => {}
                }
            }
            plan_dir(ctx, &req.source_path, &dst_root, &mut plan).await?;
        }
        EntryKind::File => {
            ctx.files_total = 1;
            ctx.bytes_total = root.size;
            plan.push(Item::File {
                src: root,
                src_path: req.source_path.clone(),
                dst: req.destination_path.clone(),
            });
        }
        EntryKind::Symlink | EntryKind::Other => {
            return Err(FsError::Unsupported(format!(
                "{} is not a regular file or directory",
                req.source_path
            )));
        }
    }
    ctx.emit(TransferEvent::Started {
        id: ctx.id,
        bytes_total: ctx.bytes_total,
        files_total: ctx.files_total,
    });
    for item in plan {
        if let Flow::Cancelled = ctx.checkpoint().await {
            return Ok(Flow::Cancelled);
        }
        match item {
            Item::Dir { dst } => match ctx.dst.mkdir(&dst).await {
                Ok(()) | Err(FsError::AlreadyExists(_)) => {}
                Err(e) => return Err(e),
            },
            Item::File { src, src_path, dst } => {
                if let Flow::Cancelled = copy_one(ctx, &src, &src_path, &dst).await? {
                    return Ok(Flow::Cancelled);
                }
            }
        }
    }
    ctx.progress("", true);
    Ok(Flow::Continue)
}

/// Walks the source tree breadth-first and builds the plan (directories before contents).
async fn plan_dir(
    ctx: &mut Ctx,
    src_root: &str,
    dst_root: &str,
    plan: &mut Vec<Item>,
) -> Result<(), FsError> {
    let mut queue = VecDeque::from([(src_root.to_owned(), dst_root.to_owned(), 0usize)]);
    let mut entries = 0usize;
    while let Some((src_dir, dst_dir, depth)) = queue.pop_front() {
        if depth > ctx.opts.max_depth {
            return Err(FsError::LimitExceeded(format!(
                "directory depth above {}",
                ctx.opts.max_depth
            )));
        }
        plan.push(Item::Dir {
            dst: dst_dir.clone(),
        });
        for e in ctx.src.list(&src_dir).await? {
            entries += 1;
            if entries > ctx.opts.max_entries {
                return Err(FsError::LimitExceeded(format!(
                    "more than {} entries",
                    ctx.opts.max_entries
                )));
            }
            // Both joins validate the (untrusted) name: a server answering `../../x` cannot
            // make us read or write outside the selected trees.
            let (src_path, dst_path) = match (
                ctx.src.join(&src_dir, &e.name),
                ctx.dst.join(&dst_dir, &e.name),
            ) {
                (Ok(s), Ok(d)) => (s, d),
                (Err(err), _) | (_, Err(err)) => {
                    tracing::warn!("skipping unsafe entry name: {err}");
                    ctx.skip(&e.name, err.to_string());
                    continue;
                }
            };
            if e.name.ends_with(PART_SUFFIX) {
                continue;
            }
            let entry = if e.kind == EntryKind::Symlink {
                if !ctx.opts.follow_symlinks {
                    ctx.skip(&dst_path, "symbolic link");
                    continue;
                }
                match ctx.src.stat(&src_path).await {
                    Ok(t) => t,
                    Err(err) => {
                        ctx.skip(&dst_path, format!("dangling symbolic link: {err}"));
                        continue;
                    }
                }
            } else {
                e
            };
            match entry.kind {
                EntryKind::Dir => queue.push_back((src_path, dst_path, depth + 1)),
                EntryKind::File => {
                    ctx.files_total += 1;
                    ctx.bytes_total += entry.size;
                    plan.push(Item::File {
                        src: entry,
                        src_path,
                        dst: dst_path,
                    });
                }
                EntryKind::Symlink | EntryKind::Other => ctx.skip(&dst_path, "special file"),
            }
        }
    }
    Ok(())
}

/// First free `name (n).ext` next to `path`.
pub async fn unique_path(fs: &dyn FileSystem, path: &str) -> Result<String, FsError> {
    let name = fs
        .file_name(path)
        .ok_or_else(|| FsError::UnsafePath(path.to_owned()))?;
    let parent = fs.parent(path).unwrap_or_default();
    for n in 1..10_000 {
        let candidate = fs.join(&parent, &numbered_name(&name, n))?;
        if !fs.exists(&candidate).await? {
            return Ok(candidate);
        }
    }
    Err(FsError::LimitExceeded(format!("no free name for {path}")))
}

async fn decide_conflict(ctx: &mut Ctx, src: &Entry, existing: Entry) -> ConflictDecision {
    match ctx.opts.conflict {
        ConflictPolicy::Overwrite => ConflictDecision::Overwrite,
        ConflictPolicy::Skip => ConflictDecision::Skip,
        ConflictPolicy::KeepBoth => ConflictDecision::KeepBoth,
        ConflictPolicy::Ask => {
            if let Some(d) = ctx.sticky_decision {
                return d;
            }
            let Some(r) = ctx.inner.resolver.clone() else {
                return ConflictDecision::Skip;
            };
            let (d, all) = r
                .resolve(&Conflict {
                    transfer: ctx.id,
                    source: src.clone(),
                    destination: existing,
                })
                .await;
            if all {
                ctx.sticky_decision = Some(d);
            }
            d
        }
    }
}

async fn copy_one(
    ctx: &mut Ctx,
    src: &Entry,
    src_path: &str,
    dst_path: &str,
) -> Result<Flow, FsError> {
    let mut dst_path = dst_path.to_owned();
    if let Ok(existing) = ctx.dst.lstat(&dst_path).await {
        if existing.is_dir_like() {
            ctx.skip(&dst_path, "a directory with this name exists");
            ctx.bytes_total = ctx.bytes_total.saturating_sub(src.size);
            return Ok(Flow::Continue);
        }
        match decide_conflict(ctx, src, existing).await {
            ConflictDecision::Overwrite => {}
            ConflictDecision::Skip => {
                ctx.skip(&dst_path, "destination exists");
                ctx.bytes_total = ctx.bytes_total.saturating_sub(src.size);
                return Ok(Flow::Continue);
            }
            ConflictDecision::KeepBoth => dst_path = unique_path(&*ctx.dst, &dst_path).await?,
            ConflictDecision::Cancel => return Ok(Flow::Cancelled),
        }
    }
    let part = format!("{dst_path}{PART_SUFFIX}");
    let offset = resume_offset(ctx, src, src_path, &part).await?;
    if offset > 0 {
        ctx.emit(TransferEvent::FileResumed {
            id: ctx.id,
            path: dst_path.clone(),
            offset,
        });
    }
    let file_start = ctx.bytes_done;
    ctx.bytes_done += offset;

    let reader = ctx.src.open_read(src_path, offset).await?;
    let mode = if offset > 0 {
        WriteMode::Resume { offset }
    } else {
        WriteMode::Truncate
    };
    let mut writer = ctx.dst.open_write(&part, mode).await?;

    let chunk = ctx.opts.chunk_size.clamp(4 * 1024, 4 * 1024 * 1024);
    let (tx, mut rx) = mpsc::channel::<Result<Vec<u8>, FsError>>(ctx.opts.chunks_in_flight.max(1));
    let read_path = src_path.to_owned();
    let reader_task = tokio::spawn(async move {
        let mut reader = reader;
        loop {
            let mut buf = vec![0u8; chunk];
            let mut filled = 0;
            // Fill the chunk (SFTP reads may return less than asked).
            while filled < chunk {
                match reader.read(&mut buf[filled..]).await {
                    Ok(0) => break,
                    Ok(n) => filled += n,
                    Err(e) => {
                        let _ = tx.send(Err(FsError::stream(&e, &read_path))).await;
                        return;
                    }
                }
            }
            if filled == 0 {
                return;
            }
            buf.truncate(filled);
            if tx.send(Ok(buf)).await.is_err() {
                return;
            }
        }
    });

    let outcome: Result<Flow, FsError> = async {
        while let Some(chunk) = rx.recv().await {
            let data = chunk?;
            writer
                .write_all(&data)
                .await
                .map_err(|e| FsError::stream(&e, &part))?;
            ctx.bytes_done += data.len() as u64;
            ctx.progress(&dst_path, false);
            if let Flow::Cancelled = ctx.checkpoint().await {
                return Ok(Flow::Cancelled);
            }
        }
        Ok(Flow::Continue)
    }
    .await;
    drop(rx);
    reader_task.abort();
    // Always close the writer so the part file keeps everything written so far.
    let closed = writer
        .shutdown()
        .await
        .map_err(|e| FsError::stream(&e, &part));
    match outcome {
        Ok(Flow::Continue) => closed?,
        other => {
            ctx.bytes_done = file_start;
            return other;
        }
    }

    let written = ctx.dst.lstat(&part).await?.size;
    if written != src.size {
        return Err(FsError::Io(format!(
            "{dst_path}: size mismatch after transfer ({written} of {} bytes; source changed?)",
            src.size
        )));
    }
    ctx.dst.rename_replace(&part, &dst_path).await?;
    if ctx.opts.preserve_permissions
        && let Some(p) = src.permissions
        && let Err(e) = ctx.dst.chmod(&dst_path, p).await
    {
        tracing::debug!("could not preserve permissions of {dst_path}: {e}");
    }
    if ctx.opts.preserve_mtime
        && let Some(m) = src.modified
        && let Err(e) = ctx.dst.set_times(&dst_path, m, src.accessed).await
    {
        tracing::debug!("could not preserve mtime of {dst_path}: {e}");
    }
    ctx.files_done += 1;
    ctx.emit(TransferEvent::FileCompleted {
        id: ctx.id,
        path: dst_path.clone(),
    });
    ctx.progress(&dst_path, true);
    Ok(Flow::Continue)
}

/// Offset at which an existing part file can be continued (0 = start over).
async fn resume_offset(ctx: &Ctx, src: &Entry, src_path: &str, part: &str) -> Result<u64, FsError> {
    if !ctx.opts.resume {
        return Ok(0);
    }
    let Ok(existing) = ctx.dst.lstat(part).await else {
        return Ok(0);
    };
    if existing.kind != EntryKind::File || existing.size == 0 || existing.size > src.size {
        return Ok(0);
    }
    let size = existing.size;
    let window = size.min(VERIFY_WINDOW);
    let start = size - window;
    let (a, b) = tokio::join!(
        read_range(&*ctx.src, src_path, start, window),
        read_range(&*ctx.dst, part, start, window)
    );
    match (a, b) {
        (Ok(a), Ok(b)) if a == b => Ok(size),
        _ => {
            tracing::info!("part file {part} does not match the source; starting over");
            Ok(0)
        }
    }
}

async fn read_range(
    fs: &dyn FileSystem,
    path: &str,
    offset: u64,
    len: u64,
) -> Result<Vec<u8>, FsError> {
    let mut r = fs.open_read(path, offset).await?;
    let mut buf = vec![0u8; usize::try_from(len).unwrap_or(usize::MAX)];
    r.read_exact(&mut buf)
        .await
        .map_err(|e| FsError::stream(&e, path))?;
    Ok(buf)
}

#[cfg(test)]
mod tests;
