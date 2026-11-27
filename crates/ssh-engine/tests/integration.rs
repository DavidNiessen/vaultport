// SPDX-License-Identifier: GPL-3.0-or-later
//! Integration tests against real OpenSSH servers in docker (see `docker/test-servers`).
//!
//! Run with `cargo test -p ssh-engine --features integration`. Without docker the tests print
//! a notice and pass.

#![cfg(feature = "integration")]
#![allow(clippy::print_stdout, clippy::unwrap_used)]

mod support;

use core_model::{HostKeyPolicy, KeyAlgorithm, KnownHostMarker};
use ssh_engine::keys::{ExportOutcome, GenerateOptions, export_to_host, generate};
use ssh_engine::known_hosts::{new_entry, parse_known_hosts_line};
use ssh_engine::ssh_key::PublicKey;
use ssh_engine::{
    AuthMethod, CloseCategory, ConnectOptions, ExecOptions, HostKeyDecision, HostKeyRejection,
    HostKeyStatus, KnownHosts, MemoryKnownHosts, Proxy, ProxyKind, ShellEvent, ShellRequest,
    SshError, Zeroizing, connect,
};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use support::{ALICE_PASSWORD, BOB_PASSWORD, Env, TestUi};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

fn known_a(env: &Env) -> Arc<MemoryKnownHosts> {
    let line = format!("[{}]:{} {}", env.a.host, env.a.port, env.host_key_a());
    Arc::new(MemoryKnownHosts::new(vec![
        parse_known_hosts_line(&line).unwrap(),
    ]))
}

async fn connect_ok(
    opts: ConnectOptions,
    ui: Arc<TestUi>,
    kh: Arc<MemoryKnownHosts>,
) -> ssh_engine::Connection {
    match connect(opts, ui, kh).await {
        Ok(c) => c,
        Err(e) => panic!("connect failed: {e:?}"),
    }
}

async fn whoami(conn: &ssh_engine::Connection) -> String {
    conn.exec("whoami")
        .await
        .unwrap()
        .stdout_string()
        .trim()
        .to_owned()
}

#[tokio::test]
async fn password_auth_exec_and_algorithms() {
    let env = require_env!();
    let ui = Arc::new(TestUi::accept());
    let conn = connect_ok(env.alice_password(), ui.clone(), known_a(env)).await;
    assert_eq!(ui.prompts(), 0, "known host must not prompt");
    assert_eq!(whoami(&conn).await, "alice");
    let out = conn.exec("echo out; echo err >&2; exit 3").await.unwrap();
    assert_eq!(out.stdout_string(), "out\n");
    assert_eq!(String::from_utf8_lossy(&out.stderr), "err\n");
    assert_eq!(out.exit_status, Some(3));

    let alg = conn.negotiated_algorithms().unwrap();
    println!("negotiated with OpenSSH: {alg:?}");
    assert_eq!(alg.kex, "mlkem768x25519-sha256");
    assert!(alg.post_quantum);
    assert!(alg.strict_kex);
    assert_eq!(alg.host_key, "ssh-ed25519");
    assert_eq!(alg.cipher, "chacha20-poly1305@openssh.com");
    assert_eq!(conn.detect_os().await.as_deref(), Some("alpine"));
    assert_eq!(conn.detect_os_kind().await, core_model::OsKind::Alpine);

    // Output limit.
    let err = conn
        .exec_with(
            "head -c 100000 /dev/zero",
            ExecOptions {
                max_output: 1000,
                ..ExecOptions::default()
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, SshError::OutputLimit(1000)));

    conn.close().await;
    let reason = tokio::time::timeout(Duration::from_secs(5), conn.closed())
        .await
        .unwrap();
    assert_eq!(reason.category, CloseCategory::Local);
    assert!(conn.is_closed());
}

