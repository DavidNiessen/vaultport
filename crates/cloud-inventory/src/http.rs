// SPDX-License-Identifier: GPL-3.0-or-later
//! Small HTTP helpers shared by the providers (and reused by the sync backends).

use crate::error::InventoryError;
use reqwest::StatusCode;
use reqwest::header::HeaderMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

/// Maximum size of a provider response body we are willing to buffer.
pub const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
/// Upper bound on pages followed for one listing (protects against pagination loops).
pub const MAX_PAGES: usize = 1000;

/// Builds the HTTP client used by the inventory clients: rustls, no redirects (so bearer
/// tokens and signatures are never replayed to another origin), sane timeouts.
pub fn default_client() -> Result<reqwest::Client, InventoryError> {
    reqwest::Client::builder()
        .use_rustls_tls()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("ssh-client/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(InventoryError::from)
}

/// A fully buffered HTTP response.
#[derive(Debug)]
pub struct Buffered {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl Buffered {
    /// Reads a response body, failing if it exceeds `limit` bytes.
    pub async fn read(mut resp: reqwest::Response, limit: usize) -> Result<Self, InventoryError> {
        let status = resp.status();
        let headers = resp.headers().clone();
        if resp.content_length().is_some_and(|l| l > limit as u64) {
            return Err(InventoryError::Decode("response body too large".into()));
        }
        let mut body = Vec::new();
        while let Some(chunk) = resp.chunk().await? {
            if body.len() + chunk.len() > limit {
                return Err(InventoryError::Decode("response body too large".into()));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Self {
            status,
            headers,
            body,
        })
    }

    /// The body as (lossy) text, truncated for use in error messages.
    #[must_use]
    pub fn snippet(&self) -> String {
        truncate(&String::from_utf8_lossy(&self.body), 300)
    }
}

/// Parses `Retry-After` (delta seconds or HTTP date is not supported: seconds only) and falls
/// back to `RateLimit-Reset` / `X-RateLimit-Reset` (either delta seconds or a Unix timestamp).
#[must_use]
pub fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let num = |name: &str| -> Option<u64> {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
    };
    if let Some(s) = num("retry-after") {
        return Some(Duration::from_secs(s));
    }
    let reset = num("ratelimit-reset").or_else(|| num("x-ratelimit-reset"))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // Values larger than ~10 years of seconds are absolute Unix timestamps.
    if reset > 315_360_000 {
        Some(Duration::from_secs(reset.saturating_sub(now)))
    } else {
        Some(Duration::from_secs(reset))
    }
}

/// Default mapping of an unsuccessful status to an error.
#[must_use]
pub fn status_error(status: StatusCode, headers: &HeaderMap, message: String) -> InventoryError {
    match status.as_u16() {
        401 | 403 => InventoryError::AuthError { message },
        429 => InventoryError::RateLimited {
            retry_after: retry_after(headers),
        },
        s => InventoryError::Http { status: s, message },
    }
}

/// Truncates a string to at most `max` characters.
#[must_use]
pub fn truncate(s: &str, max: usize) -> String {
    let t: String = s.chars().take(max).collect();
    if t.len() < s.len() {
        format!("{t}…")
    } else {
        t
    }
}

/// Whether two URLs share scheme, host and port. Pagination links returned by a provider are
/// only followed when they stay on the configured origin, so credentials never leak.
#[must_use]
pub fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

/// Parses a base URL, making sure it ends with `/` so that `join` appends to it.
pub fn base_url(s: &str) -> Result<Url, InventoryError> {
    let mut u = Url::parse(s).map_err(|e| InventoryError::Config(format!("invalid URL: {e}")))?;
    if !matches!(u.scheme(), "https" | "http") {
        return Err(InventoryError::Config("URL must be http(s)".into()));
    }
    if !u.path().ends_with('/') {
        let p = format!("{}/", u.path());
        u.set_path(&p);
    }
    Ok(u)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn retry_after_parsing() {
        let mut h = HeaderMap::new();
        assert_eq!(retry_after(&h), None);
        h.insert("ratelimit-reset", HeaderValue::from_static("30"));
        assert_eq!(retry_after(&h), Some(Duration::from_secs(30)));
        h.insert("retry-after", HeaderValue::from_static("7"));
        assert_eq!(retry_after(&h), Some(Duration::from_secs(7)));
    }

    #[test]
    fn origin_checks() {
        let a = Url::parse("https://api.example.com/v2/x").unwrap();
        assert!(same_origin(
            &a,
            &Url::parse("https://api.example.com:443/v2/y?page=2").unwrap()
        ));
        assert!(!same_origin(
            &a,
            &Url::parse("https://evil.example/v2/x").unwrap()
        ));
        assert!(!same_origin(
            &a,
            &Url::parse("http://api.example.com/v2/x").unwrap()
        ));
        assert_eq!(base_url("http://h:1/a").unwrap().as_str(), "http://h:1/a/");
        assert!(base_url("file:///etc").is_err());
    }

    #[test]
    fn truncation() {
        assert_eq!(truncate("héllo", 2), "hé…");
        assert_eq!(truncate("hi", 5), "hi");
    }
}
