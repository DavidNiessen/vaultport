// SPDX-License-Identifier: GPL-3.0-or-later
//! WebDAV and S3 backends against stateful in-process fake servers, including full engine
//! syncs between two devices.

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::many_single_char_names,
    clippy::format_push_string
)]

mod common;

use base64::Engine as _;
use cloud_inventory::sigv4::{self, Credentials, SigningParams};
use common::{add_host, labels, new_vault, replica};
use percent_encoding::percent_decode_str;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sync::backends::{Addressing, S3Backend, S3Config, WebDavBackend, WebDavCredentials};
use sync::{Precondition, SyncBackend, SyncEngine, SyncError, SyncState};
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

// ----- fake WebDAV server -------------------------------------------------------------------

#[derive(Default)]
struct DavState {
    files: BTreeMap<String, Vec<u8>>,
    dirs: BTreeSet<String>,
    requests: Vec<String>,
}

#[derive(Clone, Default)]
struct FakeDav(Arc<Mutex<DavState>>);

const DAV_ROOT: &str = "/remote.php/dav/files/alice/";

impl Respond for FakeDav {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let expected = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode("alice:secret")
        );
        let auth = req
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok());
        if auth != Some(expected.as_str()) {
            return ResponseTemplate::new(401);
        }
        let Some(rel) = req.url.path().strip_prefix(DAV_ROOT) else {
            return ResponseTemplate::new(404);
        };
        let rel = percent_decode_str(rel).decode_utf8_lossy().into_owned();
        let mut st = self.0.lock().unwrap();
        st.requests.push(format!("{} {rel}", req.method));
        let parent_ok = |st: &DavState, p: &str| {
            let t = p.trim_end_matches('/');
            match t.rsplit_once('/') {
                None => true,
                Some((parent, _)) => st.dirs.contains(&format!("{parent}/")),
            }
        };
        match req.method.as_str() {
            "MKCOL" => {
                if st.dirs.contains(&rel) {
                    ResponseTemplate::new(405)
                } else if !parent_ok(&st, &rel) {
                    ResponseTemplate::new(409)
                } else {
                    st.dirs.insert(rel);
                    ResponseTemplate::new(201)
                }
            }
            "PUT" => {
                if !parent_ok(&st, &rel) {
                    return ResponseTemplate::new(409);
                }
                let create_only = req
                    .headers
                    .get("if-none-match")
                    .is_some_and(|v| v.as_bytes() == b"*");
                if create_only && st.files.contains_key(&rel) {
                    return ResponseTemplate::new(412);
                }
                st.files.insert(rel, req.body.clone());
                ResponseTemplate::new(201).insert_header("etag", "\"e1\"")
            }
            "GET" => match st.files.get(&rel) {
                Some(b) => ResponseTemplate::new(200).set_body_bytes(b.clone()),
                None => ResponseTemplate::new(404),
            },
            "DELETE" => match st.files.remove(&rel) {
                Some(_) => ResponseTemplate::new(204),
                None => ResponseTemplate::new(404),
            },
            "PROPFIND" => {
                if !(rel.is_empty() || st.dirs.contains(&rel)) {
                    return ResponseTemplate::new(404);
                }
                let depth_one = |name: &str| {
                    name.strip_prefix(rel.as_str()).is_some_and(|rest| {
                        let rest = rest.trim_end_matches('/');
                        !rest.is_empty() && !rest.contains('/')
                    })
                };
                let mut xml = String::from(
                    r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:" xmlns:oc="http://owncloud.org/ns">"#,
                );
                xml.push_str(&format!(
                    "<d:response><d:href>{DAV_ROOT}{rel}</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"
                ));
                for d in st.dirs.iter().filter(|d| depth_one(d)) {
                    xml.push_str(&format!(
                        "<d:response><d:href>{DAV_ROOT}{d}</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat></d:response>"
                    ));
                }
                for (f, b) in st.files.iter().filter(|(f, _)| depth_one(f)) {
                    // Percent-encode one character to exercise href decoding.
                    let href = f.replace('-', "%2D");
                    xml.push_str(&format!(
                        "<d:response><d:href>{DAV_ROOT}{href}</d:href><d:propstat><d:prop><d:resourcetype/><d:getcontentlength>{}</d:getcontentlength><d:getetag>&quot;x&quot;</d:getetag></d:prop></d:propstat><d:propstat><d:prop><oc:missing/></d:prop><d:status>HTTP/1.1 404 Not Found</d:status></d:propstat></d:response>",
                        b.len()
                    ));
                }
                xml.push_str("</d:multistatus>");
                ResponseTemplate::new(207)
                    .insert_header("content-type", "application/xml; charset=utf-8")
                    .set_body_string(xml)
            }
            _ => ResponseTemplate::new(405),
        }
    }
}