#[tokio::test]
async fn classic_kex_without_post_quantum_and_compression() {
    let env = require_env!();
    let mut opts = env.alice_password();
    opts.post_quantum = false;
    opts.compression = true;
    let conn = connect_ok(opts, Arc::new(TestUi::accept()), known_a(env)).await;
    let alg = conn.negotiated_algorithms().unwrap();
    assert_eq!(alg.kex, "curve25519-sha256");
    assert!(!alg.post_quantum);
    assert_eq!(alg.compression_client_to_server, "zlib@openssh.com");
    // Compressed transport still works for bulk data.
    let out = conn.exec("head -c 300000 /dev/zero | wc -c").await.unwrap();
    assert_eq!(out.stdout_string().trim(), "300000");
}

#[tokio::test]
async fn wrong_password_fails_with_auth_category() {
    let env = require_env!();
    let opts = ConnectOptions::new(env.hop_a("alice").with_auth(AuthMethod::password("nope")));
    let err = connect(opts, Arc::new(TestUi::accept()), known_a(env))
        .await
        .unwrap_err();
    assert!(matches!(err, SshError::AuthFailed { .. }), "{err:?}");
    assert_eq!(err.category(), CloseCategory::AuthFailed);
}

#[tokio::test]
async fn password_prompt_fallback() {
    let env = require_env!();
    let mut hop = env.hop_a("alice");
    hop.interactive_fallback = true;
    let mut ui = TestUi::accept();
    // keyboard-interactive (PAM) is offered first and declined, then the password prompt.
    ui.password = Some(ALICE_PASSWORD.into());
    let ui = Arc::new(ui);
    let conn = connect_ok(ConnectOptions::new(hop), ui.clone(), known_a(env)).await;
    assert_eq!(whoami(&conn).await, "alice");
}

#[tokio::test]
async fn keyboard_interactive_via_pam() {
    let env = require_env!();
    let mut ui = TestUi::accept();
    ui.kbd_answer = Some(ALICE_PASSWORD.into());
    let ui = Arc::new(ui);
    let opts = ConnectOptions::new(
        env.hop_a("alice")
            .with_auth(AuthMethod::KeyboardInteractive),
    );
    let conn = connect_ok(opts, ui.clone(), known_a(env)).await;
    assert!(ui.kbd_prompts.load(Ordering::SeqCst) >= 1);
    assert_eq!(whoami(&conn).await, "alice");
}

#[tokio::test]
async fn ed25519_key_auth() {
    let env = require_env!();
    let key = generate(&GenerateOptions::new(KeyAlgorithm::Ed25519)).unwrap();
    env.authorize("alice", &key.public_key);
    let opts = ConnectOptions::new(
        env.hop_a("alice")
            .with_auth(AuthMethod::key(key.private_key_pem.as_str())),
    );
    let conn = connect_ok(opts, Arc::new(TestUi::accept()), known_a(env)).await;
    assert_eq!(whoami(&conn).await, "alice");
}

#[tokio::test]
async fn encrypted_rsa_key_with_passphrase_prompt() {
    let env = require_env!();
    let mut o = GenerateOptions::new(KeyAlgorithm::Rsa);
    o.rsa_bits = 3072;
    o.passphrase = Some(Zeroizing::new("s3cret phrase".into()));
    let key = generate(&o).unwrap();
    env.authorize("alice", &key.public_key);
    let mut ui = TestUi::accept();
    ui.passphrase = Some("s3cret phrase".into());
    let opts = ConnectOptions::new(
        env.hop_a("alice")
            .with_auth(AuthMethod::key(key.private_key_pem.as_str())),
    );
    let conn = connect_ok(opts, Arc::new(ui), known_a(env)).await;
    assert_eq!(whoami(&conn).await, "alice");
    // The server must have accepted an rsa-sha2-* signature (ssh-rsa is disabled in OpenSSH 10).
    let logs = std::process::Command::new("docker")
        .args(["logs", &env.a.name])
        .output()
        .unwrap();
    let logs = String::from_utf8_lossy(&logs.stderr);
    assert!(
        logs.contains("Accepted publickey for alice from") && logs.contains("RSA"),
        "no RSA publickey login in sshd log"
    );
}

