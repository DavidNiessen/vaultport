// SPDX-License-Identifier: GPL-3.0-or-later
//! AWS inventory: EC2 `DescribeInstances` (Query API, XML) and Lightsail `GetInstances`
//! (JSON 1.1 API), signed with [`crate::sigv4`].

use crate::error::InventoryError;
use crate::http::{
    Buffered, MAX_PAGES, MAX_RESPONSE_BYTES, default_client, status_error, truncate,
};
use crate::model::{AddressPreference, CloudInstance, Inventory, kv_tag, os_hint, sanitize_label};
use crate::sigv4::{self, Credentials, SigningParams};
use core_model::OsKind;
use serde::{Deserialize, Serialize};
use url::Url;

/// AWS services that can be inventoried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AwsService {
    Ec2,
    Lightsail,
}

/// Non-secret AWS integration settings (stored in `CloudIntegration.settings`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AwsSettings {
    /// Regions to query, e.g. `["eu-central-1", "us-east-1"]`.
    #[serde(deserialize_with = "crate::model::string_list")]
    pub regions: Vec<String>,
    /// Services to query (default: EC2 only).
    #[serde(default = "default_services")]
    pub services: Vec<AwsService>,
    #[serde(default)]
    pub address: AddressPreference,
    /// Also list stopped/stopping/pending instances (default: running only). Including them
    /// keeps user edits on hosts that are temporarily stopped.
    #[serde(default)]
    pub include_stopped: bool,
}

fn default_services() -> Vec<AwsService> {
    vec![AwsService::Ec2]
}

impl AwsSettings {
    /// Parses settings from an integration's settings map.
    pub fn from_map(
        map: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<Self, InventoryError> {
        serde_json::from_value(serde_json::Value::Object(map.clone()))
            .map_err(|e| InventoryError::Config(format!("AWS settings: {e}")))
    }
}

fn check_region(region: &str) -> Result<(), InventoryError> {
    let ok = !region.is_empty()
        && region.len() <= 32
        && region
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if ok {
        Ok(())
    } else {
        Err(InventoryError::Config(format!(
            "invalid AWS region {region:?}"
        )))
    }
}

/// Read-only AWS inventory client.
#[derive(Debug)]
pub struct AwsInventory {
    http: reqwest::Client,
    credentials: Credentials,
    settings: AwsSettings,
    endpoint: Option<Url>,
}

impl AwsInventory {
    pub fn new(credentials: Credentials, settings: AwsSettings) -> Result<Self, InventoryError> {
        for r in &settings.regions {
            check_region(r)?;
        }
        if settings.regions.is_empty() {
            return Err(InventoryError::Config("no AWS region configured".into()));
        }
        Ok(Self {
            http: default_client()?,
            credentials,
            settings,
            endpoint: None,
        })
    }

    /// Sends every request to `endpoint` instead of the regional AWS endpoints (tests,
    /// LocalStack, VPC endpoints). Requests are still signed for the configured region.
    pub fn with_endpoint(mut self, endpoint: &str) -> Result<Self, InventoryError> {
        self.endpoint = Some(crate::http::base_url(endpoint)?);
        Ok(self)
    }

    fn endpoint_for(&self, service: &str, region: &str) -> Result<Url, InventoryError> {
        match &self.endpoint {
            Some(u) => Ok(u.clone()),
            None => Url::parse(&format!("https://{service}.{region}.amazonaws.com/"))
                .map_err(|e| InventoryError::Config(e.to_string())),
        }
    }

    /// Lists instances across all configured regions and services. Any failure aborts the
    /// whole run, so a partial result can never cause hosts to be removed.
    pub async fn list(&self) -> Result<Inventory, InventoryError> {
        let mut inv = Inventory::default();
        for region in &self.settings.regions {
            for service in &self.settings.services {
                match service {
                    AwsService::Ec2 => self.ec2(region, &mut inv).await?,
                    AwsService::Lightsail => self.lightsail(region, &mut inv).await?,
                }
            }
        }
        Ok(inv)
    }

