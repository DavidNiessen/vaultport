// SPDX-License-Identifier: GPL-3.0-or-later
//! Resumable, verifying model downloader.
//!
//! * **HTTPS only**, to an explicit host allow-list ([`UrlPolicy`]). Redirects are followed by
//!   hand (at most [`MAX_REDIRECTS`]) and every hop is checked against the same policy, so a
//!   redirect can never lead to an unexpected host, a plain-HTTP URL, an IP literal, a custom
//!   port or a URL with embedded credentials.
//! * **Streaming**: data goes straight to `<dest>.part`; memory use is constant.
//! * **Resumable**: an existing `.part` file is re-hashed from disk and the transfer continues
//!   with an HTTP `Range` request. Servers that ignore the range (`200`) or reject it (`416`)
//!   cause a clean restart.
//! * **Verified**: SHA-256 is computed while streaming and the size is checked against the
//!   pinned value. On mismatch the partial file is deleted. Only a verified file is atomically
//!   renamed to its final name, so a file at the final path is always complete.
//! * **Cancellable** through a [`CancellationToken`]; the `.part` file is kept for resuming.
//! * **Exclusive**: the `.part` file is locked, so two processes cannot write the same file.
//! * Free disk space is checked before the transfer starts.
//!
//! # Privacy
//!
//! The only requests made are anonymous `GET`s for the pinned file (plus the redirects the host
//! issues). No cookies, no tokens, no identifying headers: the `User-Agent` is the generic
//! `local-ai/<version>` string. The system proxy configuration is honoured.

use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::StatusCode;
use reqwest::header::{ACCEPT_ENCODING, CONTENT_RANGE, HeaderMap, LOCATION, RANGE};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;
use url::{Host, Url};

use crate::catalog::{ModelEntry, is_sha256_hex};
use crate::error::{Error, Result};

/// Maximum number of redirects followed for one request.
pub const MAX_REDIRECTS: usize = 10;

/// Extra free space required on top of the remaining download size.
const DISK_SPACE_MARGIN: u64 = 64 * 1024 * 1024;

/// Minimum interval between two progress callbacks.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

/// Buffer size used when re-hashing an existing partial file.
const REHASH_BUFFER: usize = 1024 * 1024;

/// Exact hosts Hugging Face serves model files from (documentation; all are also covered by the
/// suffix rules below).
const HF_EXACT_HOSTS: &[&str] = &[
    "huggingface.co",
    "hf.co",
    "cdn-lfs.huggingface.co",
    "cdn-lfs-us-1.huggingface.co",
    "cdn-lfs-eu-1.huggingface.co",
    "cdn-lfs.hf.co",
    "cdn-lfs-us-1.hf.co",
    "cdn-lfs-eu-1.hf.co",
    "cas-bridge.xethub.hf.co",
];

/// Domains (and all their subdomains) operated by Hugging Face. Downloads are redirected to
/// CDN hosts such as `us.aws.cdn.hf.co` or `cas-bridge.xethub.hf.co` whose exact names change
/// over time, so subdomains of these two registrable domains are accepted.
const HF_DOMAIN_SUFFIXES: &[&str] = &["huggingface.co", "hf.co"];

/// Which URLs the downloader may fetch, including redirect targets.
#[derive(Debug, Clone)]
pub struct UrlPolicy {
    exact: Vec<String>,
    suffixes: Vec<String>,
    /// Allow plain HTTP to loopback addresses. Only ever `true` for the unit-test mock server.
    loopback_http: bool,
}

impl UrlPolicy {
    /// The policy for the pinned model catalog: Hugging Face and its CDNs, HTTPS only.
    pub fn hugging_face() -> Self {
        Self::new(HF_EXACT_HOSTS, HF_DOMAIN_SUFFIXES)
    }

    /// An HTTPS-only policy accepting the `exact` hosts plus every host equal to or below one of
    /// the `domains` (for example `"example.org"` accepts `cdn.example.org`). Matching is
    /// case-insensitive and always on label boundaries.
    pub fn new(exact: &[&str], domains: &[&str]) -> Self {
        let norm = |s: &&str| s.trim_end_matches('.').to_ascii_lowercase();
        Self {
            exact: exact.iter().map(norm).collect(),
            suffixes: domains.iter().map(norm).collect(),
            loopback_http: false,
        }
    }

    /// Accepts only plain HTTP to 127.0.0.1 (for the mock server in unit tests).
    #[cfg(test)]
    pub(crate) fn loopback_for_tests() -> Self {
        Self {
            exact: Vec::new(),
            suffixes: Vec::new(),
            loopback_http: true,
        }
    }

