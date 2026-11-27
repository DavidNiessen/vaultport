// SPDX-License-Identifier: GPL-3.0-or-later
//! Port forwarding: local (`-L`), remote (`-R`) and dynamic (`-D`, SOCKS5/SOCKS4a).
//!
//! Every forward returns a [`ForwardHandle`]; dropping it (or calling
//! [`ForwardHandle::stop`]) stops listening and aborts the connections it carries.
//!
//! **Security:** binding a local or dynamic forward to a non-loopback address exposes the
//! tunnel (for `-D`: an open proxy into the remote network) to everyone who can reach that
//! address. The engine allows it (it is sometimes wanted), the UI should warn.

use crate::connection::Connection;
use crate::error::SshError;
use crate::handler::ForwardedChannel;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio::task::{JoinHandle, JoinSet};

/// Kind of forward (re-exported from the model).
pub use core_model::ForwardKind;

/// Live counters of a forward.
#[derive(Debug, Default)]
pub struct ForwardStats {
    connections_total: AtomicU64,
    connections_active: AtomicU64,
    bytes_to_remote: AtomicU64,
    bytes_from_remote: AtomicU64,
    failures: AtomicU64,
    last_error: Mutex<Option<String>>,
}

/// Snapshot of [`ForwardStats`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ForwardStatsSnapshot {
    pub connections_total: u64,
    pub connections_active: u64,
    /// Bytes sent through the tunnel towards the SSH server side.
    pub bytes_to_remote: u64,
    /// Bytes received through the tunnel.
    pub bytes_from_remote: u64,
    /// Connections that could not be established (channel refused, connect failed…).
    pub failures: u64,
    pub last_error: Option<String>,
}

impl ForwardStats {
    fn snapshot(&self) -> ForwardStatsSnapshot {
        ForwardStatsSnapshot {
            connections_total: self.connections_total.load(Ordering::Relaxed),
            connections_active: self.connections_active.load(Ordering::Relaxed),
            bytes_to_remote: self.bytes_to_remote.load(Ordering::Relaxed),
            bytes_from_remote: self.bytes_from_remote.load(Ordering::Relaxed),
            failures: self.failures.load(Ordering::Relaxed),
            last_error: self.last_error.lock().ok().and_then(|e| e.clone()),
        }
    }

    fn fail(&self, msg: String) {
        self.failures.fetch_add(1, Ordering::Relaxed);
        tracing::debug!("forward connection failed: {msg}");
        if let Ok(mut e) = self.last_error.lock() {
            *e = Some(msg);
        }
    }
}

struct ActiveGuard(Arc<ForwardStats>);

impl ActiveGuard {
    fn new(stats: &Arc<ForwardStats>) -> Self {
        stats.connections_total.fetch_add(1, Ordering::Relaxed);
        stats.connections_active.fetch_add(1, Ordering::Relaxed);
        Self(stats.clone())
    }
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.0.connections_active.fetch_sub(1, Ordering::Relaxed);
    }
}

/// A running forward.
pub struct ForwardHandle {
    kind: ForwardKind,
    local_addr: Option<SocketAddr>,
    remote: Option<(String, u16)>,
    stats: Arc<ForwardStats>,
    stop: watch::Sender<bool>,
    task: Option<JoinHandle<()>>,
    conn: Connection,
}

impl std::fmt::Debug for ForwardHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForwardHandle")
            .field("kind", &self.kind)
            .field("local_addr", &self.local_addr)
            .field("remote", &self.remote)
            .finish_non_exhaustive()
    }
}

impl ForwardHandle {
    pub fn kind(&self) -> ForwardKind {
        self.kind
    }

    /// Local listening address (local and dynamic forwards).
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr
    }

    /// Remote bind address and the port actually bound by the server (remote forwards; the
    /// server picks the port when 0 was requested).
    pub fn remote_bind(&self) -> Option<(&str, u16)> {
        self.remote.as_ref().map(|(a, p)| (a.as_str(), *p))
    }

    pub fn stats(&self) -> ForwardStatsSnapshot {
        self.stats.snapshot()
    }

    /// Whether the forward is still running.
    pub fn is_running(&self) -> bool {
        self.task.as_ref().is_some_and(|t| !t.is_finished()) && !self.conn.is_closed()
    }

    /// Stops the forward (for remote forwards also cancels it on the server).
    pub async fn stop(mut self) {
        let _ = self.stop.send(true);
        if let Some(t) = self.task.take() {
            t.abort();
            let _ = t.await;
        }
        if let Some((addr, port)) = self.remote.take() {
            self.unregister(&addr, port);
            let _ = self
                .conn
                .handle()
                .cancel_tcpip_forward(addr, u32::from(port))
                .await;
        }
    }

    fn unregister(&self, addr: &str, port: u16) {
        if let Ok(mut m) = self.conn.inner.shared.remote_forwards.lock() {
            m.remove(&(addr.to_owned(), u32::from(port)));
        }
    }
}