#[tokio::test]
async fn certificate_auth() {
    let env = require_env!();
    let key = russh::keys::PrivateKey::random(
        &mut support::ssh_engine_rng(),
        ssh_engine::ssh_key::Algorithm::Ed25519,
    )
    .unwrap();
    let cert = env.sign_user_cert(&key, "alice");
    let pem = key
        .to_openssh(ssh_engine::ssh_key::LineEnding::LF)
        .unwrap()
        .to_string();
    // Without the certificate the (unauthorized) key is rejected...
    let opts = ConnectOptions::new(env.hop_a("alice").with_auth(AuthMethod::key(pem.clone())));
    assert!(matches!(
        connect(opts, Arc::new(TestUi::accept()), known_a(env)).await,
        Err(SshError::AuthFailed { .. })
    ));
    // ...with it, the CA vouches for it.
    let opts = ConnectOptions::new(env.hop_a("alice").with_auth(AuthMethod::PrivateKey {
        key_pem: Zeroizing::new(pem),
        passphrase: None,
        certificate: Some(cert),
    }));
    let conn = connect_ok(opts, Arc::new(TestUi::accept()), known_a(env)).await;
    assert_eq!(whoami(&conn).await, "alice");
}

#[tokio::test]
async fn partial_success_publickey_then_password() {
    let env = require_env!();
    let key = generate(&GenerateOptions::new(KeyAlgorithm::EcdsaP256)).unwrap();
    env.authorize("bob", &key.public_key);
    // Password listed first: it is not allowed until the key succeeded partially.
    let opts = ConnectOptions::new(
        env.hop_a("bob")
            .with_auth(AuthMethod::password(BOB_PASSWORD))
            .with_auth(AuthMethod::key(key.private_key_pem.as_str())),
    );
    let conn = connect_ok(opts, Arc::new(TestUi::accept()), known_a(env)).await;
    assert_eq!(whoami(&conn).await, "bob");
}

#[tokio::test]
async fn unknown_host_prompt_and_save_then_strict() {
    let env = require_env!();
    let kh = Arc::new(MemoryKnownHosts::default());
    let ui = Arc::new(TestUi::new(HostKeyDecision::AcceptAndSave));
    let conn = connect_ok(env.alice_password(), ui.clone(), kh.clone()).await;
    assert_eq!(ui.prompts(), 1);
    let check = ui.checks.lock().unwrap()[0].clone();
    assert_eq!(check.status, HostKeyStatus::Unknown);
    assert!(check.fingerprint.starts_with("SHA256:"));
    assert!(check.randomart.starts_with("+--[ED25519 256]--+"));
    let expected = PublicKey::from_openssh(&env.host_key_a()).unwrap();
    assert_eq!(
        check.fingerprint,
        ssh_engine::fingerprint::sha256_fingerprint(expected.key_data())
    );
    assert_eq!(
        conn.hop_info()[0].host_key_fingerprint.as_deref(),
        Some(check.fingerprint.as_str())
    );
    conn.close().await;
    let saved = kh.entries();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].pattern, format!("[127.0.0.1]:{}", env.a.port));

    // Now known: strict policy connects without prompting.
    let mut opts = env.alice_password();
    opts.hops[0].host_key_policy = HostKeyPolicy::Strict;
    let ui2 = Arc::new(TestUi::new(HostKeyDecision::Reject));
    let conn = connect_ok(opts, ui2.clone(), kh).await;
    assert_eq!(ui2.prompts(), 0);
    conn.close().await;
}

#[tokio::test]
async fn strict_policy_rejects_unknown_without_prompt() {
    let env = require_env!();
    let mut opts = env.alice_password();
    opts.hops[0].host_key_policy = HostKeyPolicy::Strict;
    let ui = Arc::new(TestUi::new(HostKeyDecision::AcceptOnce));
    let err = connect(opts, ui.clone(), Arc::new(MemoryKnownHosts::default()))
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            SshError::HostKeyRejected {
                reason: HostKeyRejection::UnknownStrict,
                ..
            }
        ),
        "{err:?}"
    );
    assert_eq!(err.category(), CloseCategory::HostKeyRejected);
    assert_eq!(ui.prompts(), 0);
}

