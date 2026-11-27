// SPDX-License-Identifier: GPL-3.0-or-later
//! Sparse, inheritable connection configuration.
//!
//! Every field is optional. A host's effective configuration is computed by layering the host's
//! own values over its parent group's, that group's parent's, and so on, finally falling back to
//! [`Defaults`]. Inheritance is resolved **per parameter**.

use crate::id::{HostId, IdentityId, KeyId, SecretRef, SnippetId};
use serde::{Deserialize, Serialize};

/// Fills unset fields of `self` from `parent`.
pub trait Inherit {
    /// Layer `parent` *under* `self`: any field `self` does not set is taken from `parent`.
    fn inherit_from(&mut self, parent: &Self);
}

impl<T: Clone> Inherit for Option<T> {
    fn inherit_from(&mut self, parent: &Self) {
        if self.is_none() {
            self.clone_from(parent);
        }
    }
}

macro_rules! sparse_struct {
    (
        $(#[$meta:meta])*
        pub struct $name:ident {
            $( $(#[$fmeta:meta])* pub $field:ident : $ty:ty ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
        #[cfg_attr(feature = "specta", derive(specta::Type))]
        #[serde(default, rename_all = "camelCase")]
        pub struct $name {
            $( $(#[$fmeta])* #[serde(skip_serializing_if = "Option::is_none")] pub $field: Option<$ty>, )*
        }

        impl Inherit for $name {
            fn inherit_from(&mut self, parent: &Self) {
                $( self.$field.inherit_from(&parent.$field); )*
            }
        }

        impl $name {
            /// Returns `true` when no field is set.
            #[must_use]
            pub fn is_empty(&self) -> bool {
                true $( && self.$field.is_none() )*
            }
        }
    };
}

/// HTTP / SOCKS proxy kinds. SOCKS4 is intentionally unsupported (no auth, no IPv6, no DNS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum ProxyKind {
    Http,
    Socks5,
}

/// A proxy used to reach the first hop of a connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfig {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<SecretRef>,
}

impl ProxyConfig {
    #[must_use]
    pub const fn default_port(kind: ProxyKind) -> u16 {
        match kind {
            ProxyKind::Http => 3128,
            ProxyKind::Socks5 => 1080,
        }
    }
}

/// An environment variable sent on connect (`SendEnv`). The server must `AcceptEnv` it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

/// Terminal backspace behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Backspace {
    /// Send DEL (0x7f), the xterm default.
    #[default]
    Del,
    /// Send BS (Ctrl+H, 0x08).
    CtrlH,
}

/// How the client authenticates against an SSH server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Credential {
    /// Reuse a keychain [`crate::Identity`] (username + password + key bundle).
    Identity { id: IdentityId },
    /// Inline credentials.
    Inline {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        username: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: Option<SecretRef>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        keys: Vec<KeyId>,
    },
}

/// Host-key policy for a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum HostKeyPolicy {
    /// Ask on first use, reject on change (default).
    #[default]
    Ask,
    /// Only connect to hosts already in Known Hosts.
    Strict,
}

sparse_struct! {
    /// SSH protocol settings.
    pub struct SshConfig {
        pub enabled: bool,
        pub port: u16,
        pub credential: Credential,
        /// Ordered jump hosts, first hop first (ProxyJump).
        pub jump_chain: Vec<HostId>,
        pub proxy: ProxyConfig,
        pub agent_forwarding: bool,
        /// Ask before every agent signature requested through a forwarded agent.
        pub agent_confirm: bool,
        pub env: Vec<EnvVar>,
        pub startup_snippet: SnippetId,
        /// Seconds between keepalive probes (0 disables).
        pub keepalive_secs: u32,
        pub compression: bool,
        /// Allow legacy algorithms (ssh-rsa/SHA-1, CBC, DH group1/14-sha1). Off by default.
        pub legacy_algorithms: bool,
        /// Offer post-quantum key exchange first. On by default.
        pub post_quantum: bool,
        pub host_key_policy: HostKeyPolicy,
        pub x11_forwarding: bool,
        pub connect_timeout_secs: u32,
    }
}

sparse_struct! {
    /// Mosh settings (layered on top of SSH, which is used for bootstrap).
    pub struct MoshConfig {
        pub enabled: bool,
        /// Remote command used to start the server.
        pub server_command: String,
    }
}

sparse_struct! {
    /// Telnet settings. Telnet is plaintext and the UI warns about it.
    pub struct TelnetConfig {
        pub enabled: bool,
        pub port: u16,
        pub username: String,
        pub password: SecretRef,
    }
}

sparse_struct! {
    /// Terminal appearance and behaviour.
    pub struct TerminalPrefs {
        pub theme: String,
        pub font_family: String,
        pub font_size: f32,
        pub backspace: Backspace,
        pub term_type: String,
        pub encoding: String,
        pub scrollback: u32,
        pub cursor_blink: bool,
        pub bright_bold: bool,
    }
}

sparse_struct! {
    /// SFTP behaviour.
    pub struct SftpConfig {
        pub start_path: String,
        pub show_hidden: bool,
        pub follow_symlinks: bool,
    }
}

/// The full sparse configuration carried by hosts and groups.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default, rename_all = "camelCase")]
pub struct HostConfig {
    pub ssh: SshConfig,
    pub mosh: MoshConfig,
    pub telnet: TelnetConfig,
    pub terminal: TerminalPrefs,
    pub sftp: SftpConfig,
}

impl Inherit for HostConfig {
    fn inherit_from(&mut self, parent: &Self) {
        self.ssh.inherit_from(&parent.ssh);
        self.mosh.inherit_from(&parent.mosh);
        self.telnet.inherit_from(&parent.telnet);
        self.terminal.inherit_from(&parent.terminal);
        self.sftp.inherit_from(&parent.sftp);
    }
}

/// Built-in defaults that sit at the bottom of every inheritance chain.
#[derive(Debug, Clone, Copy)]
pub struct Defaults;

impl Defaults {
    pub const SSH_PORT: u16 = 22;
    pub const TELNET_PORT: u16 = 23;
    pub const KEEPALIVE_SECS: u32 = 30;
    pub const CONNECT_TIMEOUT_SECS: u32 = 15;
    pub const MOSH_SERVER: &'static str = "mosh-server new -s -l LANG=en_US.UTF-8";
    pub const TERM_TYPE: &'static str = "xterm-256color";
    pub const THEME: &'static str = "midnight";
    pub const FONT_FAMILY: &'static str = "JetBrains Mono";
    pub const FONT_SIZE: f32 = 14.0;
    pub const SCROLLBACK: u32 = 10_000;

    /// The default configuration applied below all groups.
    #[must_use]
    pub fn config() -> HostConfig {
        HostConfig {
            ssh: SshConfig {
                enabled: Some(true),
                port: Some(Self::SSH_PORT),
                credential: None,
                jump_chain: Some(Vec::new()),
                proxy: None,
                agent_forwarding: Some(false),
                agent_confirm: Some(true),
                env: Some(Vec::new()),
                startup_snippet: None,
                keepalive_secs: Some(Self::KEEPALIVE_SECS),
                compression: Some(false),
                legacy_algorithms: Some(false),
                post_quantum: Some(true),
                host_key_policy: Some(HostKeyPolicy::Ask),
                x11_forwarding: Some(false),
                connect_timeout_secs: Some(Self::CONNECT_TIMEOUT_SECS),
            },
            mosh: MoshConfig {
                enabled: Some(false),
                server_command: Some(Self::MOSH_SERVER.to_owned()),
            },
            telnet: TelnetConfig {
                enabled: Some(false),
                port: Some(Self::TELNET_PORT),
                username: None,
                password: None,
            },
            terminal: TerminalPrefs {
                theme: Some(Self::THEME.to_owned()),
                font_family: Some(Self::FONT_FAMILY.to_owned()),
                font_size: Some(Self::FONT_SIZE),
                backspace: Some(Backspace::Del),
                term_type: Some(Self::TERM_TYPE.to_owned()),
                encoding: Some("utf-8".to_owned()),
                scrollback: Some(Self::SCROLLBACK),
                cursor_blink: Some(true),
                bright_bold: Some(true),
            },
            sftp: SftpConfig {
                start_path: None,
                show_hidden: Some(false),
                follow_symlinks: Some(true),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_values_win_and_gaps_are_filled() {
        let mut child = SshConfig {
            port: Some(2222),
            ..SshConfig::default()
        };
        let parent = SshConfig {
            port: Some(22),
            agent_forwarding: Some(true),
            ..SshConfig::default()
        };
        child.inherit_from(&parent);
        assert_eq!(child.port, Some(2222));
        assert_eq!(child.agent_forwarding, Some(true));
        assert_eq!(child.proxy, None);
    }

    #[test]
    fn empty_config_serializes_compactly() {
        let json = serde_json::to_string(&HostConfig::default()).unwrap_or_default();
        assert_eq!(
            json,
            r#"{"ssh":{},"mosh":{},"telnet":{},"terminal":{},"sftp":{}}"#
        );
        assert!(SshConfig::default().is_empty());
    }

    #[test]
    fn defaults_are_secure() {
        let d = Defaults::config();
        assert_eq!(d.ssh.agent_forwarding, Some(false));
        assert_eq!(d.ssh.legacy_algorithms, Some(false));
        assert_eq!(d.ssh.post_quantum, Some(true));
        assert_eq!(d.ssh.host_key_policy, Some(HostKeyPolicy::Ask));
    }
}