impl Drop for ForwardHandle {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        if let Some(t) = self.task.take() {
            t.abort();
        }
        if let Some((addr, port)) = self.remote.take() {
            self.unregister(&addr, port);
            let conn = self.conn.clone();
            if let Ok(rt) = tokio::runtime::Handle::try_current() {
                rt.spawn(async move {
                    let _ = conn
                        .handle()
                        .cancel_tcpip_forward(addr, u32::from(port))
                        .await;
                });
            }
        }
    }
}

/// Byte-counting wrapper around a stream.
struct Counted<S> {
    inner: S,
    read: Arc<ForwardStats>,
    /// Bytes *read* from this stream count as `to_remote` when it is the local side.
    local_side: bool,
}

impl<S: AsyncRead + Unpin> AsyncRead for Counted<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let res = Pin::new(&mut self.inner).poll_read(cx, buf);
        let n = (buf.filled().len() - before) as u64;
        if n > 0 {
            let counter = if self.local_side {
                &self.read.bytes_to_remote
            } else {
                &self.read.bytes_from_remote
            };
            counter.fetch_add(n, Ordering::Relaxed);
        }
        res
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Counted<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

async fn pipe<L, R>(local: L, remote: R, stats: &Arc<ForwardStats>)
where
    L: AsyncRead + AsyncWrite + Unpin,
    R: AsyncRead + AsyncWrite + Unpin,
{
    let mut local = Counted {
        inner: local,
        read: stats.clone(),
        local_side: true,
    };
    let mut remote = Counted {
        inner: remote,
        read: stats.clone(),
        local_side: false,
    };
    let _ = tokio::io::copy_bidirectional(&mut local, &mut remote).await;
}

fn peer_parts(addr: SocketAddr) -> (String, u32) {
    (addr.ip().to_string(), u32::from(addr.port()))
}

impl Connection {
    /// `-L bind:dest_host:dest_port`: listens on `bind` and tunnels every accepted connection
    /// to `dest_host:dest_port` as seen from the target server.
    pub async fn forward_local(
        &self,
        bind: SocketAddr,
        dest_host: String,
        dest_port: u16,
    ) -> Result<ForwardHandle, SshError> {
        let listener = TcpListener::bind(bind).await?;
        let local_addr = listener.local_addr()?;
        let stats = Arc::new(ForwardStats::default());
        let (stop, mut stop_rx) = watch::channel(false);
        let conn = self.clone();
        let st = stats.clone();
        let task = tokio::spawn(async move {
            let mut conns = JoinSet::new();
            loop {
                tokio::select! {
                    _ = stop_rx.changed() => break,
                    accepted = listener.accept() => {
                        let Ok((sock, peer)) = accepted else { continue };
                        let _ = sock.set_nodelay(true);
                        let conn = conn.clone();
                        let st = st.clone();
                        let dest_host = dest_host.clone();
                        conns.spawn(async move {
                            let _guard = ActiveGuard::new(&st);
                            let (oa, op) = peer_parts(peer);
                            match conn
                                .handle()
                                .channel_open_direct_tcpip(dest_host.clone(), u32::from(dest_port), oa, op)
                                .await
                            {
                                Ok(ch) => pipe(sock, ch.into_stream(), &st).await,
                                Err(e) => st.fail(format!("{dest_host}:{dest_port}: {e}")),
                            }
                        });
                    }
                    Some(_) = conns.join_next(), if !conns.is_empty() => {}
                }
                if conn.is_closed() {
                    break;
                }
            }
        });
        Ok(ForwardHandle {
            kind: ForwardKind::Local,
            local_addr: Some(local_addr),
            remote: None,
            stats,
            stop,
            task: Some(task),
            conn: self.clone(),
        })
    }

