// SPDX-License-Identifier: GPL-3.0-or-later
//! Mapping between model entities and vault record kinds.

use core_model::{
    CloudIntegration, Group, Host, Identity, KeyMeta, KnownHost, PortForwardRule, Snippet,
    SnippetPackage, Validate, WorkspaceTemplate,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// A type that can be stored as a vault record.
pub trait Entity: Serialize + DeserializeOwned + Clone {
    /// Stable record kind name (part of the on-disk format; never rename).
    const KIND: &'static str;
    /// Whether records of this kind are replicated to other devices.
    const SYNCED: bool = true;

    /// The record id.
    fn record_id(&self) -> String;

    /// Validates the entity before it is written.
    fn check(&self) -> Result<(), core_model::ModelError> {
        Ok(())
    }

    /// Whether this particular record must stay on this device.
    fn device_local(&self) -> bool {
        !Self::SYNCED
    }
}

macro_rules! entity {
    ($ty:ty, $kind:literal, validate) => {
        impl Entity for $ty {
            const KIND: &'static str = $kind;
            fn record_id(&self) -> String {
                self.id.to_string()
            }
            fn check(&self) -> Result<(), core_model::ModelError> {
                self.validate()
            }
        }
    };
    ($ty:ty, $kind:literal) => {
        impl Entity for $ty {
            const KIND: &'static str = $kind;
            fn record_id(&self) -> String {
                self.id.to_string()
            }
        }
    };
}

entity!(Host, "host", validate);
entity!(Group, "group", validate);
entity!(Identity, "identity", validate);
entity!(Snippet, "snippet", validate);
entity!(PortForwardRule, "forward", validate);
entity!(SnippetPackage, "package");
entity!(KnownHost, "knownHost");
entity!(WorkspaceTemplate, "workspace");

impl Entity for KeyMeta {
    const KIND: &'static str = "key";
    fn record_id(&self) -> String {
        self.id.to_string()
    }
    fn device_local(&self) -> bool {
        self.is_device_bound()
    }
}

impl Entity for CloudIntegration {
    const KIND: &'static str = "integration";
    fn record_id(&self) -> String {
        self.id.to_string()
    }
}

/// Kind name used for secret blobs.
pub const SECRET_KIND: &str = "secret";
/// Kind name used for singleton settings documents.
pub const SETTINGS_KIND: &str = "settings";
