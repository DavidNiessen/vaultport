// SPDX-License-Identifier: GPL-3.0-or-later
//! S3-compatible object storage (Amazon S3, Cloudflare R2, MinIO, Backblaze B2, Wasabi, ...).
//!
//! Requests are signed with the SigV4 implementation of `cloud-inventory`. Both path-style
//! (`https://endpoint/bucket/key`, typical for MinIO) and virtual-hosted-style
//! (`https://bucket.endpoint/key`, AWS default) addressing are supported. Listing uses
//! `ListObjectsV2` with continuation tokens. Create-only uploads send `If-None-Match: *`
//! (supported by AWS S3, R2 and recent MinIO); disable [`S3Config::conditional_writes`] for
//! services that reject the header — segment names are random, so collisions are not a
//! practical concern there.

use crate::backend::{ObjectInfo, Precondition, SyncBackend, validate_name, validate_prefix};
use crate::error::SyncError;
use async_trait::async_trait;
use cloud_inventory::http::{Buffered, retry_after};
use cloud_inventory::sigv4::{self, Credentials, SigningParams};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use url::Url;

const MAX_BODY: usize = 512 * 1024 * 1024;
const MAX_PAGES: usize = 10_000;

/// Bucket addressing style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Addressing {
    /// `https://endpoint/bucket/key`
    #[default]
    Path,
    /// `https://bucket.endpoint/key`
    VirtualHost,
}

/// Non-secret S3 settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S3Config {
    /// Service endpoint, e.g. `https://s3.eu-central-1.amazonaws.com`,
    /// `https://<account>.r2.cloudflarestorage.com`, `https://s3.us-west-004.backblazeb2.com`.
    pub endpoint: String,
    /// Signing region (`auto` for R2, `us-east-1` for most MinIO setups).
    pub region: String,
    pub bucket: String,
    /// Optional key prefix inside the bucket (must end with `/`).
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub addressing: Addressing,
    /// Send `If-None-Match: *` on create-only uploads.
    #[serde(default = "yes")]
    pub conditional_writes: bool,
}

fn yes() -> bool {
    true
}

/// S3-compatible backend.
#[derive(Debug)]
pub struct S3Backend {
    http: reqwest::Client,
    config: S3Config,
    credentials: Credentials,
    bucket_url: Url,
}

fn check_bucket(b: &str) -> Result<(), SyncError> {
    let ok = (3..=63).contains(&b.len())
        && b.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'.')
        && !b.starts_with(['-', '.'])
        && !b.ends_with(['-', '.']);
    if ok {
        Ok(())
    } else {
        Err(SyncError::Backend(format!("invalid bucket name {b:?}")))
    }
}

