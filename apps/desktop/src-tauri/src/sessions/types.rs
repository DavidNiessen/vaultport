// SPDX-License-Identifier: GPL-3.0-or-later
//! IPC types for interactive terminal sessions.

use serde::{Deserialize, Serialize};
use specta::Type;

/// What to connect to.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SessionTarget {
    /// A saved host (protocol and settings come from the vault).
    #[serde(rename_all = "camelCase")]
    Host { host_id: String },
    /// An ad-hoc `user@host:port` connection from the omnibox.
    Quick {
        host: String,
        port: Option<u16>,
        username: Option<String>,
    },
    /// A local shell (desktop only).
    Local,
    /// A serial port.
    #[serde(rename_all = "camelCase")]
    Serial {
        path: String,
        baud_rate: u32,
        data_bits: u8,
        stop_bits: u8,
        parity: String,
        flow_control: String,
    },
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenSessionRequest {
    pub target: SessionTarget,
    pub cols: u16,
    pub rows: u16,
}

/// A keyboard-interactive / password prompt line.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PromptLine {
    pub text: String,
    pub echo: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum HostKeyStatus {
    Unknown,
    Changed,
    Revoked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SessionPhase {
    Connecting,
    Authenticating,
    Connected,
}

/// Events streamed from the core to a terminal tab.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SessionEvent {
    /// Terminal output, base64 encoded (batched).
    Output {
        data: String,
    },
    Status {
        status: SessionPhase,
    },
    Banner {
        message: String,
    },
    #[serde(rename_all = "camelCase")]
    HostKey {
        prompt_id: String,
        host: String,
        port: u16,
        key_type: String,
        fingerprint: String,
        randomart: Option<String>,
        status: HostKeyStatus,
        previous_fingerprints: Vec<String>,
    },
    #[serde(rename_all = "camelCase")]
    Auth {
        prompt_id: String,
        target: String,
        name: String,
        instructions: String,
        prompts: Vec<PromptLine>,
    },
    /// The session ended. `reason` is `"network"` for recoverable network failures.
    Closed {
        reason: Option<String>,
        error: bool,
    },
}

/// The user's answer to a prompt.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PromptResponse {
    HostKey {
        accept: bool,
        remember: bool,
    },
    /// `None` cancels authentication.
    Auth {
        answers: Option<Vec<String>>,
    },
}

impl Drop for PromptResponse {
    fn drop(&mut self) {
        if let Self::Auth {
            answers: Some(answers),
        } = self
        {
            for a in answers.iter_mut() {
                zeroize::Zeroize::zeroize(a);
            }
        }
    }
}
