// SPDX-License-Identifier: GPL-3.0-or-later
//! Callbacks into the app (prompts, confirmations).

use crate::known_hosts::HostKeyCheck;
use async_trait::async_trait;
use zeroize::Zeroizing;

/// The user's answer to a host key prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyDecision {
    /// Abort the connection.
    Reject,
    /// Trust the key for this connection only.
    AcceptOnce,
    /// Trust the key and persist it through [`crate::KnownHosts::save`].
    AcceptAndSave,
}

/// One `keyboard-interactive` prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub text: String,
    /// Whether the answer may be echoed (false for passwords / OTPs).
    pub echo: bool,
}

/// A signature request made through the forwarded agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSignRequest {
    /// Label of the hop whose session asked (the target host).
    pub hop: String,
    /// `SHA256:` fingerprint of the key that would sign.
    pub key_fingerprint: String,
    /// Key comment / label.
    pub key_comment: String,
}

/// Prompts and notifications, implemented by the app.
///
/// All methods are async; the engine waits for the answer (time spent here does not count
/// against the connect timeout). Every method except [`Self::verify_host_key`] has a
/// conservative default.
#[async_trait]
pub trait Interaction: Send + Sync {
    /// The presented host key is not trusted (unknown or changed). Not called for trusted
    /// keys, and not called under [`core_model::HostKeyPolicy::Strict`]. Called with
    /// [`crate::HostKeyStatus::Revoked`] for information only (the decision is ignored).
    async fn verify_host_key(&self, check: &HostKeyCheck) -> HostKeyDecision;

    /// `keyboard-interactive` round. Return `None` to cancel this method. The number of
    /// answers must equal the number of prompts.
    async fn keyboard_interactive(
        &self,
        _hop: &str,
        _name: &str,
        _instructions: &str,
        _prompts: &[Prompt],
    ) -> Option<Vec<Zeroizing<String>>> {
        None
    }

    /// Ask for a password (none configured, or the configured one was rejected).
    async fn password(&self, _hop: &str, _username: &str) -> Option<Zeroizing<String>> {
        None
    }

    /// Ask for the passphrase of an encrypted private key. `key` is a description (comment
    /// or fingerprint when available). Called again after a wrong passphrase (max 3 times).
    async fn key_passphrase(&self, _hop: &str, _key: &str) -> Option<Zeroizing<String>> {
        None
    }

    /// Pre-authentication banner sent by the server (may contain terminal escapes; the UI
    /// should sanitize before rendering).
    async fn banner(&self, _hop: &str, _message: &str) {}

    /// Confirm one signature made by the forwarded agent (only called when
    /// [`crate::ConnectOptions::agent_confirm`] is set). Default: deny.
    async fn confirm_agent_use(&self, _request: &AgentSignRequest) -> bool {
        false
    }
}
