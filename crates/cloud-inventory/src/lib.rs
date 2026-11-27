// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only cloud inventory clients that call provider APIs directly from the device.
//!
//! There is no backend in between: credentials stay in the local vault and requests go
//! straight to AWS ([`aws`]: EC2 and Lightsail, signed with our own [`sigv4`]),
//! DigitalOcean ([`digitalocean`]) and Azure Resource Manager ([`azure`]). Every client
//! produces a provider-neutral [`Inventory`], which [`sync_group`] reconciles with the hosts
//! of the integration's group.
//!
//! * TLS is rustls only; redirects are disabled and pagination links are only followed on
//!   the configured origin, so tokens and signatures never leave for another host.
//! * Base URLs are injectable (`with_endpoint`, `with_base_url`, `with_endpoints`) for tests,
//!   sovereign clouds and S3-compatible services.
//! * Credential types zeroize on drop and redact themselves in `Debug`.
//! * Any failure aborts a run with an [`InventoryError`], so a partial listing can never make
//!   [`sync_group`] remove hosts.

#![forbid(unsafe_code)]

pub mod aws;
pub mod azure;
pub mod digitalocean;
mod error;
pub mod http;
mod model;
mod reconcile;
pub mod sigv4;

pub use error::InventoryError;
pub use model::{AddressPreference, CloudInstance, Inventory, os_hint};
pub use reconcile::{Diff, MANAGED_STATE_KEY, ManagedFields, ManagedState, sync_group};

use core_model::CloudProvider;

/// Runs the inventory of an integration given its provider, its non-secret settings
/// (`CloudIntegration.settings`) and its credentials secret as stored in the vault (JSON:
/// [`sigv4::Credentials`], [`digitalocean::DigitalOceanCredentials`] or
/// [`azure::AzureCredentials`]).
pub async fn fetch_inventory(
    provider: CloudProvider,
    settings: &serde_json::Map<String, serde_json::Value>,
    credentials: &[u8],
) -> Result<Inventory, InventoryError> {
    fn creds<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, InventoryError> {
        // Do not echo serde's message: it may quote parts of the secret.
        serde_json::from_slice(bytes)
            .map_err(|_| InventoryError::Config("malformed credentials".into()))
    }
    match provider {
        CloudProvider::Aws => {
            aws::AwsInventory::new(creds(credentials)?, aws::AwsSettings::from_map(settings)?)?
                .list()
                .await
        }
        CloudProvider::Digitalocean => {
            digitalocean::DigitalOceanInventory::new(
                creds(credentials)?,
                digitalocean::DigitalOceanSettings::from_map(settings)?,
            )?
            .list()
            .await
        }
        CloudProvider::Azure => {
            azure::AzureInventory::new(
                creds(credentials)?,
                azure::AzureSettings::from_map(settings)?,
            )?
            .list()
            .await
        }
    }
}