    fn allow_loopback_http(&self) -> bool {
        self.loopback_http
    }

    /// Checks one URL against the policy.
    pub fn check(&self, url: &Url) -> Result<()> {
        let deny = |reason| {
            Err(Error::DisallowedUrl {
                url: redact(url),
                reason,
            })
        };
        if !url.username().is_empty() || url.password().is_some() {
            return deny("URLs with credentials are not allowed");
        }
        if self.allow_loopback_http()
            && url.scheme() == "http"
            && matches!(url.host(), Some(Host::Ipv4(ip)) if ip.is_loopback())
        {
            return Ok(());
        }
        if url.scheme() != "https" {
            return deny("only https is allowed");
        }
        if url.port().is_some() {
            return deny("non-default ports are not allowed");
        }
        let host = match url.host() {
            Some(Host::Domain(d)) => d.to_ascii_lowercase(),
            Some(Host::Ipv4(_) | Host::Ipv6(_)) => return deny("IP address hosts are not allowed"),
            None => return deny("URL has no host"),
        };
        let allowed = self.exact.contains(&host)
            || self.suffixes.iter().any(|s| {
                host == *s
                    || host
                        .strip_suffix(s.as_str())
                        .is_some_and(|prefix| prefix.ends_with('.') && prefix.len() > 1)
            });
        if allowed {
            Ok(())
        } else {
            deny("host is not on the allow-list")
        }
    }
}

/// Strips query and fragment (signed CDN URLs carry tokens) before a URL ends up in an error.
fn redact(url: &Url) -> String {
    let mut u = url.clone();
    u.set_query(None);
    u.set_fragment(None);
    u.to_string()
}

/// What to download and how to verify it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadSpec {
    /// Source URL (must pass the downloader's [`UrlPolicy`]).
    pub url: String,
    /// Exact expected size in bytes.
    pub size: u64,
    /// Expected SHA-256, lowercase hex.
    pub sha256: String,
}

impl From<&ModelEntry> for DownloadSpec {
    fn from(m: &ModelEntry) -> Self {
        Self {
            url: m.url(),
            size: m.size,
            sha256: m.sha256.to_owned(),
        }
    }
}

/// A progress report.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Progress {
    /// Bytes on disk so far (including bytes resumed from an earlier attempt).
    pub downloaded: u64,
    /// Total size of the file.
    pub total: u64,
    /// Average transfer rate of the current attempt, in bytes per second.
    pub bytes_per_second: f64,
}

impl Progress {
    /// Completion in `0.0..=1.0`.
    pub fn fraction(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.downloaded as f64 / self.total as f64).clamp(0.0, 1.0)
        }
    }
}

/// Rate-limited progress reporting.
struct Meter {
    total: u64,
    started: Instant,
    start_bytes: u64,
    last_emit: Instant,
}

impl Meter {
    fn new(total: u64, start_bytes: u64) -> Self {
        let now = Instant::now();
        Self {
            total,
            started: now,
            start_bytes,
            last_emit: now,
        }
    }

    fn snapshot(&self, downloaded: u64) -> Progress {
        let secs = self.started.elapsed().as_secs_f64();
        let transferred = downloaded.saturating_sub(self.start_bytes) as f64;
        Progress {
            downloaded,
            total: self.total,
            bytes_per_second: if secs > 0.0 { transferred / secs } else { 0.0 },
        }
    }

    fn tick(&mut self, downloaded: u64, cb: &mut impl FnMut(Progress)) {
        if self.last_emit.elapsed() >= PROGRESS_INTERVAL {
            self.last_emit = Instant::now();
            cb(self.snapshot(downloaded));
        }
    }
}

/// The path of the partial file for `dest` (`<dest>.part`).
pub fn part_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// HTTP downloader enforcing a [`UrlPolicy`].
#[derive(Debug, Clone)]
pub struct Downloader {
    client: reqwest::Client,
    policy: UrlPolicy,
}

impl Downloader {
    /// A downloader for the Hugging Face hosted catalog.
    pub fn new() -> Result<Self> {
        Self::with_policy(UrlPolicy::hugging_face())
    }

