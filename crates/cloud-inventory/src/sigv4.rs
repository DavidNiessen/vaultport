// SPDX-License-Identifier: GPL-3.0-or-later
//! A small, dependency-light implementation of AWS Signature Version 4.
//!
//! This is shared by the AWS inventory clients and by the S3-compatible sync backend (Amazon
//! S3, Cloudflare R2, MinIO, Backblaze B2, ...). Only header-based signing is implemented
//! (no presigned URLs, no chunked/streaming payloads).
//!
//! The implementation is verified against vectors from the official AWS SigV4 test suite and
//! the examples in the AWS documentation (see the tests at the bottom of this file).

use hmac::{Hmac, Mac};
use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

type HmacSha256 = Hmac<Sha256>;

/// SHA-256 of the empty string, hex encoded (the payload hash of body-less requests).
pub const EMPTY_PAYLOAD_SHA256: &str =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// AWS-style access credentials.
///
/// The secret parts are wiped from memory on drop and never printed by `Debug`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
    /// Access key id (`AKIA...`). Not secret, but still not logged.
    pub access_key_id: String,
    /// Secret access key.
    pub secret_access_key: String,
    /// Optional session token for temporary (STS) credentials.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
}

impl Credentials {
    /// Creates long-term credentials.
    #[must_use]
    pub fn new(access_key_id: impl Into<String>, secret_access_key: impl Into<String>) -> Self {
        Self {
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            session_token: None,
        }
    }

    /// Adds an STS session token.
    #[must_use]
    pub fn with_session_token(mut self, token: impl Into<String>) -> Self {
        self.session_token = Some(token.into());
        self
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let id = &self.access_key_id;
        let shown = if id.len() > 4 && id.is_char_boundary(4) {
            format!("{}…", &id[..4])
        } else {
            "…".to_owned()
        };
        f.debug_struct("Credentials")
            .field("access_key_id", &shown)
            .field("secret_access_key", &"<redacted>")
            .field(
                "session_token",
                &self.session_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// Errors produced while signing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SignError {
    #[error("the request URL has no host")]
    MissingHost,
    #[error("the system clock is before the Unix epoch")]
    Clock,
    #[error("invalid header {0:?}")]
    InvalidHeader(String),
}

/// Parameters of one signature.
#[derive(Debug, Clone, Copy)]
pub struct SigningParams<'a> {
    pub credentials: &'a Credentials,
    pub region: &'a str,
    pub service: &'a str,
    pub time: SystemTime,
    /// Encode the path twice (every service except S3). S3 uses the request path as-is.
    pub double_encode_path: bool,
    /// Add an `x-amz-content-sha256` header (required by S3).
    pub content_sha256_header: bool,
}

impl<'a> SigningParams<'a> {
    /// Parameters for a regular AWS service (`ec2`, `lightsail`, ...).
    #[must_use]
    pub fn service(credentials: &'a Credentials, region: &'a str, service: &'a str) -> Self {
        Self {
            credentials,
            region,
            service,
            time: SystemTime::now(),
            double_encode_path: true,
            content_sha256_header: false,
        }
    }

    /// Parameters for S3 and S3-compatible storage.
    #[must_use]
    pub fn s3(credentials: &'a Credentials, region: &'a str) -> Self {
        Self {
            credentials,
            region,
            service: "s3",
            time: SystemTime::now(),
            double_encode_path: false,
            content_sha256_header: true,
        }
    }

    /// Overrides the signing time (used by tests).
    #[must_use]
    pub fn at(mut self, time: SystemTime) -> Self {
        self.time = time;
        self
    }
}

/// Hex-encoded SHA-256 of a payload.
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// URI-encodes `input` the way SigV4 requires: every byte except the unreserved characters
/// `A-Z a-z 0-9 - _ . ~` is percent-encoded (upper-case hex). When `keep_slash` is set, `/`
/// is left alone (for object keys in paths).
#[must_use]
pub fn uri_encode(input: &str, keep_slash: bool) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        let unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~');
        if unreserved || (keep_slash && b == b'/') {
            out.push(char::from(b));
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0xf)]));
        }
    }
    out
}

/// Builds a query string from pairs, encoding keys and values with [`uri_encode`].
#[must_use]
pub fn encode_query(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", uri_encode(k, false), uri_encode(v, false)))
        .collect::<Vec<_>>()
        .join("&")
}

fn hmac(key: &[u8], data: &[u8]) -> Zeroizing<Vec<u8>> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(data);
    Zeroizing::new(mac.finalize().into_bytes().to_vec())
}