async fn dav_server() -> (MockServer, FakeDav) {
    let server = MockServer::start().await;
    let fake = FakeDav::default();
    Mock::given(any())
        .respond_with(fake.clone())
        .mount(&server)
        .await;
    (server, fake)
}

fn dav_backend(server: &MockServer, password: &str) -> WebDavBackend {
    WebDavBackend::new(
        &format!("{}{DAV_ROOT}", server.uri()),
        Some(WebDavCredentials {
            username: "alice".into(),
            password: password.into(),
        }),
    )
    .unwrap()
}

#[tokio::test]
async fn webdav_backend_operations() {
    let (server, fake) = dav_server().await;
    let b = dav_backend(&server, "secret");
    assert!(b.list("v/").await.unwrap().is_empty());
    b.put("v/s/a-1.seg", b"one", Some(Precondition::IfNoneMatch))
        .await
        .unwrap();
    assert!(matches!(
        b.put("v/s/a-1.seg", b"two", Some(Precondition::IfNoneMatch))
            .await,
        Err(SyncError::PreconditionFailed(_))
    ));
    b.put("v/s/b.seg", b"three", None).await.unwrap();
    b.put("v/t.seg", b"4", None).await.unwrap();
    let list = b.list("v/").await.unwrap();
    let names: Vec<&str> = list.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, vec!["v/s/a-1.seg", "v/s/b.seg", "v/t.seg"]);
    assert_eq!(list[0].size, 3);
    assert_eq!(list[0].etag.as_deref(), Some("\"x\""));
    assert_eq!(b.list("v/s/b").await.unwrap().len(), 1);
    assert_eq!(b.get("v/s/a-1.seg").await.unwrap(), b"one");
    assert!(matches!(b.get("v/nope").await, Err(SyncError::NotFound(_))));
    b.delete("v/s/b.seg").await.unwrap();
    b.delete("v/s/b.seg").await.unwrap();
    assert_eq!(b.list("v/").await.unwrap().len(), 2);
    // Parent collections were created once and then remembered.
    let mkcols = fake
        .0
        .lock()
        .unwrap()
        .requests
        .iter()
        .filter(|r| r.starts_with("MKCOL"))
        .count();
    assert_eq!(mkcols, 2);

    let bad = dav_backend(&server, "wrong");
    assert!(matches!(bad.list("v/").await, Err(SyncError::Auth(_))));
    assert!(matches!(
        bad.put("v/x", b"", None).await,
        Err(SyncError::Auth(_))
    ));
}

#[tokio::test]
async fn two_devices_sync_over_webdav() {
    let (server, _fake) = dav_server().await;
    let backend = dav_backend(&server, "secret");
    let tmp = tempfile::tempdir().unwrap();
    let mut a = new_vault(tmp.path(), "a.db");
    let mut b = replica(&a, tmp.path(), "b.db");
    let (mut sa, mut sb) = (SyncState::new(a.vault_id()), SyncState::new(b.vault_id()));
    let engine = SyncEngine::default();
    add_host(&mut a, "from-a");
    add_host(&mut b, "from-b");
    for _ in 0..2 {
        assert!(
            engine
                .sync(&mut a, &backend, &mut sa)
                .await
                .unwrap()
                .is_clean()
        );
        assert!(
            engine
                .sync(&mut b, &backend, &mut sb)
                .await
                .unwrap()
                .is_clean()
        );
    }
    assert_eq!(labels(&a), vec!["from-a", "from-b"]);
    assert_eq!(labels(&b), labels(&a));
}

// ----- fake S3 server -----------------------------------------------------------------------

const BUCKET: &str = "sync-bucket";

#[derive(Default)]
struct S3State {
    objects: BTreeMap<String, Vec<u8>>,
    list_calls: usize,
}

#[derive(Clone, Default)]
struct FakeS3(Arc<Mutex<S3State>>);

fn s3_error(status: u16, code: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_string(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Error><Code>{code}</Code><Message>m</Message></Error>"
    ))
}

/// Parses `YYYYMMDDTHHMMSSZ`.
fn parse_amz_date(s: &str) -> SystemTime {
    let n = |r: std::ops::Range<usize>| s[r].parse::<i64>().unwrap();
    let (y, m, d) = (n(0..4), n(4..6), n(6..8));
    // Days from civil (Howard Hinnant).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + n(9..11) * 3600 + n(11..13) * 60 + n(13..15);
    UNIX_EPOCH + Duration::from_secs(secs as u64)
}

