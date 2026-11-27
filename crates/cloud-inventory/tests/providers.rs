// SPDX-License-Identifier: GPL-3.0-or-later
//! Provider clients against a local mock HTTP server with canned responses.

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used)]

use cloud_inventory::aws::{AwsInventory, AwsService, AwsSettings};
use cloud_inventory::azure::{AzureCredentials, AzureInventory, AzureSettings};
use cloud_inventory::digitalocean::{
    DigitalOceanCredentials, DigitalOceanInventory, DigitalOceanSettings,
};
use cloud_inventory::sigv4::Credentials;
use cloud_inventory::{AddressPreference, InventoryError};
use core_model::OsKind;
use std::time::Duration;
use wiremock::matchers::{
    body_string_contains, header, header_exists, header_regex, method, path, query_param,
    query_param_is_missing,
};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ----- AWS ------------------------------------------------------------------------------

fn aws(server: &MockServer, services: Vec<AwsService>) -> AwsInventory {
    AwsInventory::new(
        Credentials::new("AKIDEXAMPLE", "secret").with_session_token("tok"),
        AwsSettings {
            regions: vec!["eu-central-1".into()],
            services,
            address: AddressPreference::PublicThenPrivate,
            include_stopped: false,
        },
    )
    .unwrap()
    .with_endpoint(&server.uri())
    .unwrap()
}

const EC2_PAGE_1: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<DescribeInstancesResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
  <requestId>1</requestId>
  <reservationSet>
    <item>
      <reservationId>r-1</reservationId>
      <instancesSet>
        <item>
          <instanceId>i-aaa</instanceId>
          <instanceState><code>16</code><name>running</name></instanceState>
          <privateDnsName>ip-10-0-0-1.ec2.internal</privateDnsName>
          <dnsName>ec2-54-1-1-1.compute-1.amazonaws.com</dnsName>
          <ipAddress>54.1.1.1</ipAddress>
          <privateIpAddress>10.0.0.1</privateIpAddress>
          <platformDetails>Linux/UNIX</platformDetails>
          <tagSet>
            <item><key>Name</key><value>web-1</value></item>
            <item><key>env</key><value>prod</value></item>
            <item><key>aws:cloudformation:stack-name</key><value>x</value></item>
          </tagSet>
        </item>
        <item>
          <instanceId>i-bbb</instanceId>
          <privateIpAddress>10.0.0.2</privateIpAddress>
          <dnsName/>
          <platform>windows</platform>
          <platformDetails>Windows</platformDetails>
        </item>
      </instancesSet>
    </item>
  </reservationSet>
  <nextToken>page/2+token</nextToken>
</DescribeInstancesResponse>"#;

const EC2_PAGE_2: &str = r#"<DescribeInstancesResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
  <reservationSet>
    <item>
      <instancesSet>
        <item>
          <instanceId>i-ccc</instanceId>
          <dnsName>ec2-3-3-3-3.compute-1.amazonaws.com</dnsName>
          <ipAddress>3.3.3.3</ipAddress>
          <platformDetails>Red Hat Enterprise Linux</platformDetails>
        </item>
        <item>
          <instanceId>i-ddd</instanceId>
        </item>
      </instancesSet>
    </item>
  </reservationSet>
</DescribeInstancesResponse>"#;

#[tokio::test]
async fn ec2_pagination_and_mapping() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(query_param("Action", "DescribeInstances"))
        .and(query_param("Filter.1.Value.1", "running"))
        .and(query_param_is_missing("NextToken"))
        .and(header_regex(
            "authorization",
            r"^AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/\d{8}/eu-central-1/ec2/aws4_request, SignedHeaders=host;x-amz-date;x-amz-security-token, Signature=[0-9a-f]{64}$",
        ))
        .and(header("x-amz-security-token", "tok"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EC2_PAGE_1))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(query_param("NextToken", "page/2+token"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EC2_PAGE_2))
        .expect(1)
        .mount(&server)
        .await;

    let inv = aws(&server, vec![AwsService::Ec2]).list().await.unwrap();
    assert_eq!(inv.instances.len(), 3, "{inv:#?}");
    let a = &inv.instances[0];
    assert_eq!(a.cloud_id, "aws:ec2:eu-central-1:i-aaa");
    assert_eq!(a.label, "web-1");
    assert_eq!(a.address, "54.1.1.1");
    assert_eq!(a.private_ip.as_deref(), Some("10.0.0.1"));
    assert_eq!(a.tags, vec!["env:prod".to_owned()]);
    assert_eq!(a.os, OsKind::Linux);
    let b = &inv.instances[1];
    assert_eq!(b.label, "i-bbb", "no name tag, no DNS name → instance id");
    assert_eq!(b.address, "10.0.0.2", "falls back to the private IP");
    assert_eq!(b.os, OsKind::Windows);
    let c = &inv.instances[2];
    assert_eq!(c.label, "ec2-3-3-3-3.compute-1.amazonaws.com");
    assert_eq!(c.os, OsKind::Rhel);
    assert_eq!(inv.warnings.len(), 1, "i-ddd has no address");
}