    async fn send(
        &self,
        method: reqwest::Method,
        url: Url,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
        region: &str,
        service: &str,
    ) -> Result<Buffered, InventoryError> {
        let params = SigningParams::service(&self.credentials, region, service);
        let added = sigv4::sign(
            method.as_str(),
            &url,
            &headers,
            &sigv4::sha256_hex(&body),
            &params,
        )?;
        let mut req = self.http.request(method, url);
        for (k, v) in headers.iter().chain(added.iter()) {
            req = req.header(k, v);
        }
        let resp = req.body(body).send().await?;
        Buffered::read(resp, MAX_RESPONSE_BYTES).await
    }

    // ----- EC2 ------------------------------------------------------------------------------

    async fn ec2(&self, region: &str, inv: &mut Inventory) -> Result<(), InventoryError> {
        let base = self.endpoint_for("ec2", region)?;
        let mut token: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut pairs: Vec<(&str, &str)> = vec![
                ("Action", "DescribeInstances"),
                ("Version", "2016-11-15"),
                ("Filter.1.Name", "instance-state-name"),
                ("Filter.1.Value.1", "running"),
                ("MaxResults", "1000"),
            ];
            if self.settings.include_stopped {
                pairs.extend([
                    ("Filter.1.Value.2", "pending"),
                    ("Filter.1.Value.3", "stopping"),
                    ("Filter.1.Value.4", "stopped"),
                ]);
            }
            if let Some(t) = &token {
                pairs.push(("NextToken", t));
            }
            let mut url = base.clone();
            url.set_query(Some(&sigv4::encode_query(&pairs)));
            let resp = self
                .send(
                    reqwest::Method::GET,
                    url,
                    Vec::new(),
                    Vec::new(),
                    region,
                    "ec2",
                )
                .await?;
            if !resp.status.is_success() {
                return Err(ec2_error(&resp));
            }
            let text = String::from_utf8_lossy(&resp.body);
            let page: ec2::DescribeInstancesResponse = quick_xml::de::from_str(&text)
                .map_err(|e| InventoryError::Decode(format!("EC2 response: {e}")))?;
            for reservation in page.reservation_set.items {
                for i in reservation.instances_set.items {
                    inv.push(ec2_instance(region, i), self.settings.address);
                }
            }
            match page.next_token.filter(|t| !t.is_empty()) {
                Some(t) if token.as_deref() != Some(t.as_str()) => token = Some(t),
                Some(_) => {
                    return Err(InventoryError::Decode("EC2 pagination loop".into()));
                }
                None => return Ok(()),
            }
        }
        Err(InventoryError::Decode("too many EC2 pages".into()))
    }

    // ----- Lightsail ------------------------------------------------------------------------

    async fn lightsail(&self, region: &str, inv: &mut Inventory) -> Result<(), InventoryError> {
        let url = self.endpoint_for("lightsail", region)?;
        let mut token: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let body = match &token {
                Some(t) => serde_json::json!({ "pageToken": t }),
                None => serde_json::json!({}),
            };
            let body =
                serde_json::to_vec(&body).map_err(|e| InventoryError::Decode(e.to_string()))?;
            let headers = vec![
                (
                    "content-type".to_owned(),
                    "application/x-amz-json-1.1".to_owned(),
                ),
                (
                    "x-amz-target".to_owned(),
                    "Lightsail_20161128.GetInstances".to_owned(),
                ),
            ];
            let resp = self
                .send(
                    reqwest::Method::POST,
                    url.clone(),
                    headers,
                    body,
                    region,
                    "lightsail",
                )
                .await?;
            if !resp.status.is_success() {
                return Err(json_error(&resp));
            }
            let page: lightsail::GetInstancesResponse = serde_json::from_slice(&resp.body)
                .map_err(|e| InventoryError::Decode(format!("Lightsail response: {e}")))?;
            for i in page.instances {
                let state = i
                    .state
                    .as_ref()
                    .map(|s| s.name.as_str())
                    .unwrap_or_default();
                let wanted = state == "running"
                    || (self.settings.include_stopped
                        && matches!(state, "pending" | "stopping" | "stopped"));
                if wanted {
                    inv.push(lightsail_instance(region, i), self.settings.address);
                }
            }
            match page.next_page_token.filter(|t| !t.is_empty()) {
                Some(t) if token.as_deref() != Some(t.as_str()) => token = Some(t),
                Some(_) => {
                    return Err(InventoryError::Decode("Lightsail pagination loop".into()));
                }
                None => return Ok(()),
            }
        }
        Err(InventoryError::Decode("too many Lightsail pages".into()))
    }
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