#[tokio::test]
async fn changed_host_key_is_reported_and_rejected() {
    let env = require_env!();
    let other = generate(&GenerateOptions::new(KeyAlgorithm::Ed25519)).unwrap();
    let other_pk = PublicKey::from_openssh(&other.public_key).unwrap();
    let kh = Arc::new(MemoryKnownHosts::new(vec![new_entry(
        &env.a.host,
        env.a.port,
        other_pk.key_data(),
        None,
    )]));
    let ui = Arc::new(TestUi::new(HostKeyDecision::Reject));
    let err = connect(env.alice_password(), ui.clone(), kh.clone())
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            SshError::HostKeyRejected {
                reason: HostKeyRejection::Declined,
                ..
            }
        ),
        "{err:?}"
    );
    let check = ui.checks.lock().unwrap()[0].clone();
    assert_eq!(
        check.status,
        HostKeyStatus::Changed {
            previous_fingerprints: vec![other.fingerprint.clone()]
        }
    );
    // Strict: rejected without asking.
    let mut opts = env.alice_password();
    opts.hops[0].host_key_policy = HostKeyPolicy::Strict;
    let ui = Arc::new(TestUi::new(HostKeyDecision::AcceptAndSave));
    let err = connect(opts, ui.clone(), kh.clone()).await.unwrap_err();
    assert!(matches!(
        err,
        SshError::HostKeyRejected {
            reason: HostKeyRejection::ChangedStrict,
            ..
        }
    ));
    assert_eq!(ui.prompts(), 0);
    // Accepting the change replaces the old entry.
    let ui = Arc::new(TestUi::new(HostKeyDecision::AcceptAndSave));
    let conn = connect_ok(env.alice_password(), ui, kh.clone()).await;
    conn.close().await;
    let entries = kh.entries();
    assert_eq!(entries.len(), 1);
    assert_ne!(entries[0].fingerprint, other.fingerprint);
}

#[tokio::test]
async fn revoked_host_key_is_refused() {
    let env = require_env!();
    let mut kh = known_a(env).entries();
    let mut revoked = parse_known_hosts_line(&format!("* {}", env.host_key_a())).unwrap();
    revoked.marker = Some(KnownHostMarker::Revoked);
    kh.push(revoked);
    let ui = Arc::new(TestUi::new(HostKeyDecision::AcceptAndSave));
    let err = connect(
        env.alice_password(),
        ui.clone(),
        Arc::new(MemoryKnownHosts::new(kh)),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        SshError::HostKeyRejected {
            reason: HostKeyRejection::Revoked,
            ..
        }
    ));
    assert_eq!(ui.checks.lock().unwrap()[0].status, HostKeyStatus::Revoked);
}

#[tokio::test]
async fn host_certificate_trusted_via_cert_authority() {
    let env = require_env!();
    // Non-standard port: the pattern must be `[host]:port`, exactly like OpenSSH.
    let mut ca = parse_known_hosts_line(&format!(
        "@cert-authority [127.0.0.1]:{},[localhost]:{} {}",
        env.a.port,
        env.a.port,
        env.host_ca_a()
    ))
    .unwrap();
    ca.marker = Some(KnownHostMarker::CertAuthority);
    let kh = Arc::new(MemoryKnownHosts::new(vec![ca]));
    let mut opts = env.alice_password();
    opts.hops[0].host_key_policy = HostKeyPolicy::Strict;
    let ui = Arc::new(TestUi::new(HostKeyDecision::Reject));
    let conn = connect_ok(opts, ui.clone(), kh).await;
    assert_eq!(ui.prompts(), 0);
    let info = &conn.hop_info()[0];
    assert!(info.trusted_by_certificate);
    assert!(
        info.algorithms
            .as_ref()
            .unwrap()
            .host_key
            .contains("ssh-ed25519")
    );
}