#[tokio::test]
async fn ec2_error_mapping() {
    let server = MockServer::start().await;
    let err = |code: &str| {
        format!(
            "<Response><Errors><Error><Code>{code}</Code><Message>nope</Message></Error></Errors><RequestID>x</RequestID></Response>"
        )
    };
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401).set_body_string(err("AuthFailure")))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(503)
                .set_body_string(err("RequestLimitExceeded"))
                .insert_header("retry-after", "3"),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;

    let client = aws(&server, vec![AwsService::Ec2]);
    match client.list().await {
        Err(InventoryError::AuthError { message }) => assert!(message.contains("AuthFailure")),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        client.list().await,
        Err(InventoryError::RateLimited {
            retry_after: Some(Duration::from_secs(3))
        })
    );
    assert!(matches!(
        client.list().await,
        Err(InventoryError::Http { status: 500, .. })
    ));
}

#[tokio::test]
async fn lightsail_pagination_and_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(header("x-amz-target", "Lightsail_20161128.GetInstances"))
        .and(header("content-type", "application/x-amz-json-1.1"))
        .and(header_exists("authorization"))
        .and(body_string_contains("pageToken"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "instances": [{
                "name": "blog", "publicIpAddress": "5.5.5.5", "privateIpAddress": "172.26.0.5",
                "blueprintId": "ubuntu_22_04", "username": "ubuntu",
                "state": {"name": "running"}, "tags": [{"key": "team", "value": "web"}, {"key": "solo"}]
            }]
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "instances": [
                {"name": "stopped", "publicIpAddress": "6.6.6.6", "state": {"name": "stopped"}},
                {"name": "app", "privateIpAddress": "172.26.0.7", "blueprintId": "amazon_linux_2023", "state": {"name": "running"}}
            ],
            "nextPageToken": "p2"
        })))
        .mount(&server)
        .await;

    let inv = aws(&server, vec![AwsService::Lightsail])
        .list()
        .await
        .unwrap();
    let names: Vec<&str> = inv.instances.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(names, vec!["app", "blog"]);
    let blog = &inv.instances[1];
    assert_eq!(blog.cloud_id, "aws:lightsail:eu-central-1:blog");
    assert_eq!(blog.os, OsKind::Ubuntu);
    assert_eq!(blog.username_hint.as_deref(), Some("ubuntu"));
    assert_eq!(blog.tags, vec!["team:web".to_owned(), "solo".into()]);
    assert_eq!(inv.instances[0].os, OsKind::Amazon);

    // JSON-protocol errors are mapped by `__type`, even with HTTP 400.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "__type": "UnrecognizedClientException", "message": "bad token"
        })))
        .mount(&server)
        .await;
    assert!(matches!(
        aws(&server, vec![AwsService::Lightsail]).list().await,
        Err(InventoryError::AuthError { .. })
    ));
}

// ----- DigitalOcean -----------------------------------------------------------------------

fn droplet(id: u64, name: &str, public: Option<&str>, status: &str) -> serde_json::Value {
    let mut v4 = vec![serde_json::json!({"ip_address": "10.10.0.1", "type": "private"})];
    if let Some(p) = public {
        v4.push(serde_json::json!({"ip_address": p, "type": "public"}));
    }
    serde_json::json!({
        "id": id, "name": name, "status": status, "tags": ["prod", "web"],
        "networks": {"v4": v4, "v6": []},
        "region": {"slug": "fra1"},
        "image": {"distribution": "Debian", "slug": "debian-12-x64"}
    })
}

