// SPDX-License-Identifier: GPL-3.0-or-later
//! Interactive shells (PTY) and command execution.

use crate::connection::Connection;
use crate::error::SshError;
use russh::client::Msg;
use russh::{ChannelMsg, ChannelWriteHalf, Sig};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::mpsc;

/// Parameters of [`Connection::open_shell`].
#[derive(Debug, Clone)]
pub struct ShellRequest {
    /// `TERM` value, e.g. `xterm-256color`.
    pub term: String,
    pub cols: u32,
    pub rows: u32,
    /// Environment variables (`SendEnv`); the server must `AcceptEnv` them.
    pub env: Vec<(String, String)>,
    /// Request agent forwarding for this session (keys of the target hop are served).
    pub agent_forwarding: bool,
    /// Run this command in the PTY instead of the login shell.
    pub command: Option<String>,
}

impl Default for ShellRequest {
    fn default() -> Self {
        Self {
            term: core_model::Defaults::TERM_TYPE.to_owned(),
            cols: 80,
            rows: 24,
            env: Vec::new(),
            agent_forwarding: false,
            command: None,
        }
    }
}

/// Events from a shell channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellEvent {
    /// Terminal output.
    Data(Vec<u8>),
    /// Extended data (`ext == 1` is stderr; rare with a PTY).
    ExtendedData {
        ext: u32,
        data: Vec<u8>,
    },
    ExitStatus(u32),
    ExitSignal {
        signal: String,
        core_dumped: bool,
        message: String,
    },
    /// The remote side will send no more data.
    Eof,
    /// The channel is closed; no more events follow.
    Closed,
}

/// Signals that can be sent to the remote process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Int,
    Term,
    Kill,
    Hup,
    Quit,
    Usr1,
}

impl From<Signal> for Sig {
    fn from(s: Signal) -> Self {
        match s {
            Signal::Int => Sig::INT,
            Signal::Term => Sig::TERM,
            Signal::Kill => Sig::KILL,
            Signal::Hup => Sig::HUP,
            Signal::Quit => Sig::QUIT,
            Signal::Usr1 => Sig::USR1,
        }
    }
}

fn sig_name(s: &Sig) -> String {
    match s {
        Sig::ABRT => "ABRT".into(),
        Sig::ALRM => "ALRM".into(),
        Sig::FPE => "FPE".into(),
        Sig::HUP => "HUP".into(),
        Sig::ILL => "ILL".into(),
        Sig::INT => "INT".into(),
        Sig::KILL => "KILL".into(),
        Sig::PIPE => "PIPE".into(),
        Sig::QUIT => "QUIT".into(),
        Sig::SEGV => "SEGV".into(),
        Sig::TERM => "TERM".into(),
        Sig::USR1 => "USR1".into(),
        Sig::Custom(c) => c.clone(),
    }
}

/// Write side of an interactive shell. Dropping it does not close the channel; call
/// [`Self::close`].
pub struct ShellHandle {
    write: ChannelWriteHalf<Msg>,
}

impl std::fmt::Debug for ShellHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShellHandle")
            .field("channel", &self.write.id())
            .finish()
    }
}

impl ShellHandle {
    /// Sends keyboard input.
    pub async fn write(&self, data: &[u8]) -> Result<(), SshError> {
        self.write.data(data).await?;
        Ok(())
    }

    /// Informs the server about a new terminal size.
    pub async fn resize(&self, cols: u32, rows: u32) -> Result<(), SshError> {
        self.write.window_change(cols, rows, 0, 0).await?;
        Ok(())
    }

    /// Sends a signal to the remote process (many servers ignore this).
    pub async fn signal(&self, signal: Signal) -> Result<(), SshError> {
        self.write.signal(signal.into()).await?;
        Ok(())
    }

    /// Sends EOF (end of input).
    pub async fn eof(&self) -> Result<(), SshError> {
        self.write.eof().await?;
        Ok(())
    }

    /// Closes the channel.
    pub async fn close(&self) -> Result<(), SshError> {
        self.write.close().await?;
        Ok(())
    }
}

/// Output of [`Connection::exec`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// Exit status; `None` if the server did not report one (e.g. killed by a signal).
    pub exit_status: Option<u32>,
    pub exit_signal: Option<String>,
}