/// Recomputes the SigV4 signature of a received request.
fn verify_signature(req: &Request, creds: &Credentials) -> bool {
    let header = |n: &str| {
        req.headers
            .get(n)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    let (Some(auth), Some(date), Some(sha)) = (
        header("authorization"),
        header("x-amz-date"),
        header("x-amz-content-sha256"),
    ) else {
        return false;
    };
    if sha != sigv4::sha256_hex(&req.body) {
        return false;
    }
    let Some(signed) = auth
        .split("SignedHeaders=")
        .nth(1)
        .and_then(|s| s.split(',').next())
    else {
        return false;
    };
    let mut headers = Vec::new();
    for h in signed.split(';') {
        if matches!(h, "host" | "x-amz-date" | "x-amz-content-sha256") {
            continue;
        }
        headers.push((h.to_owned(), header(h).unwrap_or_default()));
    }
    // Rebuild the URL the client signed from the Host header it sent.
    let host = header("host").unwrap();
    let mut url = req.url.clone();
    let (h, p) = host.split_once(':').unwrap();
    url.set_host(Some(h)).unwrap();
    url.set_port(Some(p.parse().unwrap())).unwrap();
    let params = SigningParams::s3(creds, "us-east-1").at(parse_amz_date(&date));
    let added = sigv4::sign(req.method.as_str(), &url, &headers, &sha, &params).unwrap();
    added
        .iter()
        .any(|(k, v)| k == "authorization" && *v == auth)
}

impl Respond for FakeS3 {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        if !verify_signature(req, &Credentials::new("AKIDTEST", "s3-secret")) {
            return s3_error(403, "SignatureDoesNotMatch");
        }
        let mut st = self.0.lock().unwrap();
        let path = req.url.path().to_owned();
        if path == format!("/{BUCKET}") {
            // ListObjectsV2 with a page size of 2.
            let q: BTreeMap<String, String> = req.url.query_pairs().into_owned().collect();
            if q.get("list-type").map(String::as_str) != Some("2") {
                return s3_error(400, "InvalidArgument");
            }
            st.list_calls += 1;
            let prefix = q.get("prefix").cloned().unwrap_or_default();
            let start: usize = q
                .get("continuation-token")
                .map_or(0, |t| t.trim_start_matches("tok/").parse().unwrap());
            let keys: Vec<(&String, &Vec<u8>)> = st
                .objects
                .iter()
                .filter(|(k, _)| k.starts_with(&prefix))
                .collect();
            let page = &keys[start.min(keys.len())..(start + 2).min(keys.len())];
            let truncated = start + 2 < keys.len();
            let mut xml = String::from(
                r#"<?xml version="1.0" encoding="UTF-8"?><ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">"#,
            );
            xml.push_str(&format!(
                "<Name>{BUCKET}</Name><KeyCount>{}</KeyCount>",
                page.len()
            ));
            for (k, v) in page {
                xml.push_str(&format!(
                    "<Contents><Key>{k}</Key><Size>{}</Size><ETag>&quot;e&quot;</ETag><StorageClass>STANDARD</StorageClass></Contents>",
                    v.len()
                ));
            }
            xml.push_str(&format!("<IsTruncated>{truncated}</IsTruncated>"));
            if truncated {
                xml.push_str(&format!(
                    "<NextContinuationToken>tok/{}</NextContinuationToken>",
                    start + 2
                ));
            }
            xml.push_str("</ListBucketResult>");
            return ResponseTemplate::new(200).set_body_string(xml);
        }
        let Some(key) = path.strip_prefix(&format!("/{BUCKET}/")) else {
            return s3_error(404, "NoSuchBucket");
        };
        let key = key.to_owned();
        match req.method.as_str() {
            "PUT" => {
                let create_only = req
                    .headers
                    .get("if-none-match")
                    .is_some_and(|v| v.as_bytes() == b"*");
                if create_only && st.objects.contains_key(&key) {
                    return s3_error(412, "PreconditionFailed");
                }
                st.objects.insert(key, req.body.clone());
                ResponseTemplate::new(200).insert_header("etag", "\"abc\"")
            }
            "GET" => match st.objects.get(&key) {
                Some(b) => ResponseTemplate::new(200).set_body_bytes(b.clone()),
                None => s3_error(404, "NoSuchKey"),
            },
            "DELETE" => {
                st.objects.remove(&key);
                ResponseTemplate::new(204)
            }
            _ => s3_error(405, "MethodNotAllowed"),
        }
    }
}