#[tokio::test]
async fn jump_host_chain() {
    let env = require_env!();
    let mut opts = ConnectOptions::new(
        env.hop_b("alice")
            .with_auth(AuthMethod::password(ALICE_PASSWORD)),
    );
    opts.hops.insert(
        0,
        env.hop_a("alice")
            .with_auth(AuthMethod::password(ALICE_PASSWORD)),
    );
    let kh = known_a(env);
    let ui = Arc::new(TestUi::new(HostKeyDecision::AcceptAndSave));
    let conn = connect_ok(opts, ui.clone(), kh.clone()).await;
    // Only the second hop was unknown.
    assert_eq!(ui.prompts(), 1);
    let check = ui.checks.lock().unwrap()[0].clone();
    assert_eq!(check.hop_index, 1);
    assert_eq!(check.host, env.b.host);
    let hostname = conn.exec("hostname").await.unwrap().stdout_string();
    assert_eq!(hostname.trim(), env.b.name);
    assert_eq!(conn.hop_info().len(), 2);
    assert!(
        kh.lookup(&env.b.host, 22)
            .iter()
            .any(|e| e.pattern == env.b.host)
    );
    conn.close().await;
}

#[tokio::test]
async fn through_http_connect_proxy() {
    let env = require_env!();
    // Minimal CONNECT proxy that only allows the test server.
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let pport = l.local_addr().unwrap().port();
    let target = format!("127.0.0.1:{}", env.a.port);
    tokio::spawn(async move {
        let (mut s, _) = l.accept().await.unwrap();
        let mut buf = Vec::new();
        let mut b = [0u8; 1];
        while !buf.ends_with(b"\r\n\r\n") {
            s.read_exact(&mut b).await.unwrap();
            buf.push(b[0]);
        }
        let req = String::from_utf8(buf).unwrap();
        assert!(req.starts_with(&format!("CONNECT {target} ")));
        let mut up = TcpStream::connect(&target).await.unwrap();
        s.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
        let _ = tokio::io::copy_bidirectional(&mut s, &mut up).await;
    });
    let mut opts = env.alice_password();
    opts.proxy = Some(Proxy {
        kind: ProxyKind::Http,
        host: "127.0.0.1".into(),
        port: pport,
        auth: None,
    });
    let conn = connect_ok(opts, Arc::new(TestUi::accept()), known_a(env)).await;
    assert_eq!(whoami(&conn).await, "alice");
}

