// SPDX-License-Identifier: GPL-3.0-or-later
//! Input validation for user-editable entities.

use crate::entities::{Host, PortForwardRule, Snippet};
use crate::error::ModelError;
use crate::{ForwardKind, Group, Identity};
use std::net::IpAddr;

const MAX_LABEL: usize = 256;
const MAX_HOSTNAME: usize = 253;
const MAX_SCRIPT: usize = 1024 * 1024;

/// Entities that can validate their own invariants before being persisted.
pub trait Validate {
    fn validate(&self) -> Result<(), ModelError>;
}

/// Validates a display label.
pub fn validate_label(label: &str) -> Result<(), ModelError> {
    if label.trim().is_empty() {
        return Err(ModelError::Required { field: "label" });
    }
    if label.chars().count() > MAX_LABEL {
        return Err(ModelError::TooLong {
            field: "label",
            max: MAX_LABEL,
        });
    }
    if label.chars().any(char::is_control) {
        return Err(ModelError::Invalid {
            field: "label",
            reason: "control characters are not allowed".into(),
        });
    }
    Ok(())
}

/// Validates a hostname (RFC 1123, also accepting `_` which is common in internal DNS),
/// an IPv4 address or an IPv6 address (optionally in brackets, optionally with a zone id).
pub fn validate_hostname(host: &str) -> Result<(), ModelError> {
    let h = host.trim();
    if h.is_empty() {
        return Err(ModelError::Required { field: "address" });
    }
    let unbracketed = h
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(h);
    let without_zone = unbracketed.split('%').next().unwrap_or(unbracketed);
    if without_zone.parse::<IpAddr>().is_ok() {
        return Ok(());
    }
    if h.len() > MAX_HOSTNAME {
        return Err(ModelError::InvalidHost(h.to_owned()));
    }
    let ok = h.trim_end_matches('.').split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    });
    if ok {
        Ok(())
    } else {
        Err(ModelError::InvalidHost(h.to_owned()))
    }
}

/// Validates a TCP/UDP port (1-65535).
pub fn validate_port(port: u16) -> Result<(), ModelError> {
    if port == 0 {
        Err(ModelError::InvalidPort("0".into()))
    } else {
        Ok(())
    }
}

fn validate_tags(tags: &[String]) -> Result<(), ModelError> {
    for t in tags {
        if t.trim().is_empty() || t.len() > 64 || t.chars().any(|c| c.is_control() || c == ',') {
            return Err(ModelError::Invalid {
                field: "tags",
                reason: format!("invalid tag {t:?}"),
            });
        }
    }
    Ok(())
}

impl Validate for Host {
    fn validate(&self) -> Result<(), ModelError> {
        validate_label(&self.label)?;
        validate_hostname(&self.address)?;
        validate_tags(&self.tags)?;
        if let Some(p) = self.config.ssh.port {
            validate_port(p)?;
        }
        if let Some(p) = self.config.telnet.port {
            validate_port(p)?;
        }
        if let Some(proxy) = &self.config.ssh.proxy {
            validate_hostname(&proxy.host)?;
            validate_port(proxy.port)?;
        }
        if let Some(env) = &self.config.ssh.env {
            for v in env {
                let valid = !v.name.is_empty()
                    && v.name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_');
                if !valid {
                    return Err(ModelError::Invalid {
                        field: "env",
                        reason: format!("invalid variable name {:?}", v.name),
                    });
                }
            }
        }
        if let Some(chain) = &self.config.ssh.jump_chain
            && chain.contains(&self.id)
        {
            return Err(ModelError::ChainCycle);
        }
        Ok(())
    }
}

impl Validate for Group {
    fn validate(&self) -> Result<(), ModelError> {
        validate_label(&self.label)?;
        if self.parent == Some(self.id) {
            return Err(ModelError::GroupCycle);
        }
        Ok(())
    }
}

impl Validate for Identity {
    fn validate(&self) -> Result<(), ModelError> {
        validate_label(&self.label)?;
        if self.username.trim().is_empty() {
            return Err(ModelError::Required { field: "username" });
        }
        Ok(())
    }
}

impl Validate for Snippet {
    fn validate(&self) -> Result<(), ModelError> {
        validate_label(&self.label)?;
        if self.script.is_empty() {
            return Err(ModelError::Required { field: "script" });
        }
        if self.script.len() > MAX_SCRIPT {
            return Err(ModelError::TooLong {
                field: "script",
                max: MAX_SCRIPT,
            });
        }
        validate_tags(&self.tags)
    }
}

impl Validate for PortForwardRule {
    fn validate(&self) -> Result<(), ModelError> {
        validate_label(&self.label)?;
        validate_hostname(&self.bind_address)?;
        validate_port(self.bind_port)?;
        if self.kind != ForwardKind::Dynamic {
            let dest = self
                .dest_host
                .as_deref()
                .ok_or(ModelError::Required { field: "destHost" })?;
            validate_hostname(dest)?;
            validate_port(
                self.dest_port
                    .ok_or(ModelError::Required { field: "destPort" })?,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostnames() {
        for ok in [
            "example.com",
            "a",
            "my_host.internal",
            "10.0.0.1",
            "::1",
            "[fe80::1%eth0]",
            "fd00::1",
            "example.com.",
        ] {
            assert!(validate_hostname(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "-bad.com",
            "bad-.com",
            "a..b",
            "spaces here",
            "x;rm -rf",
        ] {
            assert!(validate_hostname(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn labels() {
        assert!(validate_label("web").is_ok());
        assert!(validate_label("  ").is_err());
        assert!(validate_label("a\u{7}b").is_err());
        assert!(validate_label(&"x".repeat(300)).is_err());
    }

    #[test]
    fn host_cannot_jump_through_itself() {
        let mut h = Host::new("a", "a.example");
        h.config.ssh.jump_chain = Some(vec![h.id]);
        assert_eq!(h.validate(), Err(ModelError::ChainCycle));
    }
}