fn ec2_instance(region: &str, i: ec2::Instance) -> CloudInstance {
    let name = i
        .tag_set
        .items
        .iter()
        .find(|t| t.key == "Name")
        .map(|t| t.value.clone());
    let public_dns = non_empty(i.dns_name);
    let label = sanitize_label(&[name.as_deref(), public_dns.as_deref()], &i.instance_id);
    let os = match i.platform.as_deref() {
        Some(p) if p.eq_ignore_ascii_case("windows") => OsKind::Windows,
        _ => i
            .platform_details
            .as_deref()
            .map_or(OsKind::Unknown, os_hint),
    };
    CloudInstance {
        cloud_id: format!("aws:ec2:{region}:{}", i.instance_id),
        label,
        address: String::new(),
        public_ip: non_empty(i.ip_address),
        private_ip: non_empty(i.private_ip_address),
        public_dns,
        tags: i
            .tag_set
            .items
            .iter()
            .filter(|t| t.key != "Name" && !t.key.starts_with("aws:"))
            .filter_map(|t| kv_tag(&t.key, &t.value))
            .collect(),
        os,
        region: Some(region.to_owned()),
        username_hint: None,
    }
}

fn lightsail_instance(region: &str, i: lightsail::Instance) -> CloudInstance {
    let os_text = format!(
        "{} {}",
        i.blueprint_id.as_deref().unwrap_or_default(),
        i.blueprint_name.as_deref().unwrap_or_default()
    );
    CloudInstance {
        cloud_id: format!("aws:lightsail:{region}:{}", i.name),
        label: sanitize_label(&[Some(&i.name)], &i.name),
        address: String::new(),
        public_ip: non_empty(i.public_ip_address),
        private_ip: non_empty(i.private_ip_address),
        public_dns: None,
        tags: i
            .tags
            .iter()
            .filter_map(|t| kv_tag(&t.key, t.value.as_deref().unwrap_or_default()))
            .collect(),
        os: os_hint(&os_text),
        region: Some(region.to_owned()),
        username_hint: non_empty(i.username),
    }
}

/// Maps an EC2 (Query API) error response.
fn ec2_error(resp: &Buffered) -> InventoryError {
    let text = String::from_utf8_lossy(&resp.body);
    let parsed: Option<ec2::ErrorResponse> = quick_xml::de::from_str(&text).ok();
    let (code, message) = parsed
        .and_then(|r| r.errors.error.into_iter().next())
        .map_or_else(
            || (String::new(), resp.snippet()),
            |e| (e.code, e.message.unwrap_or_default()),
        );
    classify_aws(resp, &code, message)
}

/// Maps a JSON-protocol (`__type`) error response.
fn json_error(resp: &Buffered) -> InventoryError {
    #[derive(Deserialize)]
    struct E {
        #[serde(rename = "__type", default)]
        kind: String,
        #[serde(default, alias = "Message")]
        message: Option<String>,
    }
    let parsed: Option<E> = serde_json::from_slice(&resp.body).ok();
    let (code, message) = parsed.map_or_else(
        || (String::new(), resp.snippet()),
        |e| {
            // `__type` may be `com.amazonaws...#AccessDeniedException`.
            let code = e.kind.rsplit('#').next().unwrap_or_default().to_owned();
            (code, e.message.unwrap_or_default())
        },
    );
    classify_aws(resp, &code, message)
}

