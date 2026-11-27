// SPDX-License-Identifier: GPL-3.0-or-later
//! Docker-backed test fixture shared by the `integration` test suites of `ssh-engine` and
//! `sftp-engine` (the latter includes this file with `#[path]`).
//!
//! Two containers of `docker/test-servers/sshd` are started on a private network:
//! `a` publishes port 22 on 127.0.0.1, `b` is only reachable through `a` (jump host tests).
//! When docker (or the image) is unavailable, [`env`] returns `None` and tests skip.
//! Containers stop themselves after `SSHD_LIFETIME` seconds and are started with `--rm`; unused
//! networks of earlier runs are pruned on start-up.

#![allow(dead_code, clippy::print_stdout, clippy::missing_panics_doc)]

use async_trait::async_trait;
use ssh_engine::ssh_key::certificate::{Builder, CertType};
use ssh_engine::ssh_key::{Algorithm, PrivateKey};
use ssh_engine::{
    AgentSignRequest, AuthMethod, ConnectOptions, Hop, HostKeyCheck, HostKeyDecision, Interaction,
    Prompt, Zeroizing,
};
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const IMAGE: &str = "ssh-client-test-sshd:1";
pub const ALICE_PASSWORD: &str = "alice-password";
pub const BOB_PASSWORD: &str = "bob-password";

#[derive(Debug)]
pub struct Server {
    pub name: String,
    /// Host and port reachable from the test process (only for `a`).
    pub host: String,
    pub port: u16,
}

pub struct Env {
    pub a: Server,
    /// Reachable only from `a` as `b.name:22`.
    pub b: Server,
    pub user_ca: PrivateKey,
    pub network: String,
}

impl std::fmt::Debug for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Env")
            .field("a", &self.a)
            .field("b", &self.b)
            .field("network", &self.network)
            .finish_non_exhaustive()
    }
}

static ENV: OnceLock<Option<Env>> = OnceLock::new();

/// The shared fixture, or `None` (after printing why) when docker is not usable.
pub fn env() -> Option<&'static Env> {
    let e = ENV.get_or_init(|| match setup() {
        Ok(e) => Some(e),
        Err(why) => {
            println!("SKIPPING docker integration tests: {why}");
            None
        }
    });
    e.as_ref()
}

/// Skips the current test when docker is unavailable.
#[macro_export]
macro_rules! require_env {
    () => {
        match support::env() {
            Some(e) => e,
            None => {
                println!("skipped: docker not available");
                return;
            }
        }
    };
}