    /// A downloader with a custom (still HTTPS-only) policy, e.g. for a self-hosted mirror.
    pub fn with_policy(policy: UrlPolicy) -> Result<Self> {
        let builder = reqwest::Client::builder()
            // Redirects are followed manually so that every hop is checked.
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("local-ai/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(30))
            .read_timeout(Duration::from_secs(60))
            .https_only(!policy.allow_loopback_http());
        let builder = if policy.allow_loopback_http() {
            builder.no_proxy()
        } else {
            builder
        };
        Ok(Self {
            client: builder.build()?,
            policy,
        })
    }

    /// The policy in effect.
    pub fn policy(&self) -> &UrlPolicy {
        &self.policy
    }

    /// Downloads `spec` to `dest`, resuming from `<dest>.part` if present.
    ///
    /// On success `dest` holds exactly the verified bytes. If `dest` already exists it is
    /// re-verified and, if it does not match, replaced.
    ///
    /// `on_progress` is called at most every 200 ms and once at the end.
    pub async fn download<F>(
        &self,
        spec: &DownloadSpec,
        dest: &Path,
        mut on_progress: F,
        cancel: &CancellationToken,
    ) -> Result<()>
    where
        F: FnMut(Progress) + Send,
    {
        if !is_sha256_hex(&spec.sha256) {
            return Err(Error::InvalidRequest(
                "expected SHA-256 must be 64 lowercase hex characters",
            ));
        }
        if spec.size == 0 {
            return Err(Error::InvalidRequest("expected size must not be zero"));
        }
        let url = Url::parse(&spec.url).map_err(|_| Error::DisallowedUrl {
            url: spec.url.clone(),
            reason: "not a valid URL",
        })?;
        self.policy.check(&url)?;
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let dir = dest
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or(Error::InvalidRequest("destination has no parent directory"))?;
        tokio::fs::create_dir_all(dir).await?;

        if tokio::fs::try_exists(dest).await? {
            if verify_file(dest, spec, cancel).await? {
                on_progress(Meter::new(spec.size, spec.size).snapshot(spec.size));
                return Ok(());
            }
            tracing::warn!(path = %dest.display(), "existing model file failed verification; replacing");
            tokio::fs::remove_file(dest).await?;
        }

        let part = part_path(dest);
        let result = self
            .download_part(spec, &url, dir, &part, dest, &mut on_progress, cancel)
            .await;
        if let Err(e @ (Error::HashMismatch { .. } | Error::SizeMismatch { .. })) = &result {
            tracing::warn!(error = %e, "discarding corrupt partial download");
            if let Err(rm) = tokio::fs::remove_file(&part).await
                && rm.kind() != io::ErrorKind::NotFound
            {
                tracing::warn!(error = %rm, "could not delete partial download");
            }
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    async fn download_part<F>(
        &self,
        spec: &DownloadSpec,
        url: &Url,
        dir: &Path,
        part: &Path,
        dest: &Path,
        on_progress: &mut F,
        cancel: &CancellationToken,
    ) -> Result<()>
    where
        F: FnMut(Progress) + Send,
    {
        let std_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(part)?;
        match std_file.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(Error::Busy(dest.display().to_string()));
            }
            Err(std::fs::TryLockError::Error(e)) if e.kind() == io::ErrorKind::Unsupported => {
                tracing::debug!("file locking unsupported here; continuing without lock");
            }
            Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
        }

        let (std_file, mut hasher, mut have) =
            resume_state(std_file, spec.size, cancel.clone()).await?;
        let mut file = tokio::fs::File::from_std(std_file);
        on_progress(Meter::new(spec.size, have).snapshot(have));

        if have < spec.size {
            ensure_disk_space(dir, spec.size - have)?;
            let fetched = self
                .fetch(spec, url, &mut file, &mut hasher, have, on_progress, cancel)
                .await;
            // Keep whatever reached the disk for a later resume.
            file.flush().await?;
            have = fetched?;
        }
        file.sync_all().await?;
        drop(file); // releases the lock; required before renaming on Windows

        if have != spec.size {
            return Err(Error::Incomplete {
                received: have,
                expected: spec.size,
            });
        }
        let actual = hex::encode(hasher.finalize());
        if actual != spec.sha256 {
            return Err(Error::HashMismatch {
                expected: spec.sha256.clone(),
                actual,
            });
        }
        tokio::fs::rename(part, dest).await?;
        sync_dir(dir);
        tracing::info!(path = %dest.display(), "model download verified");
        Ok(())
    }

    /// Streams the remainder of the file into `file`; returns the new number of bytes on disk.
    #[allow(clippy::too_many_arguments)]
    async fn fetch<F>(
        &self,
        spec: &DownloadSpec,
        url: &Url,
        file: &mut tokio::fs::File,
        hasher: &mut Sha256,
        mut have: u64,
        on_progress: &mut F,
        cancel: &CancellationToken,
    ) -> Result<u64>
    where
        F: FnMut(Progress) + Send,
    {
        let mut restarted = false;
        let response = loop {
            let response = tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(Error::Cancelled),
                r = self.get(url, have) => r?,
            };
            match response.status() {
                StatusCode::PARTIAL_CONTENT if have > 0 => {
                    let (start, total) = parse_content_range(response.headers())?;
                    if let Some(total) = total
                        && total != spec.size
                    {
                        return Err(Error::SizeMismatch {
                            expected: spec.size,
                            actual: total,
                        });
                    }
                    if start != have {
                        return Err(Error::Protocol(format!(
                            "asked for bytes from {have}, server sent from {start}"
                        )));
                    }
                    break response;
                }
                StatusCode::OK => {
                    if have > 0 {
                        tracing::debug!("server ignored range request; restarting download");
                        file.set_len(0).await?;
                        *hasher = Sha256::new();
                        have = 0;
                    }
                    if let Some(len) = response.content_length()
                        && len != spec.size
                    {
                        return Err(Error::SizeMismatch {
                            expected: spec.size,
                            actual: len,
                        });
                    }
                    break response;
                }
                StatusCode::RANGE_NOT_SATISFIABLE if have > 0 && !restarted => {
                    tracing::debug!("server rejected range; restarting download");
                    file.set_len(0).await?;
                    *hasher = Sha256::new();
                    have = 0;
                    restarted = true;
                }
                status => {
                    return Err(Error::HttpStatus {
                        status: status.as_u16(),
                        url: redact(url),
                    });
                }
            }
        };

        file.seek(SeekFrom::Start(have)).await?;
        let mut meter = Meter::new(spec.size, have);
        let mut body = response.bytes_stream();
        loop {
            let next = tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(Error::Cancelled),
                next = body.next() => next,
            };
            let Some(chunk) = next else { break };
            let chunk = chunk?;
            let len = chunk.len() as u64;
            if have + len > spec.size {
                return Err(Error::SizeMismatch {
                    expected: spec.size,
                    actual: have + len,
                });
            }
            file.write_all(&chunk).await?;
            hasher.update(&chunk);
            have += len;
            meter.tick(have, on_progress);
        }
        on_progress(meter.snapshot(have));
        Ok(have)
    }

    /// Sends a GET (optionally ranged), following allow-listed redirects.
    async fn get(&self, url: &Url, offset: u64) -> Result<reqwest::Response> {
        let mut current = url.clone();
        for _ in 0..=MAX_REDIRECTS {
            self.policy.check(&current)?;
            let mut request = self
                .client
                .get(current.clone())
                // Byte ranges must refer to the stored representation.
                .header(ACCEPT_ENCODING, "identity");
            if offset > 0 {
                request = request.header(RANGE, format!("bytes={offset}-"));
            }
            let response = request.send().await?;
            if !response.status().is_redirection() {
                return Ok(response);
            }
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| Error::Protocol("redirect without a valid Location".into()))?;
            current = current
                .join(location)
                .map_err(|_| Error::Protocol("redirect to an invalid URL".into()))?;
            tracing::debug!(to = %redact(&current), "following redirect");
        }
        Err(Error::TooManyRedirects)
    }
}