/// Formats a time as `(YYYYMMDD, YYYYMMDDTHHMMSSZ)`.
fn format_time(time: SystemTime) -> Result<(String, String), SignError> {
    let secs = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| SignError::Clock)?
        .as_secs();
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    let date = format!("{y:04}{m:02}{d:02}");
    let stamp = format!(
        "{date}T{:02}{:02}{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    );
    Ok((date, stamp))
}

/// Converts days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn canonical_uri(url: &Url, double_encode: bool) -> String {
    let path = url.path();
    let path = if path.is_empty() { "/" } else { path };
    if double_encode {
        path.split('/')
            .map(|seg| uri_encode(seg, false))
            .collect::<Vec<_>>()
            .join("/")
    } else {
        path.to_owned()
    }
}

fn canonical_query(url: &Url) -> String {
    let Some(query) = url.query() else {
        return String::new();
    };
    let mut pairs: Vec<(String, String)> = query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            let k = percent_decode_str(k).decode_utf8_lossy();
            let v = percent_decode_str(v).decode_utf8_lossy();
            (uri_encode(&k, false), uri_encode(&v, false))
        })
        .collect();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn normalize_value(v: &str) -> String {
    v.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The host header value derived from a URL (`host[:port]`).
pub fn host_header(url: &Url) -> Result<String, SignError> {
    let host = url.host_str().ok_or(SignError::MissingHost)?;
    Ok(match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host.to_owned(),
    })
}