impl S3Backend {
    pub fn new(config: S3Config, credentials: Credentials) -> Result<Self, SyncError> {
        check_bucket(&config.bucket)?;
        if !config.prefix.is_empty() && !config.prefix.ends_with('/') {
            return Err(SyncError::InvalidName(config.prefix.clone()));
        }
        validate_prefix(&config.prefix)?;
        if config.region.is_empty()
            || !config
                .region
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(SyncError::Backend("invalid region".into()));
        }
        let endpoint = cloud_inventory::http::base_url(&config.endpoint)?;
        let bucket_url = match config.addressing {
            Addressing::Path => {
                let mut u = endpoint.clone();
                u.set_path(&format!("{}{}", endpoint.path(), config.bucket));
                u
            }
            Addressing::VirtualHost => {
                let host = endpoint
                    .host_str()
                    .ok_or_else(|| SyncError::Backend("endpoint without host".into()))?;
                let mut u = endpoint.clone();
                u.set_host(Some(&format!("{}.{host}", config.bucket)))
                    .map_err(|e| SyncError::Backend(e.to_string()))?;
                u
            }
        };
        let http = reqwest::Client::builder()
            .use_rustls_tls()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(300))
            .user_agent(concat!("ssh-client/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            http,
            config,
            credentials,
            bucket_url,
        })
    }

    fn object_url(&self, name: &str) -> Result<Url, SyncError> {
        validate_name(name)?;
        // Valid names only contain unreserved characters and `/`, so the path is already in
        // canonical (SigV4) form.
        let mut u = self.bucket_url.clone();
        let base = u.path().trim_end_matches('/').to_owned();
        u.set_path(&format!("{base}/{}{name}", self.config.prefix));
        Ok(u)
    }

    async fn send(
        &self,
        method: Method,
        url: Url,
        mut headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Result<Buffered, SyncError> {
        let payload = sigv4::sha256_hex(&body);
        let params = SigningParams::s3(&self.credentials, &self.config.region);
        let added = sigv4::sign(method.as_str(), &url, &headers, &payload, &params)?;
        headers.extend(added);
        let mut req = self.http.request(method, url);
        for (k, v) in &headers {
            req = req.header(k, v);
        }
        let resp = req.body(body).send().await?;
        Ok(Buffered::read(resp, MAX_BODY).await?)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ListBucketResult {
    #[serde(default)]
    contents: Vec<Contents>,
    #[serde(default)]
    is_truncated: bool,
    #[serde(default)]
    next_continuation_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Contents {
    key: String,
    #[serde(default)]
    size: u64,
    #[serde(default, rename = "ETag")]
    etag: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct S3ErrorBody {
    #[serde(default)]
    code: String,
}

fn error(name: &str, resp: &Buffered) -> SyncError {
    let code = quick_xml::de::from_str::<S3ErrorBody>(&String::from_utf8_lossy(&resp.body))
        .map(|e| e.code)
        .unwrap_or_default();
    match (resp.status.as_u16(), code.as_str()) {
        (_, "SlowDown" | "Throttling" | "TooManyRequests") | (429 | 503, _) => {
            SyncError::RateLimited {
                retry_after: retry_after(&resp.headers),
            }
        }
        (401 | 403, _) => SyncError::Auth(if code.is_empty() {
            format!("HTTP {}", resp.status.as_u16())
        } else {
            code
        }),
        (404, "NoSuchKey" | "") => SyncError::NotFound(name.to_owned()),
        (412, _) | (409, "ConditionalRequestConflict") => {
            SyncError::PreconditionFailed(name.to_owned())
        }
        (s, c) => SyncError::Backend(format!("HTTP {s} {c} for {name}")),
    }
}

#[async_trait]
impl SyncBackend for S3Backend {
    async fn list(&self, prefix: &str) -> Result<Vec<ObjectInfo>, SyncError> {
        validate_prefix(prefix)?;
        let full_prefix = format!("{}{prefix}", self.config.prefix);
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut pairs = vec![("list-type", "2"), ("prefix", full_prefix.as_str())];
            if let Some(t) = &token {
                pairs.push(("continuation-token", t.as_str()));
            }
            let mut url = self.bucket_url.clone();
            url.set_query(Some(&sigv4::encode_query(&pairs)));
            let resp = self.send(Method::GET, url, Vec::new(), Vec::new()).await?;
            if !resp.status.is_success() {
                return Err(error(prefix, &resp));
            }
            let page: ListBucketResult =
                quick_xml::de::from_str(&String::from_utf8_lossy(&resp.body))
                    .map_err(|e| SyncError::Backend(format!("ListObjectsV2 response: {e}")))?;
            for c in page.contents {
                let Some(name) = c.key.strip_prefix(&self.config.prefix) else {
                    continue;
                };
                if validate_name(name).is_err() || !name.starts_with(prefix) {
                    continue;
                }
                out.push(ObjectInfo {
                    name: name.to_owned(),
                    size: c.size,
                    etag: c.etag,
                });
            }
            match (page.is_truncated, page.next_continuation_token) {
                (true, Some(t)) if token.as_deref() != Some(t.as_str()) => token = Some(t),
                (true, _) => return Err(SyncError::Backend("broken S3 pagination".into())),
                (false, _) => {
                    out.sort_by(|a, b| a.name.cmp(&b.name));
                    return Ok(out);
                }
            }
        }
        Err(SyncError::Backend("too many S3 pages".into()))
    }

    async fn get(&self, name: &str) -> Result<Vec<u8>, SyncError> {
        let resp = self
            .send(Method::GET, self.object_url(name)?, Vec::new(), Vec::new())
            .await?;
        if resp.status.is_success() {
            Ok(resp.body)
        } else {
            Err(error(name, &resp))
        }
    }

    async fn put(
        &self,
        name: &str,
        bytes: &[u8],
        precondition: Option<Precondition>,
    ) -> Result<String, SyncError> {
        let mut headers = vec![(
            "content-type".to_owned(),
            "application/octet-stream".to_owned(),
        )];
        match precondition {
            Some(Precondition::IfNoneMatch) if self.config.conditional_writes => {
                headers.push(("if-none-match".to_owned(), "*".to_owned()));
            }
            Some(Precondition::IfMatch(etag)) => headers.push(("if-match".to_owned(), etag)),
            _ => {}
        }
        let resp = self
            .send(Method::PUT, self.object_url(name)?, headers, bytes.to_vec())
            .await?;
        if !resp.status.is_success() {
            return Err(error(name, &resp));
        }
        Ok(resp
            .headers
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned())
    }

    async fn delete(&self, name: &str) -> Result<(), SyncError> {
        let resp = self
            .send(
                Method::DELETE,
                self.object_url(name)?,
                Vec::new(),
                Vec::new(),
            )
            .await?;
        if resp.status.is_success() || resp.status.as_u16() == 404 {
            Ok(())
        } else {
            Err(error(name, &resp))
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn cfg(addressing: Addressing) -> S3Config {
        S3Config {
            endpoint: "https://s3.example.com".into(),
            region: "us-east-1".into(),
            bucket: "my-bucket".into(),
            prefix: "ssh/".into(),
            addressing,
            conditional_writes: true,
        }
    }

    #[test]
    fn urls() {
        let c = Credentials::new("a", "b");
        let p = S3Backend::new(cfg(Addressing::Path), c.clone()).unwrap();
        assert_eq!(
            p.object_url("v/x.seg").unwrap().as_str(),
            "https://s3.example.com/my-bucket/ssh/v/x.seg"
        );
        assert_eq!(p.bucket_url.as_str(), "https://s3.example.com/my-bucket");
        let v = S3Backend::new(cfg(Addressing::VirtualHost), c.clone()).unwrap();
        assert_eq!(
            v.object_url("v/x.seg").unwrap().as_str(),
            "https://my-bucket.s3.example.com/ssh/v/x.seg"
        );
        assert!(p.object_url("../x").is_err());
        let mut bad = cfg(Addressing::Path);
        bad.bucket = "Bad_Bucket".into();
        assert!(S3Backend::new(bad, c).is_err());
    }
}