/// Opens the resume state of a partial file: truncates it if it is longer than expected,
/// otherwise re-hashes its contents. Runs on the blocking pool.
async fn resume_state(
    file: std::fs::File,
    expected: u64,
    cancel: CancellationToken,
) -> Result<(std::fs::File, Sha256, u64)> {
    tokio::task::spawn_blocking(move || {
        let mut file = file;
        let mut hasher = Sha256::new();
        let len = file.metadata()?.len();
        if len > expected {
            file.set_len(0)?;
            return Ok((file, hasher, 0));
        }
        if len > 0 {
            file.seek(SeekFrom::Start(0))?;
            let hashed = hash_reader(&mut (&mut file).take(len), &mut hasher, &cancel)?;
            if hashed != len {
                return Err(Error::Protocol("partial file changed while hashing".into()));
            }
            tracing::debug!(bytes = len, "resuming partial download");
        }
        Ok((file, hasher, len))
    })
    .await
    .map_err(|e| Error::Io(io::Error::other(e)))?
}

/// Re-verifies an existing file against `spec` (size, then SHA-256).
pub(crate) async fn verify_file(
    path: &Path,
    spec: &DownloadSpec,
    cancel: &CancellationToken,
) -> Result<bool> {
    let path = path.to_owned();
    let expected_size = spec.size;
    let expected_hash = spec.sha256.clone();
    let cancel = cancel.clone();
    tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(&path)?;
        if file.metadata()?.len() != expected_size {
            return Ok(false);
        }
        let mut hasher = Sha256::new();
        hash_reader(&mut file, &mut hasher, &cancel)?;
        Ok(hex::encode(hasher.finalize()) == expected_hash)
    })
    .await
    .map_err(|e| Error::Io(io::Error::other(e)))?
}

