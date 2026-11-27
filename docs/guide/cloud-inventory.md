---
title: Cloud inventory
description: Keep a group of hosts in sync with AWS EC2 and Lightsail, DigitalOcean and Azure using read-only credentials, called directly from your device.
---

# Cloud inventory

A **cloud integration** keeps a group of hosts in line with the machines in a cloud account. There is no service in between: ssh-client calls the provider's API **directly from your device**, with credentials that stay in your vault.

## Supported providers

| Provider        | Lists                                 | Credentials                                                                    |
| --------------- | ------------------------------------- | ------------------------------------------------------------------------------ |
| AWS             | EC2 instances and Lightsail instances | Access key ID + secret access key, and a comma-separated list of regions       |
| DigitalOcean    | Droplets                              | Personal access token with read scope                                          |
| Microsoft Azure | Virtual machines with a public IP     | Service principal (tenant ID, client ID, client secret) with the _Reader_ role |

**Use read-only credentials.** ssh-client only ever reads the inventory; a key that can do more is an unnecessary risk.

## Adding an integration

1. **Hosts → Import → Cloud**.
2. Choose the provider, give the integration a label, and enter the credentials (and, for AWS, regions such as `us-east-1,eu-central-1`).
3. **Add and sync.** ssh-client creates a group named after the integration and fills it with one host per instance.

In the current build the sync runs when you add the integration; the core can re-sync an existing integration, but there is no button for it in the UI yet. Cloud credentials are stored as vault secrets that **stay on this device**: they are not synced to your other devices.

::: info Azure subscriptions
The Azure client lists VMs per subscription. If a sync reports that no Azure subscription is configured, your build's import dialog does not yet ask for subscription IDs; this is a known gap in the current UI.
:::

## What a sync changes

Hosts are matched to instances by their cloud id. Only a few fields are **provider-managed**: the label, the address, the operating system and the provider's tags. Everything else — credentials, jump hosts, notes, favorites, terminal settings — belongs to you and is never touched.

- If you have not edited a provider-managed field, it follows the provider.
- If you edited it (for example renamed a host), your value wins on every later sync.
- Tags you add are kept; tags removed at the provider disappear.
- Instances that no longer exist are removed from the group.
- Hosts you added to the group by hand are never modified or removed.
- Any API error aborts the sync without changing anything, so a partial listing can never delete hosts.

Because the integration owns a normal group, you can put shared settings on it — a username, a key, a jump host — and every discovered host [inherits](/concepts/groups-and-inheritance) them.

## Network behaviour

- TLS is provided by rustls; redirects are disabled.
- Pagination links are followed only on the provider's own API origin, so tokens and signatures are never sent to another host.
- AWS requests are signed with Signature Version 4 inside ssh-client; no AWS SDK or CLI is needed.
- Credentials are wiped from memory after use and redacted from logs.