fn classify_aws(resp: &Buffered, code: &str, message: String) -> InventoryError {
    const AUTH: &[&str] = &[
        "AuthFailure",
        "UnauthorizedOperation",
        "UnauthorizedAccess",
        "AccessDeniedException",
        "AccessDenied",
        "UnrecognizedClientException",
        "InvalidSignatureException",
        "SignatureDoesNotMatch",
        "InvalidClientTokenId",
        "ExpiredToken",
        "ExpiredTokenException",
        "OptInRequired",
    ];
    const THROTTLE: &[&str] = &[
        "RequestLimitExceeded",
        "Throttling",
        "ThrottlingException",
        "TooManyRequestsException",
    ];
    let message = truncate(
        &if code.is_empty() {
            message
        } else {
            format!("{code}: {message}")
        },
        300,
    );
    let code = code.rsplit('.').next().unwrap_or(code);
    if AUTH.contains(&code) {
        InventoryError::AuthError { message }
    } else if THROTTLE.contains(&code) {
        InventoryError::RateLimited {
            retry_after: crate::http::retry_after(&resp.headers),
        }
    } else {
        status_error(resp.status, &resp.headers, message)
    }
}

/// XML shapes of the EC2 Query API.
mod ec2 {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    #[serde(bound(deserialize = "T: Deserialize<'de>"))]
    pub struct ItemList<T> {
        #[serde(default, rename = "item")]
        pub items: Vec<T>,
    }

    impl<T> Default for ItemList<T> {
        fn default() -> Self {
            Self { items: Vec::new() }
        }
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DescribeInstancesResponse {
        #[serde(default)]
        pub reservation_set: ItemList<Reservation>,
        #[serde(default)]
        pub next_token: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Reservation {
        #[serde(default)]
        pub instances_set: ItemList<Instance>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[allow(clippy::struct_field_names)]
    pub struct Instance {
        pub instance_id: String,
        #[serde(default)]
        pub dns_name: Option<String>,
        #[serde(default)]
        pub ip_address: Option<String>,
        #[serde(default)]
        pub private_ip_address: Option<String>,
        #[serde(default)]
        pub platform: Option<String>,
        #[serde(default)]
        pub platform_details: Option<String>,
        #[serde(default)]
        pub tag_set: ItemList<Tag>,
    }

    #[derive(Debug, Deserialize)]
    pub struct Tag {
        pub key: String,
        #[serde(default)]
        pub value: String,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct ErrorResponse {
        pub errors: Errors,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct Errors {
        #[serde(default)]
        pub error: Vec<ErrorItem>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct ErrorItem {
        pub code: String,
        #[serde(default)]
        pub message: Option<String>,
    }
}

/// JSON shapes of the Lightsail API.
mod lightsail {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct GetInstancesResponse {
        #[serde(default)]
        pub instances: Vec<Instance>,
        #[serde(default)]
        pub next_page_token: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Instance {
        pub name: String,
        #[serde(default)]
        pub public_ip_address: Option<String>,
        #[serde(default)]
        pub private_ip_address: Option<String>,
        #[serde(default)]
        pub blueprint_id: Option<String>,
        #[serde(default)]
        pub blueprint_name: Option<String>,
        #[serde(default)]
        pub username: Option<String>,
        #[serde(default)]
        pub state: Option<State>,
        #[serde(default)]
        pub tags: Vec<Tag>,
    }

    #[derive(Debug, Deserialize)]
    pub struct State {
        #[serde(default)]
        pub name: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct Tag {
        pub key: String,
        #[serde(default)]
        pub value: Option<String>,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn settings_accept_comma_separated_regions() {
        // The desktop app stores every settings value as a JSON string.
        let map = serde_json::json!({ "regions": "us-east-1, eu-central-1" });
        let s = AwsSettings::from_map(map.as_object().unwrap()).unwrap();
        assert_eq!(s.regions, ["us-east-1", "eu-central-1"]);
        assert_eq!(s.services, [AwsService::Ec2]);
    }
}