fn hash_reader(
    reader: &mut impl Read,
    hasher: &mut Sha256,
    cancel: &CancellationToken,
) -> Result<u64> {
    let mut buf = vec![0u8; REHASH_BUFFER];
    let mut total = 0u64;
    loop {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let n = match reader.read(&mut buf) {
            Ok(0) => return Ok(total),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        };
        hasher.update(&buf[..n]);
        total += n as u64;
    }
}

/// Parses `Content-Range: bytes <start>-<end>/<total|*>` into `(start, total)`.
fn parse_content_range(headers: &HeaderMap) -> Result<(u64, Option<u64>)> {
    let bad = || Error::Protocol("missing or malformed Content-Range".into());
    let value = headers
        .get(CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(bad)?;
    let rest = value.trim().strip_prefix("bytes ").ok_or_else(bad)?;
    let (range, total) = rest.split_once('/').ok_or_else(bad)?;
    let (start, end) = range.split_once('-').ok_or_else(bad)?;
    let start: u64 = start.trim().parse().map_err(|_| bad())?;
    let end: u64 = end.trim().parse().map_err(|_| bad())?;
    if end < start {
        return Err(bad());
    }
    let total = match total.trim() {
        "*" => None,
        t => Some(t.parse::<u64>().map_err(|_| bad())?),
    };
    if total.is_some_and(|t| end >= t) {
        return Err(bad());
    }
    Ok((start, total))
}

/// Fails if the file system holding `dir` has less than `remaining` (+ margin) bytes free.
/// Platforms where free space cannot be determined are not blocked.
fn ensure_disk_space(dir: &Path, remaining: u64) -> Result<()> {
    match fs4::available_space(dir) {
        Ok(available) => {
            let required = remaining.saturating_add(DISK_SPACE_MARGIN);
            if available < required {
                return Err(Error::InsufficientDiskSpace {
                    required,
                    available,
                });
            }
            Ok(())
        }
        Err(e) => {
            tracing::debug!(error = %e, "cannot determine free disk space; skipping check");
            Ok(())
        }
    }
}

/// Makes the rename durable (best effort; directories cannot be opened on Windows).
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Err(e) = std::fs::File::open(dir).and_then(|d| d.sync_all()) {
        tracing::debug!(error = %e, "could not sync model directory");
    }
    #[cfg(not(unix))]
    let _ = dir;
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::{header, header_exists, method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    fn payload(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 31 % 251) as u8).collect()
    }

    fn sha(data: &[u8]) -> String {
        hex::encode(Sha256::digest(data))
    }

    fn spec_for(server: &MockServer, file: &str, data: &[u8]) -> DownloadSpec {
        DownloadSpec {
            url: format!("{}/{file}", server.uri()),
            size: data.len() as u64,
            sha256: sha(data),
        }
    }

    fn downloader() -> Downloader {
        Downloader::with_policy(UrlPolicy::loopback_for_tests()).expect("client")
    }

    /// Lets wiremock serve a `Range: bytes=N-` request like a real server would.
    fn ranged(data: Vec<u8>) -> impl Fn(&Request) -> ResponseTemplate + Send + Sync + 'static {
        move |req: &Request| match req.headers.get("range").and_then(|v| v.to_str().ok()) {
            Some(r) => {
                let start: usize = r
                    .trim_start_matches("bytes=")
                    .trim_end_matches('-')
                    .parse()
                    .expect("range");
                ResponseTemplate::new(206)
                    .insert_header(
                        "content-range",
                        format!("bytes {start}-{}/{}", data.len() - 1, data.len()).as_str(),
                    )
                    .set_body_bytes(data[start..].to_vec())
            }
            None => ResponseTemplate::new(200).set_body_bytes(data.clone()),
        }
    }

    #[tokio::test]
    async fn full_download() {
        let server = MockServer::start().await;
        let data = payload(300_000);
        Mock::given(method("GET"))
            .and(path("/m.gguf"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(data.clone()))
            .expect(1)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("models/m.gguf");
        let reports = Arc::new(Mutex::new(Vec::new()));
        let sink = reports.clone();
        downloader()
            .download(
                &spec_for(&server, "m.gguf", &data),
                &dest,
                move |p| sink.lock().expect("lock").push(p),
                &CancellationToken::new(),
            )
            .await
            .expect("download");
        assert_eq!(std::fs::read(&dest).expect("read"), data);
        assert!(!part_path(&dest).exists());
        let reports = reports.lock().expect("lock");
        let last = reports.last().expect("progress");
        assert_eq!(last.downloaded, data.len() as u64);
        assert_eq!(last.total, data.len() as u64);
        assert!((last.fraction() - 1.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn resume_uses_range_and_rehashes_part() {
        let server = MockServer::start().await;
        let data = payload(200_000);
        let split = 123_457;
        Mock::given(method("GET"))
            .and(path("/m.gguf"))
            .and(header("range", format!("bytes={split}-").as_str()))
            .respond_with(ranged(data.clone()))
            .expect(1)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("m.gguf");
        std::fs::write(part_path(&dest), &data[..split]).expect("seed part");
        let first = Arc::new(Mutex::new(None));
        let sink = first.clone();
        downloader()
            .download(
                &spec_for(&server, "m.gguf", &data),
                &dest,
                move |p| {
                    sink.lock().expect("lock").get_or_insert(p);
                },
                &CancellationToken::new(),
            )
            .await
            .expect("download");
        assert_eq!(std::fs::read(&dest).expect("read"), data);
        assert_eq!(
            first.lock().expect("lock").expect("progress").downloaded,
            split as u64
        );
    }

    #[tokio::test]
    async fn resume_with_corrupt_part_fails_and_cleans_up() {
        let server = MockServer::start().await;
        let data = payload(100_000);
        Mock::given(method("GET"))
            .respond_with(ranged(data.clone()))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("m.gguf");
        std::fs::write(part_path(&dest), vec![0u8; 5000]).expect("seed part");
        let err = downloader()
            .download(
                &spec_for(&server, "m.gguf", &data),
                &dest,
                |_| {},
                &CancellationToken::new(),
            )
            .await
            .expect_err("must fail");
        assert!(matches!(err, Error::HashMismatch { .. }), "{err:?}");
        assert!(!dest.exists());
        assert!(!part_path(&dest).exists());
        // The next attempt starts from scratch and succeeds.
        downloader()
            .download(
                &spec_for(&server, "m.gguf", &data),
                &dest,
                |_| {},
                &CancellationToken::new(),
            )
            .await
            .expect("retry");
        assert_eq!(std::fs::read(&dest).expect("read"), data);
    }

    #[tokio::test]
    async fn server_ignoring_range_restarts() {
        let server = MockServer::start().await;
        let data = payload(50_000);
        Mock::given(method("GET"))
            .and(header_exists("range"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(data.clone()))
            .expect(1)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("m.gguf");
        std::fs::write(part_path(&dest), &data[..10_000]).expect("seed part");
        downloader()
            .download(
                &spec_for(&server, "m.gguf", &data),
                &dest,
                |_| {},
                &CancellationToken::new(),
            )
            .await
            .expect("download");
        assert_eq!(std::fs::read(&dest).expect("read"), data);
    }

    #[tokio::test]
    async fn complete_part_needs_no_network() {
        let server = MockServer::start().await;
        let data = payload(10_000);
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("m.gguf");
        std::fs::write(part_path(&dest), &data).expect("seed part");
        downloader()
            .download(
                &spec_for(&server, "m.gguf", &data),
                &dest,
                |_| {},
                &CancellationToken::new(),
            )
            .await
            .expect("download");
        assert_eq!(std::fs::read(&dest).expect("read"), data);
    }

    #[tokio::test]
    async fn hash_mismatch_deletes_everything() {
        let server = MockServer::start().await;
        let data = payload(64_000);
        let mut evil = data.clone();
        evil[1000] ^= 0xff;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(evil))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("m.gguf");
        let err = downloader()
            .download(
                &spec_for(&server, "m.gguf", &data),
                &dest,
                |_| {},
                &CancellationToken::new(),
            )
            .await
            .expect_err("must fail");
        match err {
            Error::HashMismatch { expected, actual } => {
                assert_eq!(expected, sha(&data));
                assert_ne!(actual, expected);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(!dest.exists());
        assert!(!part_path(&dest).exists());
    }

    #[tokio::test]
    async fn oversized_body_is_rejected() {
        let server = MockServer::start().await;
        let data = payload(1000);
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(payload(2000)))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("m.gguf");
        let err = downloader()
            .download(
                &spec_for(&server, "m.gguf", &data),
                &dest,
                |_| {},
                &CancellationToken::new(),
            )
            .await
            .expect_err("must fail");
        assert!(matches!(err, Error::SizeMismatch { .. }), "{err:?}");
        assert!(!part_path(&dest).exists());
    }

    #[tokio::test]
    async fn allowed_redirect_is_followed_with_range() {
        let server = MockServer::start().await;
        let data = payload(40_000);
        Mock::given(method("GET"))
            .and(path("/resolve/m.gguf"))
            .respond_with(
                ResponseTemplate::new(302).insert_header("location", "/cdn/m.gguf?sig=secret"),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/cdn/m.gguf"))
            .and(header("range", "bytes=1000-"))
            .respond_with(ranged(data.clone()))
            .expect(1)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("m.gguf");
        std::fs::write(part_path(&dest), &data[..1000]).expect("seed part");
        downloader()
            .download(
                &spec_for(&server, "resolve/m.gguf", &data),
                &dest,
                |_| {},
                &CancellationToken::new(),
            )
            .await
            .expect("download");
        assert_eq!(std::fs::read(&dest).expect("read"), data);
    }

    #[tokio::test]
    async fn disallowed_redirect_is_refused() {
        let server = MockServer::start().await;
        let data = payload(1000);
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", "https://evil.example.com/m.gguf?token=abc"),
            )
            .expect(1)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("m.gguf");
        let err = downloader()
            .download(
                &spec_for(&server, "m.gguf", &data),
                &dest,
                |_| {},
                &CancellationToken::new(),
            )
            .await
            .expect_err("must fail");
        match err {
            Error::DisallowedUrl { url, .. } => {
                assert!(url.contains("evil.example.com"));
                assert!(!url.contains("token"), "query must be redacted: {url}");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(!dest.exists());
    }

    #[tokio::test]
    async fn redirect_loop_is_bounded() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "/again"))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let err = downloader()
            .download(
                &spec_for(&server, "m.gguf", b"x"),
                &dir.path().join("m.gguf"),
                |_| {},
                &CancellationToken::new(),
            )
            .await
            .expect_err("must fail");
        assert!(matches!(err, Error::TooManyRedirects), "{err:?}");
    }

    #[tokio::test]
    async fn http_error_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let err = downloader()
            .download(
                &spec_for(&server, "m.gguf", b"x"),
                &dir.path().join("m.gguf"),
                |_| {},
                &CancellationToken::new(),
            )
            .await
            .expect_err("must fail");
        assert!(
            matches!(err, Error::HttpStatus { status: 404, .. }),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn cancel_while_waiting_keeps_part() {
        let server = MockServer::start().await;
        let data = payload(1000);
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(data.clone())
                    .set_delay(Duration::from_secs(30)),
            )
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("m.gguf");
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            trigger.cancel();
        });
        let started = Instant::now();
        let err = downloader()
            .download(&spec_for(&server, "m.gguf", &data), &dest, |_| {}, &cancel)
            .await
            .expect_err("must be cancelled");
        assert!(matches!(err, Error::Cancelled), "{err:?}");
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(!dest.exists());
        assert!(part_path(&dest).exists(), "part file is kept for resuming");
    }

    #[tokio::test]
    async fn cancelled_before_start() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let dir = tempfile::tempdir().expect("tmp");
        let spec = DownloadSpec {
            url: "http://127.0.0.1:9/m.gguf".into(),
            size: 1,
            sha256: "0".repeat(64),
        };
        let err = downloader()
            .download(&spec, &dir.path().join("m.gguf"), |_| {}, &cancel)
            .await
            .expect_err("cancelled");
        assert!(matches!(err, Error::Cancelled));
    }

    #[tokio::test]
    async fn locked_part_reports_busy() {
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("m.gguf");
        let holder = std::fs::File::create(part_path(&dest)).expect("create");
        holder.try_lock().expect("lock");
        let spec = DownloadSpec {
            url: "http://127.0.0.1:9/m.gguf".into(),
            size: 10,
            sha256: "0".repeat(64),
        };
        let err = downloader()
            .download(&spec, &dest, |_| {}, &CancellationToken::new())
            .await
            .expect_err("busy");
        assert!(matches!(err, Error::Busy(_)), "{err:?}");
    }

    #[tokio::test]
    async fn existing_valid_file_is_kept() {
        let data = payload(5000);
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("m.gguf");
        std::fs::write(&dest, &data).expect("seed");
        let spec = DownloadSpec {
            // Unreachable on purpose: no request may be made.
            url: "http://127.0.0.1:9/m.gguf".into(),
            size: data.len() as u64,
            sha256: sha(&data),
        };
        downloader()
            .download(&spec, &dest, |_| {}, &CancellationToken::new())
            .await
            .expect("verified in place");
    }

    #[tokio::test]
    async fn malformed_spec_fails_closed() {
        let dir = tempfile::tempdir().expect("tmp");
        let spec = DownloadSpec {
            url: "https://huggingface.co/x".into(),
            size: 10,
            sha256: "TODO(verify)".into(),
        };
        let err = Downloader::new()
            .expect("client")
            .download(
                &spec,
                &dir.path().join("x"),
                |_| {},
                &CancellationToken::new(),
            )
            .await
            .expect_err("fail closed");
        assert!(matches!(err, Error::InvalidRequest(_)));
    }

    #[test]
    fn policy_rules() {
        let p = UrlPolicy::hugging_face();
        let ok = |u: &str| p.check(&Url::parse(u).expect("url")).is_ok();
        assert!(ok("https://huggingface.co/Qwen/x/resolve/abc/f.gguf"));
        assert!(ok("https://HuggingFace.co/x"));
        assert!(ok("https://cdn-lfs.huggingface.co/x"));
        assert!(ok("https://cdn-lfs-us-1.hf.co/x"));
        assert!(ok("https://cas-bridge.xethub.hf.co/x?sig=1"));
        assert!(ok("https://us.aws.cdn.hf.co/xet-bridge-us/abc"));
        assert!(
            ok("https://huggingface.co:443/x"),
            "default port is normalized"
        );
        assert!(!ok("http://huggingface.co/x"));
        assert!(!ok("https://huggingface.co:8443/x"));
        assert!(!ok("https://user:pw@huggingface.co/x"));
        assert!(!ok("https://huggingface.co.evil.com/x"));
        assert!(!ok("https://evilhuggingface.co/x"));
        assert!(!ok("https://evil-hf.co/x"));
        assert!(!ok("https://huggingface.co./x"));
        assert!(!ok("https://1.2.3.4/x"));
        assert!(!ok("https://[::1]/x"));
        assert!(!ok("http://127.0.0.1:8080/x"), "loopback only in tests");
        assert!(!ok("ftp://huggingface.co/x"));
        assert!(!ok("file:///etc/passwd"));
    }

    #[test]
    fn custom_policy_matches_on_label_boundaries() {
        let p = UrlPolicy::new(&["mirror.example.org"], &["cdn.example.net"]);
        let ok = |u: &str| p.check(&Url::parse(u).expect("url")).is_ok();
        assert!(ok("https://mirror.example.org/x"));
        assert!(!ok("https://a.mirror.example.org/x"));
        assert!(ok("https://cdn.example.net/x"));
        assert!(ok("https://eu.cdn.example.net/x"));
        assert!(!ok("https://xcdn.example.net/x"));
    }

    #[test]
    fn content_range_parsing() {
        let parse = |v: &str| {
            let mut h = HeaderMap::new();
            h.insert(CONTENT_RANGE, HeaderValue::from_str(v).expect("header"));
            parse_content_range(&h)
        };
        assert_eq!(parse("bytes 100-199/200").expect("ok"), (100, Some(200)));
        assert_eq!(parse("bytes 0-0/*").expect("ok"), (0, None));
        assert!(parse("bytes 100-199/150").is_err());
        assert!(parse("bytes 200-100/300").is_err());
        assert!(parse("items 0-1/2").is_err());
        assert!(parse("bytes */200").is_err());
        assert!(parse_content_range(&HeaderMap::new()).is_err());
    }

    #[test]
    fn part_path_appends_suffix() {
        assert_eq!(
            part_path(Path::new("/a/b/m.gguf")),
            PathBuf::from("/a/b/m.gguf.part")
        );
    }

    #[test]
    fn disk_space_check() {
        let dir = tempfile::tempdir().expect("tmp");
        ensure_disk_space(dir.path(), 1).expect("one byte fits");
        let err = ensure_disk_space(dir.path(), u64::MAX / 2).expect_err("too big");
        assert!(matches!(err, Error::InsufficientDiskSpace { .. }));
    }
}
