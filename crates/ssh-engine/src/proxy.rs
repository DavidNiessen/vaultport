// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal HTTP `CONNECT` (RFC 9110 §9.3.6, Basic auth) and SOCKS5 (RFC 1928 / RFC 1929)
//! clients used to reach the first hop.
//!
//! Host names are always sent to the proxy unresolved (SOCKS5 `DOMAINNAME` address type), so
//! DNS resolution happens on the proxy side and does not leak locally.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use std::fmt;
use std::net::IpAddr;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use zeroize::Zeroizing;

/// Credentials for a proxy.
#[derive(Clone)]
pub struct ProxyAuth {
    pub username: String,
    pub password: Zeroizing<String>,
}

impl fmt::Debug for ProxyAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProxyAuth")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// Proxy protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyKind {
    /// HTTP `CONNECT` tunnel.
    Http,
    /// SOCKS version 5.
    Socks5,
}

/// A proxy used to reach the first hop.
#[derive(Debug, Clone)]
pub struct Proxy {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub auth: Option<ProxyAuth>,
}

impl Proxy {
    /// Builds a proxy from the model's [`core_model::ProxyConfig`]; the password (stored in
    /// the vault) must be supplied by the caller.
    #[must_use]
    pub fn from_config(cfg: &core_model::ProxyConfig, password: Option<Zeroizing<String>>) -> Self {
        let kind = match cfg.kind {
            core_model::ProxyKind::Http => ProxyKind::Http,
            core_model::ProxyKind::Socks5 => ProxyKind::Socks5,
        };
        let auth = cfg.username.as_ref().map(|u| ProxyAuth {
            username: u.clone(),
            password: password.unwrap_or_default(),
        });
        Self {
            kind,
            host: cfg.host.clone(),
            port: cfg.port,
            auth,
        }
    }
}

/// Errors of the proxy clients.
#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("i/o error talking to proxy: {0}")]
    Io(#[from] std::io::Error),
    #[error("proxy refused the request: {0}")]
    Refused(String),
    #[error("proxy requires authentication")]
    AuthRequired,
    #[error("proxy authentication failed")]
    AuthFailed,
    #[error("malformed proxy response: {0}")]
    Protocol(String),
}

/// Connects to `proxy` and asks it to tunnel to `host:port`.
pub async fn connect(proxy: &Proxy, host: &str, port: u16) -> Result<TcpStream, ProxyError> {
    let mut stream = TcpStream::connect((proxy.host.as_str(), proxy.port)).await?;
    let _ = stream.set_nodelay(true);
    match proxy.kind {
        ProxyKind::Http => http_connect(&mut stream, host, port, proxy.auth.as_ref()).await?,
        ProxyKind::Socks5 => socks5_connect(&mut stream, host, port, proxy.auth.as_ref()).await?,
    }
    Ok(stream)
}

