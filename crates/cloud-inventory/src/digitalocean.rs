// SPDX-License-Identifier: GPL-3.0-or-later
//! DigitalOcean inventory: `GET /v2/droplets` with a personal access token.

use crate::error::InventoryError;
use crate::http::{
    Buffered, MAX_PAGES, MAX_RESPONSE_BYTES, base_url, default_client, same_origin, status_error,
};
use crate::model::{
    AddressPreference, CloudInstance, Inventory, os_hint, sanitize_label, sanitize_tag,
};
use serde::{Deserialize, Serialize};
use std::fmt;
use url::Url;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Default API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.digitalocean.com/";

/// A DigitalOcean API token (read scope is sufficient).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct DigitalOceanCredentials {
    pub token: String,
}

impl fmt::Debug for DigitalOceanCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DigitalOceanCredentials { token: <redacted> }")
    }
}

/// Non-secret DigitalOcean settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DigitalOceanSettings {
    #[serde(default)]
    pub address: AddressPreference,
    /// Only list droplets carrying this tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_filter: Option<String>,
    /// Also list droplets that are off (default: active only).
    #[serde(default)]
    pub include_stopped: bool,
}

impl DigitalOceanSettings {
    pub fn from_map(
        map: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<Self, InventoryError> {
        serde_json::from_value(serde_json::Value::Object(map.clone()))
            .map_err(|e| InventoryError::Config(format!("DigitalOcean settings: {e}")))
    }
}

/// Read-only DigitalOcean inventory client.
#[derive(Debug)]
pub struct DigitalOceanInventory {
    http: reqwest::Client,
    credentials: DigitalOceanCredentials,
    settings: DigitalOceanSettings,
    base: Url,
}

impl DigitalOceanInventory {
    pub fn new(
        credentials: DigitalOceanCredentials,
        settings: DigitalOceanSettings,
    ) -> Result<Self, InventoryError> {
        if credentials.token.trim().is_empty() {
            return Err(InventoryError::Config("empty DigitalOcean token".into()));
        }
        Ok(Self {
            http: default_client()?,
            credentials,
            settings,
            base: base_url(DEFAULT_BASE_URL)?,
        })
    }

    /// Overrides the API base URL (tests).
    pub fn with_base_url(mut self, base: &str) -> Result<Self, InventoryError> {
        self.base = base_url(base)?;
        Ok(self)
    }

    /// Lists droplets, following `links.pages.next`.
    pub async fn list(&self) -> Result<Inventory, InventoryError> {
        let mut inv = Inventory::default();
        let mut url = self
            .base
            .join("v2/droplets")
            .map_err(|e| InventoryError::Config(e.to_string()))?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("per_page", "200");
            if let Some(tag) = &self.settings.tag_filter {
                q.append_pair("tag_name", tag);
            }
        }
        let mut seen = std::collections::HashSet::new();
        for _ in 0..MAX_PAGES {
            if !seen.insert(url.to_string()) {
                return Err(InventoryError::Decode("pagination loop".into()));
            }
            let resp = self
                .http
                .get(url.clone())
                .bearer_auth(&self.credentials.token)
                .header("accept", "application/json")
                .send()
                .await?;
            let resp = Buffered::read(resp, MAX_RESPONSE_BYTES).await?;
            if !resp.status.is_success() {
                return Err(error(&resp));
            }
            let page: api::DropletsPage = serde_json::from_slice(&resp.body)
                .map_err(|e| InventoryError::Decode(format!("droplets: {e}")))?;
            for d in page.droplets {
                let wanted = d.status == "active"
                    || (self.settings.include_stopped
                        && matches!(d.status.as_str(), "off" | "new"));
                if wanted {
                    inv.push(droplet(d), self.settings.address);
                }
            }
            let next = page.links.and_then(|l| l.pages).and_then(|p| p.next);
            let Some(next) = next else {
                return Ok(inv);
            };
            let next = Url::parse(&next)
                .map_err(|e| InventoryError::Decode(format!("bad next link: {e}")))?;
            if !same_origin(&next, &self.base) {
                return Err(InventoryError::Decode(
                    "pagination link points to a different origin".into(),
                ));
            }
            url = next;
        }
        Err(InventoryError::Decode("too many pages".into()))
    }
}

fn droplet(d: api::Droplet) -> CloudInstance {
    let mut public_ip = None;
    let mut private_ip = None;
    for n in d.networks.v4 {
        match n.kind.as_str() {
            "public" if public_ip.is_none() => public_ip = Some(n.ip_address),
            "private" if private_ip.is_none() => private_ip = Some(n.ip_address),
            _ => {}
        }
    }
    let image = d.image.unwrap_or_default();
    let os_text = format!(
        "{} {} {}",
        image.distribution.unwrap_or_default(),
        image.slug.unwrap_or_default(),
        image.name.unwrap_or_default()
    );
    let id = d.id.to_string();
    CloudInstance {
        cloud_id: format!("do:droplet:{id}"),
        label: sanitize_label(&[Some(&d.name)], &id),
        address: String::new(),
        public_ip,
        private_ip,
        public_dns: None,
        tags: d.tags.iter().filter_map(|t| sanitize_tag(t)).collect(),
        os: os_hint(&os_text),
        region: d.region.and_then(|r| r.slug),
        username_hint: None,
    }
}

fn error(resp: &Buffered) -> InventoryError {
    #[derive(Deserialize)]
    struct E {
        #[serde(default)]
        id: String,
        #[serde(default)]
        message: String,
    }
    let message = serde_json::from_slice::<E>(&resp.body)
        .map_or_else(|_| resp.snippet(), |e| format!("{}: {}", e.id, e.message));
    status_error(
        resp.status,
        &resp.headers,
        crate::http::truncate(&message, 300),
    )
}

mod api {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    pub struct DropletsPage {
        #[serde(default)]
        pub droplets: Vec<Droplet>,
        #[serde(default)]
        pub links: Option<Links>,
    }

    #[derive(Debug, Deserialize)]
    pub struct Links {
        #[serde(default)]
        pub pages: Option<Pages>,
    }

    #[derive(Debug, Deserialize)]
    pub struct Pages {
        #[serde(default)]
        pub next: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    pub struct Droplet {
        pub id: u64,
        pub name: String,
        #[serde(default)]
        pub status: String,
        #[serde(default)]
        pub tags: Vec<String>,
        #[serde(default)]
        pub networks: Networks,
        #[serde(default)]
        pub region: Option<Region>,
        #[serde(default)]
        pub image: Option<Image>,
    }

    #[derive(Debug, Default, Deserialize)]
    pub struct Networks {
        #[serde(default)]
        pub v4: Vec<Network>,
    }

    #[derive(Debug, Deserialize)]
    pub struct Network {
        pub ip_address: String,
        #[serde(rename = "type", default)]
        pub kind: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct Region {
        #[serde(default)]
        pub slug: Option<String>,
    }

    #[derive(Debug, Default, Deserialize)]
    pub struct Image {
        #[serde(default)]
        pub distribution: Option<String>,
        #[serde(default)]
        pub slug: Option<String>,
        #[serde(default)]
        pub name: Option<String>,
    }
}