impl ExecOutput {
    /// `stdout` as (lossy) UTF-8.
    #[must_use]
    pub fn stdout_string(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// Whether the command exited with status 0.
    #[must_use]
    pub fn success(&self) -> bool {
        self.exit_status == Some(0)
    }
}

/// Options of [`Connection::exec_with`].
#[derive(Debug, Clone)]
pub struct ExecOptions {
    /// Maximum combined size of stdout + stderr (default 16 MiB).
    pub max_output: usize,
    /// Data written to the command's stdin (followed by EOF).
    pub stdin: Option<Vec<u8>>,
    /// Overall timeout.
    pub timeout: Option<Duration>,
    pub env: Vec<(String, String)>,
}

impl Default for ExecOptions {
    fn default() -> Self {
        Self {
            max_output: 16 * 1024 * 1024,
            stdin: None,
            timeout: None,
            env: Vec::new(),
        }
    }
}

impl Connection {
    /// Opens an interactive shell with a PTY. Events arrive on the returned receiver; it
    /// yields [`ShellEvent::Closed`] last.
    pub async fn open_shell(
        &self,
        req: ShellRequest,
    ) -> Result<(ShellHandle, mpsc::Receiver<ShellEvent>), SshError> {
        let channel = self.handle().channel_open_session().await?;
        let (mut read, write) = channel.split();
        for (k, v) in &req.env {
            write.set_env(false, k.as_str(), v.as_str()).await?;
        }
        if req.agent_forwarding {
            self.inner
                .shared
                .agent_requested
                .store(true, Ordering::SeqCst);
            write.agent_forward(false).await?;
        }
        write
            .request_pty(false, &req.term, req.cols, req.rows, 0, 0, &[])
            .await?;
        match &req.command {
            Some(cmd) => write.exec(true, cmd.as_bytes()).await?,
            None => write.request_shell(true).await?,
        }
        // Wait for the reply to the shell/exec request, keeping early output.
        let mut early = Vec::new();
        loop {
            match read.wait().await {
                Some(ChannelMsg::Success) => break,
                Some(ChannelMsg::Failure) => {
                    return Err(SshError::Refused(
                        "the server refused to start a shell".into(),
                    ));
                }
                Some(msg) => {
                    let closed = matches!(msg, ChannelMsg::Close);
                    if let Some(e) = to_event(msg) {
                        early.push(e);
                    }
                    if closed {
                        return Err(SshError::Refused("the channel was closed".into()));
                    }
                }
                None => return Err(SshError::Refused("the channel was closed".into())),
            }
        }
        let (tx, rx) = mpsc::channel(1024);
        tokio::spawn(async move {
            for e in early {
                if tx.send(e).await.is_err() {
                    return;
                }
            }
            loop {
                let Some(msg) = read.wait().await else {
                    let _ = tx.send(ShellEvent::Closed).await;
                    return;
                };
                let closed = matches!(msg, ChannelMsg::Close);
                if let Some(e) = to_event(msg)
                    && tx.send(e).await.is_err()
                {
                    return;
                }
                if closed {
                    return;
                }
            }
        });
        Ok((ShellHandle { write }, rx))
    }

    /// Runs a command (no PTY) and collects its output (max 16 MiB).
    pub async fn exec(&self, cmd: &str) -> Result<ExecOutput, SshError> {
        self.exec_with(cmd, ExecOptions::default()).await
    }

    /// Runs a command with options.
    pub async fn exec_with(&self, cmd: &str, opts: ExecOptions) -> Result<ExecOutput, SshError> {
        let fut = self.exec_inner(cmd, &opts);
        match opts.timeout {
            Some(t) => tokio::time::timeout(t, fut)
                .await
                .map_err(|_| SshError::Timeout(t))?,
            None => fut.await,
        }
    }

    async fn exec_inner(&self, cmd: &str, opts: &ExecOptions) -> Result<ExecOutput, SshError> {
        let mut channel = self.handle().channel_open_session().await?;
        for (k, v) in &opts.env {
            channel.set_env(false, k.as_str(), v.as_str()).await?;
        }
        channel.exec(true, cmd.as_bytes()).await?;
        let mut out = ExecOutput::default();
        let mut started = false;
        let mut stdin = opts.stdin.as_deref();
        loop {
            let Some(msg) = channel.wait().await else {
                break;
            };
            match msg {
                ChannelMsg::Success if !started => {
                    started = true;
                    if let Some(data) = stdin.take() {
                        channel.data(data).await?;
                    }
                    if opts.stdin.is_some() {
                        channel.eof().await?;
                    }
                }
                ChannelMsg::Failure if !started => {
                    return Err(SshError::Refused(
                        "the server refused to execute the command".into(),
                    ));
                }
                ChannelMsg::Data { data } => {
                    if out.stdout.len() + out.stderr.len() + data.len() > opts.max_output {
                        let _ = channel.close().await;
                        return Err(SshError::OutputLimit(opts.max_output));
                    }
                    out.stdout.extend_from_slice(&data);
                }
                ChannelMsg::ExtendedData { data, .. } => {
                    if out.stdout.len() + out.stderr.len() + data.len() > opts.max_output {
                        let _ = channel.close().await;
                        return Err(SshError::OutputLimit(opts.max_output));
                    }
                    out.stderr.extend_from_slice(&data);
                }
                ChannelMsg::ExitStatus { exit_status } => out.exit_status = Some(exit_status),
                ChannelMsg::ExitSignal { signal_name, .. } => {
                    out.exit_signal = Some(sig_name(&signal_name));
                }
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        Ok(out)
    }
}

fn to_event(msg: ChannelMsg) -> Option<ShellEvent> {
    Some(match msg {
        ChannelMsg::Data { data } => ShellEvent::Data(data.to_vec()),
        ChannelMsg::ExtendedData { data, ext } => ShellEvent::ExtendedData {
            ext,
            data: data.to_vec(),
        },
        ChannelMsg::ExitStatus { exit_status } => ShellEvent::ExitStatus(exit_status),
        ChannelMsg::ExitSignal {
            signal_name,
            core_dumped,
            error_message,
            ..
        } => ShellEvent::ExitSignal {
            signal: sig_name(&signal_name),
            core_dumped,
            message: error_message,
        },
        ChannelMsg::Eof => ShellEvent::Eof,
        ChannelMsg::Close => ShellEvent::Closed,
        _ => return None,
    })
}
