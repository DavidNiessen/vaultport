// SPDX-License-Identifier: GPL-3.0-or-later
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[cfg_attr(feature = "specta", derive(specta::Type))]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            /// Creates a new, time-ordered (UUIDv7) identifier.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            #[must_use]
            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            #[must_use]
            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s).map(Self)
            }
        }
    };
}

define_id!(
    /// Identifier of a [`crate::Host`].
    HostId
);
define_id!(
    /// Identifier of a [`crate::Group`].
    GroupId
);
define_id!(
    /// Identifier of an [`crate::Identity`].
    IdentityId
);
define_id!(
    /// Identifier of a key in the keychain ([`crate::KeyMeta`]).
    KeyId
);
define_id!(
    /// Identifier of a [`crate::Snippet`].
    SnippetId
);
define_id!(
    /// Identifier of a [`crate::SnippetPackage`].
    PackageId
);
define_id!(
    /// Identifier of a [`crate::PortForwardRule`].
    RuleId
);
define_id!(
    /// Identifier of a [`crate::KnownHost`] entry.
    KnownHostId
);
define_id!(
    /// Identifier of a [`crate::WorkspaceTemplate`].
    WorkspaceId
);
define_id!(
    /// Identifier of a [`crate::CloudIntegration`].
    IntegrationId
);
define_id!(
    /// Identifier of a secret blob stored in the vault (password, private key, token).
    SecretRef
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_roundtrip_through_strings_and_json() {
        let id = HostId::new();
        let s = id.to_string();
        assert_eq!(s.parse::<HostId>().ok(), Some(id));
        let json = serde_json::to_string(&id).ok();
        assert_eq!(json, Some(format!("\"{s}\"")));
    }

    #[test]
    fn ids_are_time_ordered() {
        let a = HostId::new();
        let b = HostId::new();
        assert!(a < b);
    }
}