#[tokio::test]
async fn shell_echo_env_and_exit() {
    let env = require_env!();
    let conn = connect_ok(
        env.alice_password(),
        Arc::new(TestUi::accept()),
        known_a(env),
    )
    .await;
    let (shell, mut events) = conn
        .open_shell(ShellRequest {
            env: vec![("SSHC_TEST_VAR".into(), "from-client".into())],
            cols: 120,
            rows: 40,
            ..ShellRequest::default()
        })
        .await
        .unwrap();
    shell.resize(100, 30).await.unwrap();
    shell
        .write(b"echo \"marker-$SSHC_TEST_VAR-$(stty size)\"; exit 7\n")
        .await
        .unwrap();
    let mut output = Vec::new();
    let mut status = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let ev = tokio::time::timeout_at(deadline, events.recv())
            .await
            .expect("shell timed out");
        match ev {
            Some(ShellEvent::Data(d)) => output.extend_from_slice(&d),
            Some(ShellEvent::ExitStatus(s)) => status = Some(s),
            Some(ShellEvent::Closed) | None => break,
            Some(_) => {}
        }
    }
    let text = String::from_utf8_lossy(&output);
    assert!(text.contains("marker-from-client-30 100"), "{text}");
    assert_eq!(status, Some(7));

    // Exec with an environment variable.
    let out = conn
        .exec_with(
            "printf %s \"$SSHC_OTHER\"",
            ExecOptions {
                env: vec![("SSHC_OTHER".into(), "v1".into())],
                stdin: None,
                ..ExecOptions::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(out.stdout_string(), "v1");
    // Exec with stdin.
    let out = conn
        .exec_with(
            "wc -c",
            ExecOptions {
                stdin: Some(vec![b'x'; 70_000]),
                ..ExecOptions::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(out.stdout_string().trim(), "70000");
}

#[tokio::test]
async fn local_forward_roundtrip() {
    let env = require_env!();
    let conn = connect_ok(
        env.alice_password(),
        Arc::new(TestUi::accept()),
        known_a(env),
    )
    .await;
    let fwd = conn
        .forward_local("127.0.0.1:0".parse().unwrap(), "127.0.0.1".into(), 7777)
        .await
        .unwrap();
    let addr = fwd.local_addr().unwrap();
    for _ in 0..3 {
        let mut s = TcpStream::connect(addr).await.unwrap();
        let payload = vec![b'z'; 200_000];
        let (mut r, mut w) = s.split();
        let write = async {
            w.write_all(&payload).await.unwrap();
        };
        let read = async {
            let mut got = vec![0u8; payload.len()];
            r.read_exact(&mut got).await.unwrap();
            got
        };
        let ((), got) = tokio::join!(write, read);
        assert_eq!(got, payload);
    }
    let stats = fwd.stats();
    assert_eq!(stats.connections_total, 3);
    assert!(stats.bytes_to_remote >= 600_000);
    assert!(stats.bytes_from_remote >= 600_000);
    fwd.stop().await;
    assert!(TcpStream::connect(addr).await.is_err());
}

#[tokio::test]
async fn remote_forward_roundtrip() {
    let env = require_env!();
    let conn = connect_ok(
        env.alice_password(),
        Arc::new(TestUi::accept()),
        known_a(env),
    )
    .await;
    // Local service the server will reach through the tunnel.
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut s, _)) = l.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 64];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let _ = s.write_all(b"pong:").await;
                let _ = s.write_all(&buf[..n]).await;
            });
        }
    });
    let fwd = conn
        .forward_remote("127.0.0.1".into(), 0, "127.0.0.1".into(), local_port)
        .await
        .unwrap();
    let (_, port) = fwd.remote_bind().unwrap();
    assert_ne!(port, 0);
    let out = conn
        .exec(&format!("printf ping | socat -t 3 - TCP:127.0.0.1:{port}"))
        .await
        .unwrap();
    assert_eq!(out.stdout_string(), "pong:ping");
    assert_eq!(fwd.stats().connections_total, 1);
    fwd.stop().await;
    let out = conn
        .exec(&format!(
            "socat -t 1 - TCP:127.0.0.1:{port} </dev/null; echo rc=$?"
        ))
        .await
        .unwrap();
    assert!(
        !out.stdout_string().contains("rc=0"),
        "forward still active"
    );
}