#[tokio::test]
async fn digitalocean_pagination_and_mapping() {
    let server = MockServer::start().await;
    let next = format!("{}/v2/droplets?page=2&per_page=200", server.uri());
    Mock::given(method("GET"))
        .and(path("/v2/droplets"))
        .and(query_param_is_missing("page"))
        .and(query_param("per_page", "200"))
        .and(header("authorization", "Bearer do-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "droplets": [droplet(1, "web-1", Some("1.2.3.4"), "active"), droplet(2, "off", Some("1.2.3.5"), "off")],
            "links": {"pages": {"next": next}},
            "meta": {"total": 3}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/droplets"))
        .and(query_param("page", "2"))
        .and(header("authorization", "Bearer do-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "droplets": [droplet(3, "worker", None, "active")],
            "links": {},
            "meta": {"total": 3}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = DigitalOceanInventory::new(
        DigitalOceanCredentials {
            token: "do-token".into(),
        },
        DigitalOceanSettings::default(),
    )
    .unwrap()
    .with_base_url(&server.uri())
    .unwrap();
    let inv = client.list().await.unwrap();
    assert_eq!(inv.instances.len(), 2);
    assert_eq!(inv.instances[0].cloud_id, "do:droplet:1");
    assert_eq!(inv.instances[0].address, "1.2.3.4");
    assert_eq!(inv.instances[0].tags, vec!["prod".to_owned(), "web".into()]);
    assert_eq!(inv.instances[0].os, OsKind::Debian);
    assert_eq!(inv.instances[0].region.as_deref(), Some("fra1"));
    assert_eq!(inv.instances[1].address, "10.10.0.1", "private fallback");
}

#[tokio::test]
async fn digitalocean_errors_and_foreign_links() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401).set_body_json(
            serde_json::json!({"id": "Unauthorized", "message": "Unable to authenticate you"}),
        ))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "60"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "droplets": [],
            "links": {"pages": {"next": "https://evil.example/v2/droplets?page=2"}}
        })))
        .mount(&server)
        .await;
    let client = DigitalOceanInventory::new(
        DigitalOceanCredentials { token: "t".into() },
        DigitalOceanSettings::default(),
    )
    .unwrap()
    .with_base_url(&server.uri())
    .unwrap();
    assert!(matches!(
        client.list().await,
        Err(InventoryError::AuthError { .. })
    ));
    assert_eq!(
        client.list().await,
        Err(InventoryError::RateLimited {
            retry_after: Some(Duration::from_secs(60))
        })
    );
    assert!(matches!(
        client.list().await,
        Err(InventoryError::Decode(_))
    ));
    let dbg = format!("{client:?}");
    assert!(!dbg.contains("token: \"t\""), "{dbg}");
}

// ----- Azure ------------------------------------------------------------------------------

const SUB: &str = "00000000-0000-0000-0000-000000000001";

fn azure(server: &MockServer) -> AzureInventory {
    AzureInventory::new(
        AzureCredentials {
            tenant_id: "tenant-1".into(),
            client_id: "client-1".into(),
            client_secret: "s3cr3t".into(),
        },
        AzureSettings {
            subscriptions: vec![SUB.into()],
            address: AddressPreference::Public,
        },
    )
    .unwrap()
    .with_endpoints(
        &format!("{}/login/", server.uri()),
        &format!("{}/arm/", server.uri()),
    )
    .unwrap()
}

fn nic_id(n: &str) -> String {
    format!(
        "/subscriptions/{SUB}/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/{n}"
    )
}

fn pip_id(n: &str) -> String {
    format!(
        "/subscriptions/{SUB}/resourceGroups/rg/providers/Microsoft.Network/publicIPAddresses/{n}"
    )
}

fn vm(name: &str, nic: &str) -> serde_json::Value {
    serde_json::json!({
        "id": format!("/subscriptions/{SUB}/resourceGroups/rg/providers/Microsoft.Compute/virtualMachines/{name}"),
        "name": name,
        "location": "westeurope",
        "tags": {"env": "prod"},
        "properties": {
            "vmId": format!("vm-{name}"),
            "osProfile": {"adminUsername": "azureuser"},
            "storageProfile": {
                "osDisk": {"osType": "Linux"},
                "imageReference": {"publisher": "Canonical", "offer": "0001-com-ubuntu-server-jammy", "sku": "22_04-lts"}
            },
            "networkProfile": {"networkInterfaces": [{"id": nic_id(nic), "properties": {"primary": true}}]}
        }
    })
}

