// SPDX-License-Identifier: GPL-3.0-or-later
//! Azure inventory: OAuth2 client-credentials token from Microsoft Entra ID, then the Azure
//! Resource Manager list APIs for virtual machines, network interfaces and public IPs.
//!
//! Instead of one request per VM, the client lists NICs and public IP addresses once per
//! subscription and follows the resource ids locally (VM → NIC → IP configuration → public
//! IP). VMs without a public IP are skipped with a warning (unless the address preference
//! allows private addresses). Power state is not queried: deallocated VMs normally have no
//! (dynamic) public IP and are therefore skipped.

use crate::error::InventoryError;
use crate::http::{
    Buffered, MAX_PAGES, MAX_RESPONSE_BYTES, base_url, default_client, same_origin, status_error,
    truncate,
};
use crate::model::{AddressPreference, CloudInstance, Inventory, kv_tag, os_hint, sanitize_label};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use url::Url;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

pub const DEFAULT_LOGIN_URL: &str = "https://login.microsoftonline.com/";
pub const DEFAULT_MANAGEMENT_URL: &str = "https://management.azure.com/";
const SCOPE: &str = "https://management.azure.com/.default";
const COMPUTE_API: &str = "2024-07-01";
const NETWORK_API: &str = "2024-05-01";

/// Service principal credentials (app registration with the `Reader` role).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct AzureCredentials {
    pub tenant_id: String,
    pub client_id: String,
    pub client_secret: String,
}

impl fmt::Debug for AzureCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AzureCredentials")
            .field("tenant_id", &self.tenant_id)
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .finish()
    }
}

/// Non-secret Azure settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AzureSettings {
    pub subscriptions: Vec<String>,
    #[serde(default = "default_address")]
    pub address: AddressPreference,
}

fn default_address() -> AddressPreference {
    AddressPreference::Public
}

impl AzureSettings {
    pub fn from_map(
        map: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<Self, InventoryError> {
        serde_json::from_value(serde_json::Value::Object(map.clone()))
            .map_err(|e| InventoryError::Config(format!("Azure settings: {e}")))
    }
}

fn check_id(what: &str, v: &str) -> Result<(), InventoryError> {
    let ok = !v.is_empty()
        && v.len() <= 64
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.');
    if ok {
        Ok(())
    } else {
        Err(InventoryError::Config(format!("invalid Azure {what}")))
    }
}

/// Read-only Azure inventory client.
#[derive(Debug)]
pub struct AzureInventory {
    http: reqwest::Client,
    credentials: AzureCredentials,
    settings: AzureSettings,
    login: Url,
    management: Url,
}

impl AzureInventory {
    pub fn new(
        credentials: AzureCredentials,
        settings: AzureSettings,
    ) -> Result<Self, InventoryError> {
        check_id("tenant id", &credentials.tenant_id)?;
        check_id("client id", &credentials.client_id)?;
        if settings.subscriptions.is_empty() {
            return Err(InventoryError::Config(
                "no Azure subscription configured".into(),
            ));
        }
        for s in &settings.subscriptions {
            check_id("subscription id", s)?;
        }
        Ok(Self {
            http: default_client()?,
            credentials,
            settings,
            login: base_url(DEFAULT_LOGIN_URL)?,
            management: base_url(DEFAULT_MANAGEMENT_URL)?,
        })
    }

    /// Overrides the login and management base URLs (tests, sovereign clouds).
    pub fn with_endpoints(mut self, login: &str, management: &str) -> Result<Self, InventoryError> {
        self.login = base_url(login)?;
        self.management = base_url(management)?;
        Ok(self)
    }

    async fn token(&self) -> Result<Zeroizing<String>, InventoryError> {
        #[derive(Deserialize)]
        struct Token {
            access_token: String,
        }
        let url = self
            .login
            .join(&format!("{}/oauth2/v2.0/token", self.credentials.tenant_id))
            .map_err(|e| InventoryError::Config(e.to_string()))?;
        let form = [
            ("grant_type", "client_credentials"),
            ("client_id", self.credentials.client_id.as_str()),
            ("client_secret", self.credentials.client_secret.as_str()),
            ("scope", SCOPE),
        ];
        let resp = self.http.post(url).form(&form).send().await?;
        let resp = Buffered::read(resp, MAX_RESPONSE_BYTES).await?;
        if !resp.status.is_success() {
            return Err(token_error(&resp));
        }
        let mut t: Token = serde_json::from_slice(&resp.body)
            .map_err(|e| InventoryError::Decode(format!("token response: {e}")))?;
        Ok(Zeroizing::new(std::mem::take(&mut t.access_token)))
    }