fn authority(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn reject_controls(s: &str) -> Result<(), ProxyError> {
    if s.chars().any(char::is_control) {
        Err(ProxyError::Protocol(
            "control characters in host name".into(),
        ))
    } else {
        Ok(())
    }
}

const MAX_HEADER: usize = 16 * 1024;

/// Performs an HTTP `CONNECT` handshake on an established stream.
///
/// Reads the response byte by byte so that no byte of the tunnelled protocol (the SSH banner may
/// arrive in the same TCP segment as the `200`) is consumed.
pub async fn http_connect<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    host: &str,
    port: u16,
    auth: Option<&ProxyAuth>,
) -> Result<(), ProxyError> {
    reject_controls(host)?;
    let target = authority(host, port);
    let mut req = Zeroizing::new(format!(
        "CONNECT {target} HTTP/1.1\r\nHost: {target}\r\nUser-Agent: ssh-client\r\n"
    ));
    if let Some(a) = auth {
        let creds = Zeroizing::new(format!("{}:{}", a.username, a.password.as_str()));
        let encoded = Zeroizing::new(B64.encode(creds.as_bytes()));
        req.push_str("Proxy-Authorization: Basic ");
        req.push_str(&encoded);
        req.push_str("\r\n");
    }
    req.push_str("\r\n");
    stream.write_all(req.as_bytes()).await?;
    stream.flush().await?;

    let mut buf = Vec::with_capacity(256);
    let mut byte = [0u8; 1];
    while !buf.ends_with(b"\r\n\r\n") {
        if buf.len() >= MAX_HEADER {
            return Err(ProxyError::Protocol("response header too large".into()));
        }
        let n = stream.read(&mut byte).await?;
        if n == 0 {
            return Err(ProxyError::Protocol("proxy closed the connection".into()));
        }
        buf.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&buf);
    let status_line = head.lines().next().unwrap_or_default();
    let mut parts = status_line.splitn(3, ' ');
    let version = parts.next().unwrap_or_default();
    if !version.starts_with("HTTP/1.") {
        return Err(ProxyError::Protocol(format!(
            "unexpected status line {status_line:?}"
        )));
    }
    let code: u16 = parts
        .next()
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| ProxyError::Protocol(format!("unexpected status line {status_line:?}")))?;
    match code {
        200..=299 => Ok(()),
        407 if auth.is_some() => Err(ProxyError::AuthFailed),
        407 => Err(ProxyError::AuthRequired),
        _ => Err(ProxyError::Refused(status_line.to_owned())),
    }
}

/// Performs a SOCKS5 `CONNECT` handshake on an established stream.
pub async fn socks5_connect<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    host: &str,
    port: u16,
    auth: Option<&ProxyAuth>,
) -> Result<(), ProxyError> {
    reject_controls(host)?;
    // Greeting: offer "no auth" and, with credentials, "username/password".
    if auth.is_some() {
        stream.write_all(&[5, 2, 0x00, 0x02]).await?;
    } else {
        stream.write_all(&[5, 1, 0x00]).await?;
    }
    let mut choice = [0u8; 2];
    stream.read_exact(&mut choice).await?;
    if choice[0] != 5 {
        return Err(ProxyError::Protocol("not a SOCKS5 server".into()));
    }
    match (choice[1], auth) {
        (0x00, _) => {}
        (0x02, Some(a)) => {
            let user = a.username.as_bytes();
            let pass = a.password.as_bytes();
            let (Ok(ulen), Ok(plen)) = (u8::try_from(user.len()), u8::try_from(pass.len())) else {
                return Err(ProxyError::Protocol(
                    "credentials longer than 255 bytes".into(),
                ));
            };
            let mut msg = Zeroizing::new(Vec::with_capacity(3 + user.len() + pass.len()));
            msg.push(1);
            msg.push(ulen);
            msg.extend_from_slice(user);
            msg.push(plen);
            msg.extend_from_slice(pass);
            stream.write_all(&msg).await?;
            let mut status = [0u8; 2];
            stream.read_exact(&mut status).await?;
            if status[1] != 0 {
                return Err(ProxyError::AuthFailed);
            }
        }
        (0x02, None) | (0xff, _) => {
            return Err(if auth.is_some() {
                ProxyError::AuthFailed
            } else {
                ProxyError::AuthRequired
            });
        }
        (m, _) => return Err(ProxyError::Protocol(format!("unsupported method {m:#x}"))),
    }

    let mut req = vec![5, 1, 0];
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    match bare.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => {
            req.push(1);
            req.extend_from_slice(&v4.octets());
        }
        Ok(IpAddr::V6(v6)) => {
            req.push(4);
            req.extend_from_slice(&v6.octets());
        }
        Err(_) => {
            let name = host.as_bytes();
            let len = u8::try_from(name.len())
                .map_err(|_| ProxyError::Protocol("host name longer than 255 bytes".into()))?;
            req.push(3);
            req.push(len);
            req.extend_from_slice(name);
        }
    }
    req.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&req).await?;

    let mut head = [0u8; 4];
    stream.read_exact(&mut head).await?;
    if head[0] != 5 {
        return Err(ProxyError::Protocol("bad reply version".into()));
    }
    if head[1] != 0 {
        return Err(ProxyError::Refused(socks5_reply_text(head[1]).to_owned()));
    }
    let skip = match head[3] {
        1 => 4 + 2,
        4 => 16 + 2,
        3 => {
            let mut l = [0u8; 1];
            stream.read_exact(&mut l).await?;
            usize::from(l[0]) + 2
        }
        t => return Err(ProxyError::Protocol(format!("bad address type {t}"))),
    };
    let mut rest = vec![0u8; skip];
    stream.read_exact(&mut rest).await?;
    Ok(())
}