/// Signs a request.
///
/// `headers` are the headers the caller will send (they are all signed); the `host` header is
/// derived from `url` and must not be included. Returns the headers to *add* to the request:
/// `x-amz-date`, optionally `x-amz-content-sha256` and `x-amz-security-token`, and
/// `authorization`.
pub fn sign(
    method: &str,
    url: &Url,
    headers: &[(String, String)],
    payload_sha256: &str,
    params: &SigningParams<'_>,
) -> Result<Vec<(String, String)>, SignError> {
    let (date, stamp) = format_time(params.time)?;
    let mut added = vec![("x-amz-date".to_owned(), stamp.clone())];
    if params.content_sha256_header {
        added.push(("x-amz-content-sha256".to_owned(), payload_sha256.to_owned()));
    }
    if let Some(token) = &params.credentials.session_token {
        added.push(("x-amz-security-token".to_owned(), token.clone()));
    }

    let mut all: Vec<(String, String)> = vec![("host".to_owned(), host_header(url)?)];
    for (k, v) in headers.iter().chain(added.iter()) {
        let name = k.trim().to_ascii_lowercase();
        if name.is_empty() || name == "host" || name == "authorization" {
            return Err(SignError::InvalidHeader(name));
        }
        all.push((name, normalize_value(v)));
    }
    all.sort_by(|a, b| a.0.cmp(&b.0));
    // Merge duplicate header names (comma-separated, in order).
    let mut merged: Vec<(String, String)> = Vec::with_capacity(all.len());
    for (k, v) in all {
        match merged.last_mut() {
            Some((lk, lv)) if *lk == k => {
                lv.push(',');
                lv.push_str(&v);
            }
            _ => merged.push((k, v)),
        }
    }
    let mut canonical_headers = String::new();
    for (k, v) in &merged {
        canonical_headers.push_str(k);
        canonical_headers.push(':');
        canonical_headers.push_str(v);
        canonical_headers.push('\n');
    }
    let signed_headers = merged
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");

    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method.to_ascii_uppercase(),
        canonical_uri(url, params.double_encode_path),
        canonical_query(url),
        canonical_headers,
        signed_headers,
        payload_sha256
    );
    let scope = format!("{date}/{}/{}/aws4_request", params.region, params.service);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{stamp}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );

    let mut secret = Zeroizing::new(String::from("AWS4"));
    secret.push_str(&params.credentials.secret_access_key);
    let k_date = hmac(secret.as_bytes(), date.as_bytes());
    let k_region = hmac(&k_date, params.region.as_bytes());
    let k_service = hmac(&k_region, params.service.as_bytes());
    let k_signing = hmac(&k_service, b"aws4_request");
    let signature = hex::encode(&*hmac(&k_signing, string_to_sign.as_bytes()));

    added.push((
        "authorization".to_owned(),
        format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            params.credentials.access_key_id
        ),
    ));
    Ok(added)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::time::Duration;

    /// 2015-08-30T12:36:00Z, the date used by the AWS SigV4 test suite.
    fn suite_time() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_440_938_160)
    }

    fn suite_creds() -> Credentials {
        Credentials::new("AKIDEXAMPLE", "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY")
    }

    fn auth(added: &[(String, String)]) -> &str {
        &added.iter().find(|(k, _)| k == "authorization").unwrap().1
    }

    fn suite(method: &str, url: &str, headers: &[(&str, &str)], body: &[u8]) -> String {
        let creds = suite_creds();
        let params = SigningParams::service(&creds, "us-east-1", "service").at(suite_time());
        let headers: Vec<(String, String)> = headers
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        let added = sign(
            method,
            &Url::parse(url).unwrap(),
            &headers,
            &sha256_hex(body),
            &params,
        )
        .unwrap();
        auth(&added).to_owned()
    }

    #[test]
    fn time_formatting() {
        assert_eq!(
            format_time(suite_time()).unwrap(),
            ("20150830".to_owned(), "20150830T123600Z".to_owned())
        );
        // 2000-02-29 (leap day) 00:00:00
        let t = UNIX_EPOCH + Duration::from_secs(951_782_400);
        assert_eq!(format_time(t).unwrap().0, "20000229");
    }

    #[test]
    fn get_vanilla() {
        assert_eq!(
            suite("GET", "https://example.amazonaws.com/", &[], b""),
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
             SignedHeaders=host;x-amz-date, \
             Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
        );
    }

    #[test]
    fn get_vanilla_query_order_key_case() {
        let a = suite(
            "GET",
            "https://example.amazonaws.com/?Param2=value2&Param1=value1",
            &[],
            b"",
        );
        assert!(
            a.ends_with("b97d918cfa904a5beff61c982a1b6f458b799221646efd99d3219ec94cdf2500"),
            "{a}"
        );
    }

    #[test]
    fn post_vanilla() {
        let a = suite("POST", "https://example.amazonaws.com/", &[], b"");
        assert!(
            a.ends_with("5da7c1a2acd57cee7505fc6676e4e544621c30862966e37dddb68e92efbe5d6b"),
            "{a}"
        );
    }

    #[test]
    fn iam_documentation_example() {
        // https://docs.aws.amazon.com/IAM/latest/UserGuide/create-signed-request.html (2015 ed.)
        let creds = suite_creds();
        let params = SigningParams::service(&creds, "us-east-1", "iam").at(suite_time());
        let added = sign(
            "GET",
            &Url::parse("https://iam.amazonaws.com/?Action=ListUsers&Version=2010-05-08").unwrap(),
            &[(
                "Content-Type".into(),
                "application/x-www-form-urlencoded; charset=utf-8".into(),
            )],
            EMPTY_PAYLOAD_SHA256,
            &params,
        )
        .unwrap();
        assert_eq!(
            auth(&added),
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/iam/aws4_request, \
             SignedHeaders=content-type;host;x-amz-date, \
             Signature=5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7"
        );
    }

    #[test]
    fn s3_documentation_example_get_object() {
        // https://docs.aws.amazon.com/AmazonS3/latest/API/sig-v4-header-based-auth.html
        let creds = Credentials::new(
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        );
        // 2013-05-24T00:00:00Z
        let t = UNIX_EPOCH + Duration::from_secs(1_369_353_600);
        let params = SigningParams::s3(&creds, "us-east-1").at(t);
        let added = sign(
            "GET",
            &Url::parse("https://examplebucket.s3.amazonaws.com/test.txt").unwrap(),
            &[("Range".into(), "bytes=0-9".into())],
            EMPTY_PAYLOAD_SHA256,
            &params,
        )
        .unwrap();
        assert_eq!(
            auth(&added),
            "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, \
             SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, \
             Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }

    #[test]
    fn session_token_is_signed_and_debug_is_redacted() {
        let creds = suite_creds().with_session_token("TOKEN");
        let params = SigningParams::service(&creds, "us-east-1", "service").at(suite_time());
        let added = sign(
            "GET",
            &Url::parse("https://example.amazonaws.com/").unwrap(),
            &[],
            EMPTY_PAYLOAD_SHA256,
            &params,
        )
        .unwrap();
        assert!(auth(&added).contains("SignedHeaders=host;x-amz-date;x-amz-security-token"));
        let dbg = format!("{creds:?}");
        assert!(!dbg.contains("wJalr") && !dbg.contains("TOKEN"), "{dbg}");
    }

    #[test]
    fn encoding_rules() {
        assert_eq!(uri_encode("a b/c~d*", false), "a%20b%2Fc~d%2A");
        assert_eq!(uri_encode("a b/c", true), "a%20b/c");
        assert_eq!(
            canonical_query(&Url::parse("https://x/?b=2&a=%2F&a=1&c").unwrap()),
            "a=%2F&a=1&b=2&c="
        );
    }
}