    /// Lists all `value` entries of an ARM collection, following `nextLink`.
    async fn list_all<T: DeserializeOwned>(
        &self,
        token: &str,
        path: &str,
        api_version: &str,
    ) -> Result<Vec<T>, InventoryError> {
        #[derive(Deserialize)]
        #[serde(bound(deserialize = "T: DeserializeOwned"))]
        struct Page<T> {
            #[serde(default = "Vec::new")]
            value: Vec<T>,
            #[serde(rename = "nextLink", default)]
            next_link: Option<String>,
        }
        let mut url = self
            .management
            .join(path)
            .map_err(|e| InventoryError::Config(e.to_string()))?;
        url.query_pairs_mut()
            .append_pair("api-version", api_version);
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..MAX_PAGES {
            if !seen.insert(url.to_string()) {
                return Err(InventoryError::Decode("pagination loop".into()));
            }
            let resp = self.http.get(url.clone()).bearer_auth(token).send().await?;
            let resp = Buffered::read(resp, MAX_RESPONSE_BYTES).await?;
            if !resp.status.is_success() {
                return Err(arm_error(&resp));
            }
            let page: Page<T> = serde_json::from_slice(&resp.body)
                .map_err(|e| InventoryError::Decode(format!("{path}: {e}")))?;
            out.extend(page.value);
            let Some(next) = page.next_link.filter(|n| !n.is_empty()) else {
                return Ok(out);
            };
            let next = Url::parse(&next)
                .map_err(|e| InventoryError::Decode(format!("bad nextLink: {e}")))?;
            if !same_origin(&next, &self.management) {
                return Err(InventoryError::Decode(
                    "nextLink points to a different origin".into(),
                ));
            }
            url = next;
        }
        Err(InventoryError::Decode("too many pages".into()))
    }

    /// Lists VMs of all configured subscriptions.
    pub async fn list(&self) -> Result<Inventory, InventoryError> {
        let token = self.token().await?;
        let mut inv = Inventory::default();
        for sub in &self.settings.subscriptions {
            let vms: Vec<api::Vm> = self
                .list_all(
                    &token,
                    &format!("subscriptions/{sub}/providers/Microsoft.Compute/virtualMachines"),
                    COMPUTE_API,
                )
                .await?;
            if vms.is_empty() {
                continue;
            }
            let nics: Vec<api::Nic> = self
                .list_all(
                    &token,
                    &format!("subscriptions/{sub}/providers/Microsoft.Network/networkInterfaces"),
                    NETWORK_API,
                )
                .await?;
            let ips: Vec<api::PublicIp> = self
                .list_all(
                    &token,
                    &format!("subscriptions/{sub}/providers/Microsoft.Network/publicIPAddresses"),
                    NETWORK_API,
                )
                .await?;
            let nics: HashMap<String, api::Nic> = nics
                .into_iter()
                .map(|n| (n.id.to_ascii_lowercase(), n))
                .collect();
            let ips: HashMap<String, api::PublicIp> = ips
                .into_iter()
                .map(|i| (i.id.to_ascii_lowercase(), i))
                .collect();
            for vm in vms {
                inv.push(vm_instance(vm, &nics, &ips), self.settings.address);
            }
        }
        Ok(inv)
    }
}

fn vm_instance(
    vm: api::Vm,
    nics: &HashMap<String, api::Nic>,
    ips: &HashMap<String, api::PublicIp>,
) -> CloudInstance {
    let props = vm.properties.unwrap_or_default();
    let mut refs = props.network_profile.network_interfaces;
    // Primary NIC first.
    refs.sort_by_key(|r| !r.properties.as_ref().is_some_and(|p| p.primary));
    let mut public_ip = None;
    let mut public_dns = None;
    let mut private_ip = None;
    for r in &refs {
        let Some(nic) = nics.get(&r.id.to_ascii_lowercase()) else {
            continue;
        };
        let mut configs: Vec<&api::IpConfiguration> =
            nic.properties.ip_configurations.iter().collect();
        configs.sort_by_key(|c| !c.properties.primary);
        for c in configs {
            if private_ip.is_none() {
                private_ip.clone_from(&c.properties.private_ip_address);
            }
            if public_ip.is_none()
                && let Some(pip) = c
                    .properties
                    .public_ip_address
                    .as_ref()
                    .and_then(|p| ips.get(&p.id.to_ascii_lowercase()))
            {
                public_ip.clone_from(&pip.properties.ip_address);
                public_dns = pip
                    .properties
                    .dns_settings
                    .as_ref()
                    .and_then(|d| d.fqdn.clone());
            }
        }
    }
    let storage = props.storage_profile.unwrap_or_default();
    let image = storage.image_reference.unwrap_or_default();
    let os_text = format!(
        "{} {} {}",
        image.offer.unwrap_or_default(),
        image.publisher.unwrap_or_default(),
        image.sku.unwrap_or_default()
    );
    let mut os = os_hint(&os_text);
    if os == core_model::OsKind::Unknown {
        os = storage
            .os_disk
            .and_then(|d| d.os_type)
            .map_or(core_model::OsKind::Unknown, |t| os_hint(&t));
    }
    let id = props
        .vm_id
        .clone()
        .map_or_else(|| vm.id.to_ascii_lowercase(), |v| v.to_ascii_lowercase());
    let mut tags: Vec<String> = vm.tags.iter().filter_map(|(k, v)| kv_tag(k, v)).collect();
    tags.sort();
    CloudInstance {
        cloud_id: format!("azure:vm:{id}"),
        label: sanitize_label(&[Some(&vm.name)], &id),
        address: String::new(),
        public_ip,
        private_ip,
        public_dns,
        tags,
        os,
        region: vm.location,
        username_hint: props.os_profile.and_then(|o| o.admin_username),
    }
}

fn token_error(resp: &Buffered) -> InventoryError {
    #[derive(Deserialize)]
    struct E {
        #[serde(default)]
        error: String,
        #[serde(default)]
        error_description: String,
    }
    match serde_json::from_slice::<E>(&resp.body) {
        Ok(e) => {
            // Entra ID answers 400/401 for bad client ids, secrets and tenants.
            let message = truncate(&format!("{}: {}", e.error, e.error_description), 300);
            let auth = matches!(
                e.error.as_str(),
                "invalid_client" | "unauthorized_client" | "invalid_grant" | "invalid_request"
            );
            if auth && resp.status.as_u16() != 429 {
                InventoryError::AuthError { message }
            } else {
                status_error(resp.status, &resp.headers, message)
            }
        }
        Err(_) => status_error(resp.status, &resp.headers, resp.snippet()),
    }
}

fn arm_error(resp: &Buffered) -> InventoryError {
    #[derive(Deserialize)]
    struct Outer {
        error: Inner,
    }
    #[derive(Deserialize)]
    struct Inner {
        #[serde(default)]
        code: String,
        #[serde(default)]
        message: String,
    }
    let message = serde_json::from_slice::<Outer>(&resp.body).map_or_else(
        |_| resp.snippet(),
        |o| truncate(&format!("{}: {}", o.error.code, o.error.message), 300),
    );
    status_error(resp.status, &resp.headers, message)
}

mod api {
    use serde::Deserialize;
    use std::collections::BTreeMap;