fn docker(args: &[&str]) -> Result<String, String> {
    let out = Command::new("docker")
        .args(args)
        .output()
        .map_err(|e| format!("cannot run docker: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        Err(format!(
            "docker {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Runs `sh -c script` in a container, feeding `stdin`.
pub fn docker_exec(container: &str, script: &str, stdin: &[u8]) -> Result<String, String> {
    let mut child = Command::new("docker")
        .args(["exec", "-i", container, "sh", "-c", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(mut si) = child.stdin.take() {
        si.write_all(stdin).map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

fn image_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docker/test-servers/sshd")
}

fn setup() -> Result<Env, String> {
    docker(&["version", "--format", "{{.Server.Version}}"])?;
    let image = std::env::var("SSH_ENGINE_TEST_IMAGE").unwrap_or_else(|_| IMAGE.to_owned());
    if docker(&["image", "inspect", &image]).is_err() {
        let dir = image_dir();
        docker(&["build", "-t", &image, dir.to_str().unwrap_or(".")])?;
    }
    let tag = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or_default()
    );
    let network = format!("sshc-test-{tag}");
    remove_stale_containers();
    // Remove networks left behind by earlier runs (only unused ones are pruned).
    let _ = docker(&[
        "network",
        "prune",
        "-f",
        "--filter",
        "label=ssh-client-test=1",
    ]);
    docker(&[
        "network",
        "create",
        "--label",
        "ssh-client-test=1",
        &network,
    ])?;
    let a = format!("sshc-a-{tag}");
    let b = format!("sshc-b-{tag}");
    for (name, publish) in [(&a, true), (&b, false)] {
        let mut args = vec![
            "run",
            "-d",
            "--rm",
            "--label",
            "ssh-client-test=1",
            "--network",
            &network,
            "--name",
            name,
            "--hostname",
            name,
            "-e",
            "SSHD_LIFETIME=900",
        ];
        if publish {
            args.extend(["-p", "127.0.0.1::22"]);
        }
        args.push(&image);
        docker(&args)?;
    }
    let mapping = docker(&["port", &a, "22/tcp"])?;
    let port: u16 = mapping
        .lines()
        .next()
        .and_then(|l| l.rsplit(':').next())
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| format!("cannot parse port mapping {mapping:?}"))?;
    wait_for_banner("127.0.0.1", port)?;
    for c in [&a, &b] {
        let deadline = Instant::now() + Duration::from_secs(20);
        while docker_exec(
            c,
            "test -s /etc/ssh/ssh_host_ed25519_key-cert.pub && pgrep sshd.pam",
            b"",
        )
        .is_err()
        {
            if Instant::now() > deadline {
                return Err(format!("sshd in {c} did not start"));
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    let user_ca =
        PrivateKey::random(&mut ssh_engine_rng(), Algorithm::Ed25519).map_err(|e| e.to_string())?;
    let ca_line = user_ca
        .public_key()
        .to_openssh()
        .map_err(|e| e.to_string())?;
    for c in [&a, &b] {
        docker_exec(
            c,
            "cat > /etc/ssh/user_ca.pub",
            format!("{ca_line}\n").as_bytes(),
        )?;
    }
    Ok(Env {
        a: Server {
            name: a,
            host: "127.0.0.1".into(),
            port,
        },
        b: Server {
            name: b.clone(),
            host: b,
            port: 22,
        },
        user_ca,
        network,
    })
}

/// Removes containers of earlier test processes that are no longer running (Linux only; elsewhere
/// they stop by themselves after `SSHD_LIFETIME`).
fn remove_stale_containers() {
    let Ok(names) = docker(&[
        "ps",
        "-a",
        "--filter",
        "label=ssh-client-test=1",
        "--format",
        "{{.Names}}",
    ]) else {
        return;
    };
    for name in names.lines() {
        let pid = name.split('-').nth(2).unwrap_or_default();
        if cfg!(target_os = "linux")
            && !pid.is_empty()
            && !std::path::Path::new(&format!("/proc/{pid}")).exists()
        {
            let _ = docker(&["rm", "-f", name]);
        }
    }
}

fn wait_for_banner(host: &str, port: u16) -> Result<(), String> {
    use std::io::Read as _;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(mut s) = std::net::TcpStream::connect((host, port)) {
            let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = [0u8; 7];
            if s.read_exact(&mut buf).is_ok() && &buf == b"SSH-2.0" {
                return Ok(());
            }
        }
        if Instant::now() > deadline {
            return Err("sshd did not come up".into());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

pub fn ssh_engine_rng() -> impl ssh_engine::ssh_key::rand_core::CryptoRng {
    russh::keys::key::safe_rng()
}

impl Env {
    /// Appends a public key line to `user`'s authorized_keys in container `a` and `b`.
    pub fn authorize(&self, user: &str, line: &str) {
        for c in [&self.a.name, &self.b.name] {
            docker_exec(
                c,
                &format!("cat >> /home/{user}/.ssh/authorized_keys"),
                format!("{line}\n").as_bytes(),
            )
            .expect("authorize key");
        }
    }

    /// The ed25519 host public key line of container `a`.
    pub fn host_key_a(&self) -> String {
        docker_exec(&self.a.name, "cat /etc/ssh/ssh_host_ed25519_key.pub", b"")
            .expect("host key")
            .trim()
            .to_owned()
    }

    /// The host CA public key line of container `a`.
    pub fn host_ca_a(&self) -> String {
        docker_exec(&self.a.name, "cat /etc/ssh/host_ca.pub", b"")
            .expect("host ca")
            .trim()
            .to_owned()
    }

    pub fn hop_a(&self, user: &str) -> Hop {
        let mut h = Hop::new(self.a.host.clone(), self.a.port, user);
        h.interactive_fallback = false;
        h
    }

    pub fn hop_b(&self, user: &str) -> Hop {
        let mut h = Hop::new(self.b.host.clone(), self.b.port, user);
        h.interactive_fallback = false;
        h
    }

    /// Options for alice@a with password auth.
    pub fn alice_password(&self) -> ConnectOptions {
        let mut o = ConnectOptions::new(
            self.hop_a("alice")
                .with_auth(AuthMethod::password(ALICE_PASSWORD)),
        );
        o.connect_timeout = Duration::from_secs(20);
        o
    }

    /// Signs an OpenSSH user certificate for `key` with the test user CA.
    pub fn sign_user_cert(&self, key: &PrivateKey, principal: &str) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let mut b = Builder::new_with_random_nonce(
            &mut ssh_engine_rng(),
            key.public_key(),
            now - 300,
            now + 3600,
        )
        .expect("builder");
        b.cert_type(CertType::User).expect("type");
        b.key_id("integration-test").expect("key id");
        b.valid_principal(principal).expect("principal");
        for ext in [
            "permit-pty",
            "permit-port-forwarding",
            "permit-agent-forwarding",
        ] {
            b.extension(ext, "").expect("extension");
        }
        b.sign(&self.user_ca)
            .expect("sign")
            .to_openssh()
            .expect("encode")
    }
}

/// Scriptable [`Interaction`] for tests.
pub struct TestUi {
    pub decision: Mutex<HostKeyDecision>,
    pub host_key_prompts: AtomicUsize,
    pub checks: Mutex<Vec<HostKeyCheck>>,
    pub password: Option<String>,
    pub passphrase: Option<String>,
    pub kbd_answer: Option<String>,
    pub kbd_prompts: AtomicUsize,
    pub agent_confirmations: AtomicUsize,
    pub banners: Mutex<Vec<String>>,
}

impl TestUi {
    pub fn new(decision: HostKeyDecision) -> Self {
        Self {
            decision: Mutex::new(decision),
            host_key_prompts: AtomicUsize::new(0),
            checks: Mutex::new(Vec::new()),
            password: None,
            passphrase: None,
            kbd_answer: None,
            kbd_prompts: AtomicUsize::new(0),
            agent_confirmations: AtomicUsize::new(0),
            banners: Mutex::new(Vec::new()),
        }
    }

    pub fn accept() -> Self {
        Self::new(HostKeyDecision::AcceptOnce)
    }

    pub fn prompts(&self) -> usize {
        self.host_key_prompts.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Interaction for TestUi {
    async fn verify_host_key(&self, check: &HostKeyCheck) -> HostKeyDecision {
        self.host_key_prompts.fetch_add(1, Ordering::SeqCst);
        self.checks.lock().expect("lock").push(check.clone());
        *self.decision.lock().expect("lock")
    }

    async fn keyboard_interactive(
        &self,
        _hop: &str,
        _name: &str,
        _instructions: &str,
        prompts: &[Prompt],
    ) -> Option<Vec<Zeroizing<String>>> {
        self.kbd_prompts.fetch_add(1, Ordering::SeqCst);
        let a = self.kbd_answer.clone()?;
        Some(prompts.iter().map(|_| Zeroizing::new(a.clone())).collect())
    }

    async fn password(&self, _hop: &str, _username: &str) -> Option<Zeroizing<String>> {
        self.password.clone().map(Zeroizing::new)
    }

    async fn key_passphrase(&self, _hop: &str, _key: &str) -> Option<Zeroizing<String>> {
        self.passphrase.clone().map(Zeroizing::new)
    }

    async fn banner(&self, _hop: &str, message: &str) {
        self.banners.lock().expect("lock").push(message.to_owned());
    }

    async fn confirm_agent_use(&self, _request: &AgentSignRequest) -> bool {
        self.agent_confirmations.fetch_add(1, Ordering::SeqCst);
        true
    }
}