#[tokio::test]
async fn azure_token_vms_and_public_ips() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login/tenant-1/oauth2/v2.0/token"))
        .and(body_string_contains("grant_type=client_credentials"))
        .and(body_string_contains("client_id=client-1"))
        .and(body_string_contains(
            "scope=https%3A%2F%2Fmanagement.azure.com%2F.default",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "token_type": "Bearer", "expires_in": 3599, "access_token": "arm-token"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let vms_path = format!("/arm/subscriptions/{SUB}/providers/Microsoft.Compute/virtualMachines");
    let next = format!(
        "{}{vms_path}?api-version=2024-07-01&$skiptoken=abc",
        server.uri()
    );
    Mock::given(method("GET"))
        .and(path(vms_path.clone()))
        .and(query_param_is_missing("$skiptoken"))
        .and(query_param("api-version", "2024-07-01"))
        .and(header("authorization", "Bearer arm-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"value": [vm("vm1", "nic1")], "nextLink": next})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(vms_path))
        .and(query_param("$skiptoken", "abc"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"value": [vm("vm2", "nic2")]})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/arm/subscriptions/{SUB}/providers/Microsoft.Network/networkInterfaces"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"value": [
                {"id": nic_id("NIC1"), "properties": {"ipConfigurations": [
                    {"properties": {"privateIPAddress": "10.0.0.4", "primary": true,
                                    "publicIPAddress": {"id": pip_id("ip1")}}}
                ]}},
                {"id": nic_id("nic2"), "properties": {"ipConfigurations": [
                    {"properties": {"privateIPAddress": "10.0.0.5", "primary": true}}
                ]}}
            ]})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/arm/subscriptions/{SUB}/providers/Microsoft.Network/publicIPAddresses"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"value": [
                {"id": pip_id("ip1"), "properties": {"ipAddress": "20.1.2.3",
                  "dnsSettings": {"fqdn": "vm1.westeurope.cloudapp.azure.com"}}}
            ]})),
        )
        .mount(&server)
        .await;

    let inv = azure(&server).list().await.unwrap();
    assert_eq!(inv.instances.len(), 1, "{inv:#?}");
    let a = &inv.instances[0];
    assert_eq!(a.cloud_id, "azure:vm:vm-vm1");
    assert_eq!(a.address, "20.1.2.3");
    assert_eq!(a.private_ip.as_deref(), Some("10.0.0.4"));
    assert_eq!(
        a.public_dns.as_deref(),
        Some("vm1.westeurope.cloudapp.azure.com")
    );
    assert_eq!(a.os, OsKind::Ubuntu);
    assert_eq!(a.tags, vec!["env:prod".to_owned()]);
    assert_eq!(a.username_hint.as_deref(), Some("azureuser"));
    assert_eq!(inv.warnings.len(), 1);
    assert!(inv.warnings[0].contains("vm2"), "{:?}", inv.warnings);
}

#[tokio::test]
async fn azure_error_mapping() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": "invalid_client", "error_description": "AADSTS7000215: Invalid client secret provided."
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"access_token": "t"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
            "error": {"code": "AuthorizationFailed", "message": "no read permission"}
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "17"))
        .mount(&server)
        .await;
    let client = azure(&server);
    match client.list().await {
        Err(InventoryError::AuthError { message }) => {
            assert!(message.contains("invalid_client"));
            assert!(!message.contains("s3cr3t"));
        }
        other => panic!("{other:?}"),
    }
    match client.list().await {
        Err(InventoryError::AuthError { message }) => {
            assert!(message.contains("AuthorizationFailed"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        client.list().await,
        Err(InventoryError::RateLimited {
            retry_after: Some(Duration::from_secs(17))
        })
    );
    let dbg = format!("{client:?}");
    assert!(!dbg.contains("s3cr3t"), "{dbg}");
}

#[tokio::test]
async fn fetch_inventory_rejects_malformed_credentials_without_echoing() {
    let mut settings = serde_json::Map::new();
    settings.insert("regions".into(), serde_json::json!(["us-east-1"]));
    let err = cloud_inventory::fetch_inventory(
        core_model::CloudProvider::Aws,
        &settings,
        br#"{"accessKeyId": "AKIA", "secretAccessKey": 42}"#,
    )
    .await
    .unwrap_err();
    assert_eq!(err, InventoryError::Config("malformed credentials".into()));
}
