// SPDX-License-Identifier: GPL-3.0-or-later
//! WebDAV (Nextcloud, ownCloud, Synology, Apache `mod_dav`, rclone serve, ...).
//!
//! * `PROPFIND` with `Depth: 1` per collection (recursing manually, since `Depth: infinity`
//!   is disabled on most servers), parsed with quick-xml by local element names so any
//!   namespace prefix works;
//! * `GET`, `PUT` with `If-None-Match: *` / `If-Match`, `DELETE`, and `MKCOL` for missing
//!   parent collections;
//! * HTTP basic authentication over rustls. Redirects are not followed.

use crate::backend::{ObjectInfo, Precondition, SyncBackend, validate_name, validate_prefix};
use crate::error::SyncError;
use async_trait::async_trait;
use cloud_inventory::http::{Buffered, retry_after};
use percent_encoding::percent_decode_str;
use quick_xml::events::Event;
use reqwest::{Method, StatusCode};
use std::collections::HashSet;
use std::fmt;
use std::sync::Mutex;
use std::time::Duration;
use url::Url;
use zeroize::{Zeroize, ZeroizeOnDrop};

const MAX_BODY: usize = 512 * 1024 * 1024;
const MAX_DEPTH: usize = 8;
const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/><d:getcontentlength/><d:getetag/></d:prop></d:propfind>"#;

/// Basic-auth credentials.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct WebDavCredentials {
    pub username: String,
    pub password: String,
}

impl fmt::Debug for WebDavCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebDavCredentials")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// WebDAV backend rooted at a collection URL.
#[derive(Debug)]
pub struct WebDavBackend {
    http: reqwest::Client,
    base: Url,
    credentials: Option<WebDavCredentials>,
    known_dirs: Mutex<HashSet<String>>,
}