async fn s3_server() -> (MockServer, FakeS3) {
    let server = MockServer::start().await;
    let fake = FakeS3::default();
    Mock::given(any())
        .respond_with(fake.clone())
        .mount(&server)
        .await;
    (server, fake)
}

fn s3_backend(server: &MockServer, secret: &str) -> S3Backend {
    S3Backend::new(
        S3Config {
            endpoint: server.uri(),
            region: "us-east-1".into(),
            bucket: BUCKET.into(),
            prefix: "vaultport/".into(),
            addressing: Addressing::Path,
            conditional_writes: true,
        },
        Credentials::new("AKIDTEST", secret),
    )
    .unwrap()
}

#[tokio::test]
async fn s3_backend_operations() {
    let (server, fake) = s3_server().await;
    let b = s3_backend(&server, "s3-secret");
    for i in 0..5 {
        b.put(
            &format!("v/s/{i}.seg"),
            b"data",
            Some(Precondition::IfNoneMatch),
        )
        .await
        .unwrap();
    }
    assert!(matches!(
        b.put("v/s/0.seg", b"x", Some(Precondition::IfNoneMatch))
            .await,
        Err(SyncError::PreconditionFailed(_))
    ));
    assert!(
        fake.0
            .lock()
            .unwrap()
            .objects
            .contains_key("vaultport/v/s/0.seg")
    );
    let list = b.list("v/").await.unwrap();
    assert_eq!(list.len(), 5);
    assert_eq!(list[0].name, "v/s/0.seg");
    assert_eq!(list[0].size, 4);
    assert_eq!(
        fake.0.lock().unwrap().list_calls,
        3,
        "paginated by continuation token"
    );
    assert_eq!(b.get("v/s/3.seg").await.unwrap(), b"data");
    assert!(matches!(
        b.get("v/s/9.seg").await,
        Err(SyncError::NotFound(_))
    ));
    b.delete("v/s/3.seg").await.unwrap();
    assert_eq!(b.list("v/s/").await.unwrap().len(), 4);

    let bad = s3_backend(&server, "wrong-secret");
    assert!(
        matches!(bad.list("v/").await, Err(SyncError::Auth(c)) if c == "SignatureDoesNotMatch")
    );
}

#[tokio::test]
async fn s3_throttling_is_mapped() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(s3_error(503, "SlowDown").insert_header("retry-after", "4"))
        .mount(&server)
        .await;
    let b = s3_backend(&server, "s3-secret");
    match b.get("v/x").await {
        Err(SyncError::RateLimited { retry_after }) => {
            assert_eq!(retry_after, Some(Duration::from_secs(4)));
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn two_devices_sync_and_compact_over_s3() {
    let (server, fake) = s3_server().await;
    let backend = s3_backend(&server, "s3-secret");
    let tmp = tempfile::tempdir().unwrap();
    let mut a = new_vault(tmp.path(), "a.db");
    let mut b = replica(&a, tmp.path(), "b.db");
    let (mut sa, mut sb) = (SyncState::new(a.vault_id()), SyncState::new(b.vault_id()));
    let engine = SyncEngine::default();
    for i in 0..3 {
        add_host(&mut a, &format!("a{i}"));
        assert!(
            engine
                .sync(&mut a, &backend, &mut sa)
                .await
                .unwrap()
                .is_clean()
        );
        add_host(&mut b, &format!("b{i}"));
        assert!(
            engine
                .sync(&mut b, &backend, &mut sb)
                .await
                .unwrap()
                .is_clean()
        );
    }
    engine.sync(&mut a, &backend, &mut sa).await.unwrap();
    assert_eq!(labels(&a), labels(&b));
    assert_eq!(fake.0.lock().unwrap().objects.len(), 6);

    let r = engine
        .compact(
            &mut a,
            &backend,
            &mut sa,
            sync::CompactOptions {
                prune_foreign: true,
            },
        )
        .await
        .unwrap();
    assert!(r.snapshot.is_some());
    assert_eq!(fake.0.lock().unwrap().objects.len(), 1);
    let mut c = replica(&a, tmp.path(), "c.db");
    let mut sc = SyncState::new(c.vault_id());
    assert!(
        engine
            .sync(&mut c, &backend, &mut sc)
            .await
            .unwrap()
            .is_clean()
    );
    assert_eq!(labels(&c).len(), 6);
    assert!(
        engine
            .sync(&mut b, &backend, &mut sb)
            .await
            .unwrap()
            .is_clean()
    );
}