    /// `-R bind_addr:bind_port:dest_host:dest_port`: asks the server to listen on
    /// `bind_addr:bind_port` (`bind_port == 0` lets it choose; see
    /// [`ForwardHandle::remote_bind`]) and connects every forwarded connection to
    /// `dest_host:dest_port` from this device. Binding to non-loopback addresses requires
    /// `GatewayPorts` on the server.
    pub async fn forward_remote(
        &self,
        bind_addr: String,
        bind_port: u16,
        dest_host: String,
        dest_port: u16,
    ) -> Result<ForwardHandle, SshError> {
        let (tx, mut rx) = mpsc::unbounded_channel::<ForwardedChannel>();
        let requested = u32::from(bind_port);
        // Register before the request so early connections are not lost.
        if let Ok(mut m) = self.inner.shared.remote_forwards.lock() {
            m.insert((bind_addr.clone(), requested), tx.clone());
        }
        let bound = match self
            .handle()
            .tcpip_forward(bind_addr.clone(), requested)
            .await
        {
            Ok(p) => {
                if p == 0 {
                    requested
                } else {
                    p
                }
            }
            Err(e) => {
                if let Ok(mut m) = self.inner.shared.remote_forwards.lock() {
                    m.remove(&(bind_addr.clone(), requested));
                }
                return Err(match e {
                    russh::Error::RequestDenied => SshError::Refused(format!(
                        "the server refused to listen on {bind_addr}:{bind_port}"
                    )),
                    other => other.into(),
                });
            }
        };
        if bound != requested
            && let Ok(mut m) = self.inner.shared.remote_forwards.lock()
        {
            m.remove(&(bind_addr.clone(), requested));
            m.insert((bind_addr.clone(), bound), tx);
        }
        let stats = Arc::new(ForwardStats::default());
        let (stop, mut stop_rx) = watch::channel(false);
        let st = stats.clone();
        let conn = self.clone();
        let task = tokio::spawn(async move {
            let mut conns = JoinSet::new();
            loop {
                tokio::select! {
                    _ = stop_rx.changed() => break,
                    fc = rx.recv() => {
                        let Some(fc) = fc else { break };
                        let st = st.clone();
                        let dest_host = dest_host.clone();
                        conns.spawn(async move {
                            let _guard = ActiveGuard::new(&st);
                            tracing::debug!(from = %fc.originator.0, "remote forward connection");
                            match TcpStream::connect((dest_host.as_str(), dest_port)).await {
                                Ok(sock) => {
                                    let _ = sock.set_nodelay(true);
                                    // Here the "local side" is the channel from the server.
                                    pipe(fc.channel.into_stream(), sock, &st).await;
                                }
                                Err(e) => {
                                    st.fail(format!("{dest_host}:{dest_port}: {e}"));
                                    let _ = fc.channel.close().await;
                                }
                            }
                        });
                    }
                    Some(_) = conns.join_next(), if !conns.is_empty() => {}
                }
                if conn.is_closed() {
                    break;
                }
            }
        });
        Ok(ForwardHandle {
            kind: ForwardKind::Remote,
            local_addr: None,
            remote: Some((bind_addr, bound as u16)),
            stats,
            stop,
            task: Some(task),
            conn: self.clone(),
        })
    }

    /// `-D bind`: a SOCKS5 (no authentication, `CONNECT`, IPv4/IPv6/domain) and SOCKS4/4a
    /// proxy whose connections are tunnelled through the target server. Domain names are
    /// resolved by the server.
    pub async fn forward_dynamic(&self, bind: SocketAddr) -> Result<ForwardHandle, SshError> {
        let listener = TcpListener::bind(bind).await?;
        let local_addr = listener.local_addr()?;
        let stats = Arc::new(ForwardStats::default());
        let (stop, mut stop_rx) = watch::channel(false);
        let conn = self.clone();
        let st = stats.clone();
        let task = tokio::spawn(async move {
            let mut conns = JoinSet::new();
            loop {
                tokio::select! {
                    _ = stop_rx.changed() => break,
                    accepted = listener.accept() => {
                        let Ok((sock, peer)) = accepted else { continue };
                        let _ = sock.set_nodelay(true);
                        let conn = conn.clone();
                        let st = st.clone();
                        conns.spawn(async move {
                            let _guard = ActiveGuard::new(&st);
                            if let Err(e) = socks_session(&conn, sock, peer, &st).await {
                                st.fail(e.to_string());
                            }
                        });
                    }
                    Some(_) = conns.join_next(), if !conns.is_empty() => {}
                }
                if conn.is_closed() {
                    break;
                }
            }
        });
        Ok(ForwardHandle {
            kind: ForwardKind::Dynamic,
            local_addr: Some(local_addr),
            remote: None,
            stats,
            stop,
            task: Some(task),
            conn: self.clone(),
        })
    }
}