#[tokio::test]
async fn dynamic_forward_socks5() {
    let env = require_env!();
    let conn = connect_ok(
        env.alice_password(),
        Arc::new(TestUi::accept()),
        known_a(env),
    )
    .await;
    let fwd = conn
        .forward_dynamic("127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    let addr = fwd.local_addr().unwrap();
    // HTTP through SOCKS5 with a host name resolved by the server.
    let mut s = TcpStream::connect(addr).await.unwrap();
    ssh_engine::proxy::socks5_connect(&mut s, "localhost", 8080, None)
        .await
        .unwrap();
    s.write_all(b"GET /index.txt HTTP/1.0\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    let mut resp = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(5), s.read_to_end(&mut resp)).await;
    let resp = String::from_utf8_lossy(&resp);
    assert!(resp.starts_with("HTTP/1.0 200 OK"), "{resp}");
    assert!(resp.contains("hello from the test http server"));
    // Echo service through an IPv4 literal.
    let mut s = TcpStream::connect(addr).await.unwrap();
    ssh_engine::proxy::socks5_connect(&mut s, "127.0.0.1", 7777, None)
        .await
        .unwrap();
    s.write_all(b"echo!").await.unwrap();
    let mut back = [0u8; 5];
    s.read_exact(&mut back).await.unwrap();
    assert_eq!(&back, b"echo!");
    // Refused destination is reported as a SOCKS error.
    let mut s = TcpStream::connect(addr).await.unwrap();
    let err = ssh_engine::proxy::socks5_connect(&mut s, "127.0.0.1", 1, None).await;
    assert!(err.is_err());
    fwd.stop().await;
}

#[tokio::test]
async fn agent_forwarding_signs_with_confirmation() {
    let env = require_env!();
    let key = generate(&GenerateOptions::new(KeyAlgorithm::Ed25519)).unwrap();
    env.authorize("alice", &key.public_key);
    let ui = Arc::new(TestUi::accept());
    let opts = ConnectOptions::new(
        env.hop_a("alice")
            .with_auth(AuthMethod::key(key.private_key_pem.as_str())),
    );
    let conn = connect_ok(opts, ui.clone(), known_a(env)).await;
    // From the server, use the forwarded agent to log into the server itself.
    let (_shell, mut events) = conn
        .open_shell(ShellRequest {
            agent_forwarding: true,
            command: Some(
                "ssh-add -l; ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
                 -o BatchMode=yes -o LogLevel=ERROR alice@127.0.0.1 echo via-agent"
                    .into(),
            ),
            ..ShellRequest::default()
        })
        .await
        .unwrap();
    let mut out = Vec::new();
    while let Ok(Some(ev)) = tokio::time::timeout(Duration::from_secs(15), events.recv()).await {
        match ev {
            ShellEvent::Data(d) => out.extend_from_slice(&d),
            ShellEvent::Closed => break,
            _ => {}
        }
    }
    let out = String::from_utf8_lossy(&out);
    assert!(out.contains(&key.fingerprint), "{out}");
    assert!(out.contains("via-agent"), "{out}");
    assert!(ui.agent_confirmations.load(Ordering::SeqCst) >= 1);

    // A connection that never requested forwarding has no agent (sshd keeps the agent
    // socket per connection, so this needs a fresh connection).
    let other = connect_ok(
        env.alice_password(),
        Arc::new(TestUi::accept()),
        known_a(env),
    )
    .await;
    let out = other.exec("ssh-add -l; echo rc=$?").await.unwrap();
    assert!(
        out.stdout_string().contains("rc=2"),
        "{}",
        out.stdout_string()
    );
}

#[tokio::test]
async fn export_public_key_is_idempotent() {
    let env = require_env!();
    let conn = connect_ok(
        env.alice_password(),
        Arc::new(TestUi::accept()),
        known_a(env),
    )
    .await;
    let key = generate(&GenerateOptions::new(KeyAlgorithm::EcdsaP384)).unwrap();
    assert_eq!(
        export_to_host(&conn, &key.public_key, None).await.unwrap(),
        ExportOutcome::Added
    );
    assert_eq!(
        export_to_host(&conn, &key.public_key, None).await.unwrap(),
        ExportOutcome::AlreadyPresent
    );
    let perms = conn
        .exec("stat -c %a ~/.ssh ~/.ssh/authorized_keys")
        .await
        .unwrap()
        .stdout_string();
    assert_eq!(
        perms.split_whitespace().collect::<Vec<_>>(),
        vec!["700", "600"]
    );
    // The exported key now works.
    let opts = ConnectOptions::new(
        env.hop_a("alice")
            .with_auth(AuthMethod::key(key.private_key_pem.as_str())),
    );
    let c2 = connect_ok(opts, Arc::new(TestUi::accept()), known_a(env)).await;
    assert_eq!(whoami(&c2).await, "alice");
    // Custom path with spaces and quotes.
    assert_eq!(
        export_to_host(&conn, &key.public_key, Some("~/it's a dir/keys"))
            .await
            .unwrap(),
        ExportOutcome::Added
    );
    let out = conn.exec("cat ~/\"it's a dir\"/keys").await.unwrap();
    assert_eq!(out.stdout_string().trim(), key.public_key);
}

#[tokio::test]
async fn sftp_subsystem_is_available() {
    let env = require_env!();
    let conn = connect_ok(
        env.alice_password(),
        Arc::new(TestUi::accept()),
        known_a(env),
    )
    .await;
    let sftp = conn.sftp().await.unwrap();
    let home = sftp.canonicalize(".").await.unwrap();
    assert_eq!(home, "/home/alice");
}