fn http_client() -> Result<reqwest::Client, SyncError> {
    reqwest::Client::builder()
        .use_rustls_tls()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .user_agent(concat!("vaultport/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(SyncError::from)
}

fn status_error(name: &str, resp: &Buffered) -> SyncError {
    match resp.status.as_u16() {
        401 | 403 => SyncError::Auth(format!("HTTP {}", resp.status.as_u16())),
        404 => SyncError::NotFound(name.to_owned()),
        412 => SyncError::PreconditionFailed(name.to_owned()),
        429 | 503 => SyncError::RateLimited {
            retry_after: retry_after(&resp.headers),
        },
        s => SyncError::Backend(format!("HTTP {s} for {name}: {}", resp.snippet())),
    }
}

impl WebDavBackend {
    /// `base` is the collection that acts as the root, e.g.
    /// `https://cloud.example.com/remote.php/dav/files/alice/vaultport/`.
    pub fn new(base: &str, credentials: Option<WebDavCredentials>) -> Result<Self, SyncError> {
        let base = cloud_inventory::http::base_url(base)?;
        Ok(Self {
            http: http_client()?,
            base,
            credentials,
            known_dirs: Mutex::new(HashSet::new()),
        })
    }

    fn url_of(&self, name: &str) -> Result<Url, SyncError> {
        // Valid names contain only URL-safe characters, so no escaping is needed.
        self.base
            .join(name)
            .map_err(|e| SyncError::InvalidName(format!("{name}: {e}")))
    }

    async fn send(
        &self,
        method: Method,
        url: Url,
        headers: &[(&str, String)],
        body: Option<Vec<u8>>,
    ) -> Result<Buffered, SyncError> {
        let mut req = self.http.request(method, url);
        if let Some(c) = &self.credentials {
            req = req.basic_auth(&c.username, Some(&c.password));
        }
        for (k, v) in headers {
            req = req.header(*k, v);
        }
        if let Some(b) = body {
            req = req.body(b);
        }
        let resp = req.send().await?;
        Ok(Buffered::read(resp, MAX_BODY).await?)
    }

    async fn ensure_dirs(&self, name: &str) -> Result<(), SyncError> {
        let parts: Vec<&str> = name.split('/').collect();
        let mut path = String::new();
        for part in &parts[..parts.len().saturating_sub(1)] {
            path.push_str(part);
            path.push('/');
            let known = self
                .known_dirs
                .lock()
                .map_err(|_| SyncError::Backend("lock poisoned".into()))?
                .contains(&path);
            if known {
                continue;
            }
            let resp = self
                .send(
                    Method::from_bytes(b"MKCOL").map_err(|e| SyncError::Backend(e.to_string()))?,
                    self.url_of(&path)?,
                    &[],
                    None,
                )
                .await?;
            // 201 Created, 405 Method Not Allowed (already exists); some servers send 301.
            match resp.status.as_u16() {
                200..=299 | 301 | 405 => {}
                _ => return Err(status_error(&path, &resp)),
            }
            self.known_dirs
                .lock()
                .map_err(|_| SyncError::Backend("lock poisoned".into()))?
                .insert(path.clone());
        }
        Ok(())
    }

    /// Lists one collection; returns `(files, subcollections)` as names relative to the root.
    async fn propfind(&self, dir: &str) -> Result<(Vec<ObjectInfo>, Vec<String>), SyncError> {
        let url = self.url_of(dir)?;
        let resp = self
            .send(
                Method::from_bytes(b"PROPFIND").map_err(|e| SyncError::Backend(e.to_string()))?,
                url.clone(),
                &[
                    ("depth", "1".to_owned()),
                    ("content-type", "application/xml; charset=utf-8".to_owned()),
                ],
                Some(PROPFIND_BODY.as_bytes().to_vec()),
            )
            .await?;
        if resp.status == StatusCode::NOT_FOUND {
            return Ok((Vec::new(), Vec::new()));
        }
        if resp.status.as_u16() != 207 {
            return Err(status_error(dir, &resp));
        }
        let entries = parse_multistatus(&resp.body)?;
        let base_path = percent_decode_str(self.base.path())
            .decode_utf8_lossy()
            .into_owned();
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        for e in entries {
            // `href` is an absolute path or a full URL.
            let href_path = match Url::parse(&e.href) {
                Ok(u) => u.path().to_owned(),
                Err(_) => e.href.clone(),
            };
            let href_path = percent_decode_str(&href_path)
                .decode_utf8_lossy()
                .into_owned();
            let Some(rel) = href_path.strip_prefix(&base_path) else {
                continue;
            };
            let rel = rel.trim_end_matches('/');
            if rel.is_empty() || rel == dir.trim_end_matches('/') {
                continue; // the collection itself
            }
            if validate_name(rel).is_err() {
                if crate::backend::is_conflict_copy(rel) {
                    tracing::warn!(name = rel, "skipping conflict copy");
                }
                continue;
            }
            if e.collection {
                dirs.push(format!("{rel}/"));
            } else {
                files.push(ObjectInfo {
                    name: rel.to_owned(),
                    size: e.size,
                    etag: e.etag,
                });
            }
        }
        Ok((files, dirs))
    }
}

#[derive(Debug, Default)]
struct DavEntry {
    href: String,
    collection: bool,
    size: u64,
    etag: Option<String>,
}

fn local_name(raw: &[u8]) -> &[u8] {
    raw.rsplit(|b| *b == b':').next().unwrap_or(raw)
}

/// Parses a `207 Multi-Status` body into entries.
fn parse_multistatus(body: &[u8]) -> Result<Vec<DavEntry>, SyncError> {
    let mut reader = quick_xml::Reader::from_reader(body);
    reader.config_mut().trim_text(true);
    let mut out = Vec::new();
    let mut current: Option<DavEntry> = None;
    let mut path: Vec<Vec<u8>> = Vec::new();
    let mut buf = Vec::new();
    loop {
        let ev = reader
            .read_event_into(&mut buf)
            .map_err(|e| SyncError::Backend(format!("PROPFIND response: {e}")))?;
        match ev {
            Event::Start(s) => {
                let n = local_name(s.name().as_ref()).to_ascii_lowercase();
                if n == b"response" {
                    current = Some(DavEntry::default());
                }
                if n == b"collection"
                    && let Some(c) = current.as_mut()
                {
                    c.collection = true;
                }
                path.push(n);
            }
            Event::Empty(s) => {
                let n = local_name(s.name().as_ref()).to_ascii_lowercase();
                if n == b"collection"
                    && let Some(c) = current.as_mut()
                {
                    c.collection = true;
                }
            }
            Event::Text(t) => {
                let text = t
                    .decode()
                    .map_err(|e| SyncError::Backend(format!("PROPFIND response: {e}")))?;
                append_text(current.as_mut(), path.last(), &text);
            }
            Event::GeneralRef(r) => {
                // Entity references are reported separately since quick-xml 0.38.
                let resolved = match r.resolve_char_ref() {
                    Ok(Some(c)) => Some(c),
                    _ => match &*r {
                        b"amp" => Some('&'),
                        b"lt" => Some('<'),
                        b"gt" => Some('>'),
                        b"quot" => Some('"'),
                        b"apos" => Some('\''),
                        _ => None,
                    },
                };
                if let Some(c) = resolved {
                    append_text(current.as_mut(), path.last(), c.encode_utf8(&mut [0u8; 4]));
                }
            }
            Event::End(e) => {
                let n = local_name(e.name().as_ref()).to_ascii_lowercase();
                path.pop();
                if n == b"response"
                    && let Some(c) = current.take()
                {
                    out.push(c);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
        if out.len() > 1_000_000 {
            return Err(SyncError::Backend("PROPFIND response too large".into()));
        }
    }
    for e in &mut out {
        e.etag = e.etag.take().map(|t| t.trim().to_owned());
    }
    Ok(out)
}

fn append_text(current: Option<&mut DavEntry>, element: Option<&Vec<u8>>, text: &str) {
    let (Some(c), Some(el)) = (current, element) else {
        return;
    };
    match el.as_slice() {
        b"href" => c.href.push_str(text),
        b"getcontentlength" => c.size = text.trim().parse().unwrap_or(0),
        b"getetag" => c.etag.get_or_insert_with(String::new).push_str(text),
        _ => {}
    }
}

#[async_trait]
impl SyncBackend for WebDavBackend {
    async fn list(&self, prefix: &str) -> Result<Vec<ObjectInfo>, SyncError> {
        validate_prefix(prefix)?;
        let start = prefix
            .rsplit_once('/')
            .map_or(String::new(), |(d, _)| format!("{d}/"));
        let mut out = Vec::new();
        let mut stack = vec![(start, 0usize)];
        while let Some((dir, depth)) = stack.pop() {
            let (files, dirs) = self.propfind(&dir).await?;
            out.extend(files.into_iter().filter(|f| f.name.starts_with(prefix)));
            if depth < MAX_DEPTH {
                stack.extend(dirs.into_iter().map(|d| (d, depth + 1)));
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    async fn get(&self, name: &str) -> Result<Vec<u8>, SyncError> {
        validate_name(name)?;
        let resp = self
            .send(Method::GET, self.url_of(name)?, &[], None)
            .await?;
        if resp.status.is_success() {
            Ok(resp.body)
        } else {
            Err(status_error(name, &resp))
        }
    }

    async fn put(
        &self,
        name: &str,
        bytes: &[u8],
        precondition: Option<Precondition>,
    ) -> Result<String, SyncError> {
        validate_name(name)?;
        self.ensure_dirs(name).await?;
        let mut headers = vec![("content-type", "application/octet-stream".to_owned())];
        match precondition {
            Some(Precondition::IfNoneMatch) => headers.push(("if-none-match", "*".to_owned())),
            Some(Precondition::IfMatch(etag)) => headers.push(("if-match", etag)),
            None => {}
        }
        let resp = self
            .send(
                Method::PUT,
                self.url_of(name)?,
                &headers,
                Some(bytes.to_vec()),
            )
            .await?;
        if !resp.status.is_success() {
            return Err(status_error(name, &resp));
        }
        Ok(resp
            .headers
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned())
    }

    async fn delete(&self, name: &str) -> Result<(), SyncError> {
        validate_name(name)?;
        let resp = self
            .send(Method::DELETE, self.url_of(name)?, &[], None)
            .await?;
        if resp.status.is_success() || resp.status == StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(status_error(name, &resp))
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn multistatus_parsing() {
        let xml = br#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:">
  <D:response><D:href>/dav/root/a/</D:href><D:propstat><D:prop>
    <D:resourcetype><D:collection/></D:resourcetype></D:prop>
    <D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
  <D:response><D:href>/dav/root/a/x%2Dy.seg</D:href><D:propstat><D:prop>
    <D:resourcetype/><D:getcontentlength>42</D:getcontentlength>
    <D:getetag>&quot;abc&quot;</D:getetag></D:prop></D:propstat></D:response>
  <response xmlns="DAV:"><href>https://h/dav/root/a/sub/</href><propstat><prop>
    <resourcetype><collection></collection></resourcetype></prop></propstat></response>
</D:multistatus>"#;
        let e = parse_multistatus(xml).unwrap();
        assert_eq!(e.len(), 3);
        assert!(e[0].collection);
        assert_eq!(e[1].href, "/dav/root/a/x%2Dy.seg");
        assert_eq!(e[1].size, 42);
        assert_eq!(e[1].etag.as_deref(), Some("\"abc\""));
        assert!(!e[1].collection);
        assert!(e[2].collection);
        assert!(parse_multistatus(b"<a><b></a>").is_err());
    }
}