/// Target requested by a SOCKS client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SocksRequest {
    V5 { host: String, port: u16 },
    V4 { host: String, port: u16 },
}

const SOCKS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

/// Reads a SOCKS4/4a/5 `CONNECT` request (no authentication).
pub(crate) async fn read_socks_request<S: AsyncRead + AsyncWrite + Unpin>(
    s: &mut S,
) -> io::Result<SocksRequest> {
    let invalid = |m: &str| io::Error::new(io::ErrorKind::InvalidData, m.to_owned());
    let mut ver = [0u8; 1];
    s.read_exact(&mut ver).await?;
    match ver[0] {
        5 => {
            let mut n = [0u8; 1];
            s.read_exact(&mut n).await?;
            let mut methods = vec![0u8; usize::from(n[0])];
            s.read_exact(&mut methods).await?;
            if !methods.contains(&0) {
                s.write_all(&[5, 0xff]).await?;
                return Err(invalid("SOCKS5 client does not offer 'no authentication'"));
            }
            s.write_all(&[5, 0]).await?;
            let mut head = [0u8; 4];
            s.read_exact(&mut head).await?;
            if head[0] != 5 {
                return Err(invalid("bad SOCKS5 request"));
            }
            if head[1] != 1 {
                socks5_reply(s, 7).await?;
                return Err(invalid("only CONNECT is supported"));
            }
            let host = match head[3] {
                1 => {
                    let mut a = [0u8; 4];
                    s.read_exact(&mut a).await?;
                    Ipv4Addr::from(a).to_string()
                }
                4 => {
                    let mut a = [0u8; 16];
                    s.read_exact(&mut a).await?;
                    Ipv6Addr::from(a).to_string()
                }
                3 => {
                    let mut l = [0u8; 1];
                    s.read_exact(&mut l).await?;
                    let mut name = vec![0u8; usize::from(l[0])];
                    s.read_exact(&mut name).await?;
                    String::from_utf8(name).map_err(|_| invalid("non-UTF-8 host name"))?
                }
                _ => {
                    socks5_reply(s, 8).await?;
                    return Err(invalid("unsupported address type"));
                }
            };
            let mut port = [0u8; 2];
            s.read_exact(&mut port).await?;
            Ok(SocksRequest::V5 {
                host,
                port: u16::from_be_bytes(port),
            })
        }
        4 => {
            let mut head = [0u8; 7];
            s.read_exact(&mut head).await?;
            if head[0] != 1 {
                s.write_all(&[0, 91, 0, 0, 0, 0, 0, 0]).await?;
                return Err(invalid("only SOCKS4 CONNECT is supported"));
            }
            let port = u16::from_be_bytes([head[1], head[2]]);
            let ip = [head[3], head[4], head[5], head[6]];
            read_cstring(s, 255).await?; // user id, ignored
            let host = if ip[0] == 0 && ip[1] == 0 && ip[2] == 0 && ip[3] != 0 {
                // SOCKS4a: host name follows.
                let name = read_cstring(s, 255).await?;
                String::from_utf8(name).map_err(|_| invalid("non-UTF-8 host name"))?
            } else {
                Ipv4Addr::from(ip).to_string()
            };
            Ok(SocksRequest::V4 { host, port })
        }
        _ => Err(invalid("not a SOCKS request")),
    }
}

async fn read_cstring<S: AsyncRead + Unpin>(s: &mut S, max: usize) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut b = [0u8; 1];
    loop {
        s.read_exact(&mut b).await?;
        if b[0] == 0 {
            return Ok(out);
        }
        if out.len() >= max {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "string too long",
            ));
        }
        out.push(b[0]);
    }
}

async fn socks5_reply<S: AsyncWrite + Unpin>(s: &mut S, code: u8) -> io::Result<()> {
    s.write_all(&[5, code, 0, 1, 0, 0, 0, 0, 0, 0]).await
}