/// Text for a SOCKS5 reply code.
#[must_use]
pub fn socks5_reply_text(code: u8) -> &'static str {
    match code {
        1 => "general SOCKS server failure",
        2 => "connection not allowed by ruleset",
        3 => "network unreachable",
        4 => "host unreachable",
        5 => "connection refused",
        6 => "TTL expired",
        7 => "command not supported",
        8 => "address type not supported",
        _ => "unknown SOCKS error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    async fn echo_server() -> std::net::SocketAddr {
        let l = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = l.local_addr().expect("addr");
        tokio::spawn(async move {
            while let Ok((mut s, _)) = l.accept().await {
                tokio::spawn(async move {
                    let (mut r, mut w) = s.split();
                    let _ = tokio::io::copy(&mut r, &mut w).await;
                });
            }
        });
        addr
    }

    /// Fake HTTP proxy that checks the request, answers `200` immediately followed by a fake
    /// banner in the same write, then relays to `upstream`.
    async fn fake_http_proxy(
        upstream: std::net::SocketAddr,
        expect_auth: Option<&'static str>,
    ) -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = l.local_addr().expect("addr").port();
        tokio::spawn(async move {
            let (mut s, _) = l.accept().await.expect("accept");
            let mut buf = Vec::new();
            let mut b = [0u8; 1];
            while !buf.ends_with(b"\r\n\r\n") {
                s.read_exact(&mut b).await.expect("read");
                buf.push(b[0]);
            }
            let req = String::from_utf8(buf).expect("utf8");
            assert!(req.starts_with("CONNECT target.example:2222 HTTP/1.1\r\n"));
            if let Some(a) = expect_auth
                && !req.contains(&format!("Proxy-Authorization: Basic {a}\r\n"))
            {
                s.write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n")
                    .await
                    .expect("write");
                return;
            }
            s.write_all(
                b"HTTP/1.1 200 Connection established\r\nVia: fake\r\n\r\nSSH-2.0-fake\r\n",
            )
            .await
            .expect("write");
            let mut up = TcpStream::connect(upstream).await.expect("upstream");
            let _ = tokio::io::copy_bidirectional(&mut s, &mut up).await;
        });
        port
    }

    #[tokio::test]
    async fn http_connect_with_basic_auth() {
        let echo = echo_server().await;
        // base64("user:pa:ss")
        let port = fake_http_proxy(echo, Some("dXNlcjpwYTpzcw==")).await;
        let proxy = Proxy {
            kind: ProxyKind::Http,
            host: "127.0.0.1".into(),
            port,
            auth: Some(ProxyAuth {
                username: "user".into(),
                password: Zeroizing::new("pa:ss".into()),
            }),
        };
        let mut s = connect(&proxy, "target.example", 2222)
            .await
            .expect("connect");
        // The banner sent together with the 200 must not have been swallowed.
        let mut banner = [0u8; 14];
        s.read_exact(&mut banner).await.expect("banner");
        assert_eq!(&banner, b"SSH-2.0-fake\r\n");
        s.write_all(b"ping").await.expect("write");
        let mut back = [0u8; 4];
        s.read_exact(&mut back).await.expect("read");
        assert_eq!(&back, b"ping");
    }

    #[tokio::test]
    async fn http_connect_auth_required() {
        let echo = echo_server().await;
        let port = fake_http_proxy(echo, Some("x")).await;
        let proxy = Proxy {
            kind: ProxyKind::Http,
            host: "127.0.0.1".into(),
            port,
            auth: None,
        };
        let err = connect(&proxy, "target.example", 2222)
            .await
            .expect_err("407");
        assert!(matches!(err, ProxyError::AuthRequired));
    }

    /// Fake SOCKS5 proxy accepting user/pass "u"/"p" (or no auth) and DOMAINNAME targets.
    async fn fake_socks5(upstream: std::net::SocketAddr, require_auth: bool) -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = l.local_addr().expect("addr").port();
        tokio::spawn(async move {
            let (mut s, _) = l.accept().await.expect("accept");
            let mut h = [0u8; 2];
            s.read_exact(&mut h).await.expect("greet");
            let mut methods = vec![0u8; usize::from(h[1])];
            s.read_exact(&mut methods).await.expect("methods");
            if require_auth {
                if !methods.contains(&2) {
                    s.write_all(&[5, 0xff]).await.expect("w");
                    return;
                }
                s.write_all(&[5, 2]).await.expect("w");
                let mut v = [0u8; 2];
                s.read_exact(&mut v).await.expect("r");
                let mut user = vec![0u8; usize::from(v[1])];
                s.read_exact(&mut user).await.expect("r");
                let mut pl = [0u8; 1];
                s.read_exact(&mut pl).await.expect("r");
                let mut pass = vec![0u8; usize::from(pl[0])];
                s.read_exact(&mut pass).await.expect("r");
                let ok = user == b"u" && pass == b"p";
                s.write_all(&[1, u8::from(!ok)]).await.expect("w");
                if !ok {
                    return;
                }
            } else {
                s.write_all(&[5, 0]).await.expect("w");
            }
            let mut req = [0u8; 4];
            s.read_exact(&mut req).await.expect("req");
            assert_eq!(&req[..3], &[5, 1, 0]);
            assert_eq!(req[3], 3, "host names must be sent as DOMAINNAME");
            let mut l = [0u8; 1];
            s.read_exact(&mut l).await.expect("len");
            let mut name = vec![0u8; usize::from(l[0]) + 2];
            s.read_exact(&mut name).await.expect("name");
            assert_eq!(&name[..name.len() - 2], b"target.example");
            assert_eq!(&name[name.len() - 2..], &2222u16.to_be_bytes());
            s.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 80])
                .await
                .expect("w");
            let mut up = TcpStream::connect(upstream).await.expect("upstream");
            let _ = tokio::io::copy_bidirectional(&mut s, &mut up).await;
        });
        port
    }

    #[tokio::test]
    async fn socks5_with_and_without_auth() {
        let echo = echo_server().await;
        for require_auth in [false, true] {
            let port = fake_socks5(echo, require_auth).await;
            let proxy = Proxy {
                kind: ProxyKind::Socks5,
                host: "127.0.0.1".into(),
                port,
                auth: require_auth.then(|| ProxyAuth {
                    username: "u".into(),
                    password: Zeroizing::new("p".into()),
                }),
            };
            let mut s = connect(&proxy, "target.example", 2222)
                .await
                .expect("connect");
            s.write_all(b"hello").await.expect("w");
            let mut back = [0u8; 5];
            s.read_exact(&mut back).await.expect("r");
            assert_eq!(&back, b"hello");
        }
    }

    #[tokio::test]
    async fn socks5_wrong_password() {
        let echo = echo_server().await;
        let port = fake_socks5(echo, true).await;
        let proxy = Proxy {
            kind: ProxyKind::Socks5,
            host: "127.0.0.1".into(),
            port,
            auth: Some(ProxyAuth {
                username: "u".into(),
                password: Zeroizing::new("wrong".into()),
            }),
        };
        let err = connect(&proxy, "target.example", 2222)
            .await
            .expect_err("auth");
        assert!(matches!(err, ProxyError::AuthFailed));
    }

    #[test]
    fn debug_redacts_password() {
        let a = ProxyAuth {
            username: "u".into(),
            password: Zeroizing::new("hunter2".into()),
        };
        assert!(!format!("{a:?}").contains("hunter2"));
    }
}