    #[derive(Debug, Deserialize)]
    pub struct Vm {
        pub id: String,
        pub name: String,
        #[serde(default)]
        pub location: Option<String>,
        #[serde(default)]
        pub tags: BTreeMap<String, String>,
        #[serde(default)]
        pub properties: Option<VmProperties>,
    }

    #[derive(Debug, Default, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct VmProperties {
        #[serde(default)]
        pub vm_id: Option<String>,
        #[serde(default)]
        pub os_profile: Option<OsProfile>,
        #[serde(default)]
        pub storage_profile: Option<StorageProfile>,
        #[serde(default)]
        pub network_profile: NetworkProfile,
    }

    #[derive(Debug, Default, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct OsProfile {
        #[serde(default)]
        pub admin_username: Option<String>,
    }

    #[derive(Debug, Default, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct StorageProfile {
        #[serde(default)]
        pub os_disk: Option<OsDisk>,
        #[serde(default)]
        pub image_reference: Option<ImageReference>,
    }

    #[derive(Debug, Default, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct OsDisk {
        #[serde(default)]
        pub os_type: Option<String>,
    }

    #[derive(Debug, Default, Deserialize)]
    pub struct ImageReference {
        #[serde(default)]
        pub publisher: Option<String>,
        #[serde(default)]
        pub offer: Option<String>,
        #[serde(default)]
        pub sku: Option<String>,
    }

    #[derive(Debug, Default, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct NetworkProfile {
        #[serde(default)]
        pub network_interfaces: Vec<NicRef>,
    }

    #[derive(Debug, Deserialize)]
    pub struct NicRef {
        pub id: String,
        #[serde(default)]
        pub properties: Option<NicRefProperties>,
    }

    #[derive(Debug, Default, Deserialize)]
    pub struct NicRefProperties {
        #[serde(default)]
        pub primary: bool,
    }

    #[derive(Debug, Deserialize)]
    pub struct Nic {
        pub id: String,
        #[serde(default)]
        pub properties: NicProperties,
    }

    #[derive(Debug, Default, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct NicProperties {
        #[serde(default)]
        pub ip_configurations: Vec<IpConfiguration>,
    }

    #[derive(Debug, Deserialize)]
    pub struct IpConfiguration {
        #[serde(default)]
        pub properties: IpConfigurationProperties,
    }

    #[derive(Debug, Default, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct IpConfigurationProperties {
        #[serde(default, rename = "privateIPAddress")]
        pub private_ip_address: Option<String>,
        #[serde(default)]
        pub primary: bool,
        #[serde(default, rename = "publicIPAddress")]
        pub public_ip_address: Option<ResourceRef>,
    }

    #[derive(Debug, Deserialize)]
    pub struct ResourceRef {
        pub id: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct PublicIp {
        pub id: String,
        #[serde(default)]
        pub properties: PublicIpProperties,
    }

    #[derive(Debug, Default, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct PublicIpProperties {
        #[serde(default)]
        pub ip_address: Option<String>,
        #[serde(default)]
        pub dns_settings: Option<DnsSettings>,
    }

    #[derive(Debug, Deserialize)]
    pub struct DnsSettings {
        #[serde(default)]
        pub fqdn: Option<String>,
    }
}