async fn socks_session(
    conn: &Connection,
    mut sock: TcpStream,
    peer: SocketAddr,
    stats: &Arc<ForwardStats>,
) -> Result<(), SshError> {
    let req = tokio::time::timeout(SOCKS_HANDSHAKE_TIMEOUT, read_socks_request(&mut sock))
        .await
        .map_err(|_| SshError::Timeout(SOCKS_HANDSHAKE_TIMEOUT))??;
    let (host, port, v5) = match req {
        SocksRequest::V5 { host, port } => (host, port, true),
        SocksRequest::V4 { host, port } => (host, port, false),
    };
    let (oa, op) = peer_parts(peer);
    match conn
        .handle()
        .channel_open_direct_tcpip(host.clone(), u32::from(port), oa, op)
        .await
    {
        Ok(ch) => {
            if v5 {
                socks5_reply(&mut sock, 0).await?;
            } else {
                sock.write_all(&[0, 90, 0, 0, 0, 0, 0, 0]).await?;
            }
            pipe(sock, ch.into_stream(), stats).await;
            Ok(())
        }
        Err(e) => {
            let code = match &e {
                russh::Error::ChannelOpenFailure(russh::ChannelOpenFailure::ConnectFailed) => 5,
                russh::Error::ChannelOpenFailure(
                    russh::ChannelOpenFailure::AdministrativelyProhibited,
                ) => 2,
                _ => 1,
            };
            if v5 {
                let _ = socks5_reply(&mut sock, code).await;
            } else {
                let _ = sock.write_all(&[0, 91, 0, 0, 0, 0, 0, 0]).await;
            }
            Err(SshError::Refused(format!("{host}:{port}: {e}")))
        }
    }
}

/// Parses `host:port` / `[v6]:port` into a [`SocketAddr`] for local binds.
pub fn parse_bind(addr: &str, port: u16) -> Result<SocketAddr, SshError> {
    let a = addr.trim().trim_start_matches('[').trim_end_matches(']');
    let ip: IpAddr = match a {
        "" | "localhost" => IpAddr::V4(Ipv4Addr::LOCALHOST),
        "*" => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        other => other
            .parse()
            .map_err(|_| SshError::Config(format!("invalid bind address {other}")))?,
    };
    Ok(SocketAddr::new(ip, port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::{ProxyAuth, socks5_connect};

    #[tokio::test]
    async fn socks5_server_parses_what_our_client_sends() {
        for host in ["example.com", "10.1.2.3", "::1"] {
            let (mut a, mut b) = tokio::io::duplex(1024);
            let client = async { socks5_connect(&mut a, host, 8080, None::<&ProxyAuth>).await };
            let server = async {
                let r = read_socks_request(&mut b).await.expect("parse");
                socks5_reply(&mut b, 0).await.expect("reply");
                r
            };
            let (c, s) = tokio::join!(client, server);
            c.expect("client ok");
            assert_eq!(
                s,
                SocksRequest::V5 {
                    host: host.to_owned(),
                    port: 8080
                }
            );
        }
    }

    #[tokio::test]
    async fn socks4a_request() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        let mut req = vec![4, 1, 0x1f, 0x90, 0, 0, 0, 1];
        req.extend_from_slice(b"user\0example.org\0");
        a.write_all(&req).await.expect("w");
        let r = read_socks_request(&mut b).await.expect("parse");
        assert_eq!(
            r,
            SocksRequest::V4 {
                host: "example.org".into(),
                port: 8080
            }
        );
        let (mut a, mut b) = tokio::io::duplex(1024);
        a.write_all(&[4, 1, 0, 80, 192, 168, 1, 2, 0])
            .await
            .expect("w");
        assert_eq!(
            read_socks_request(&mut b).await.expect("parse"),
            SocksRequest::V4 {
                host: "192.168.1.2".into(),
                port: 80
            }
        );
    }

    #[tokio::test]
    async fn socks_rejects_garbage() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        a.write_all(b"GET / HTTP/1.1\r\n").await.expect("w");
        assert!(read_socks_request(&mut b).await.is_err());
        // SOCKS5 with only user/pass offered.
        let (mut a, mut b) = tokio::io::duplex(1024);
        a.write_all(&[5, 1, 2]).await.expect("w");
        assert!(read_socks_request(&mut b).await.is_err());
        let mut resp = [0u8; 2];
        a.read_exact(&mut resp).await.expect("r");
        assert_eq!(resp, [5, 0xff]);
    }

    #[test]
    fn bind_parsing() {
        assert_eq!(
            parse_bind("localhost", 80).expect("ok"),
            "127.0.0.1:80".parse().expect("addr")
        );
        assert_eq!(
            parse_bind("[::1]", 1).expect("ok"),
            "[::1]:1".parse().expect("addr")
        );
        assert!(parse_bind("nonsense host", 1).is_err());
    }
}
