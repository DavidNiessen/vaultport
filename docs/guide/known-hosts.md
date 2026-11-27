---
title: Known hosts
description: How ssh-client verifies host keys, what the trust prompts mean, strict mode, certificate authorities and revoked keys.
---

# Known hosts & host-key verification

A server proves its identity with its **host key**. Checking that key is what protects you from someone intercepting the connection (a man-in-the-middle attack). ssh-client verifies the host key of **every hop** of every connection — the target and each jump host — against the **Known Hosts** list in your vault.

## The prompts

| Situation                       | What you see                                                                                                       |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| First connection to a server    | **Verify host key** with the key type and SHA-256 fingerprint: _Trust and connect_, _Connect once_ or _Disconnect_ |
| The key matches a trusted entry | Nothing — you are connected                                                                                        |
| The server presents another key | **Host key changed!** with the previous and the new fingerprint. You can only _Replace and connect_ deliberately   |
| The key is marked `@revoked`    | **Revoked host key** — the connection is blocked without asking                                                    |

To check a fingerprint, ask the administrator or run on the server:

```bash
ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub
```

::: danger When the key changes
A changed key means either the server was reinstalled or someone is intercepting your connection. Only continue if you can verify the new fingerprint through another channel.
:::

## Policies

Per host or group (inherited like any other setting):

- **Ask** (default): ask on first use, reject on change.
- **Strict host key checking**: only connect to hosts already in Known Hosts; unknown hosts are refused instead of asking. Good for automation-like workflows and production groups whose keys you imported in advance.

## Managing known hosts

**Known Hosts** lists every trusted key with its host patterns and fingerprint, and can be searched by host or fingerprint. Remove entries you no longer trust.

**Import known_hosts** reads an OpenSSH `known_hosts` file, including hashed entries (`|1|salt|hash`, from `HashKnownHosts yes`), comma-separated names, `[host]:port` entries, wildcards and `!` negations, and the markers below. Invalid lines are skipped with a warning. See also [Importing](/guide/importing).

## Certificate authorities

If your organization signs host keys with an SSH certificate authority, import a `known_hosts` file that contains the CA's public key as a `@cert-authority` line:

```text
@cert-authority *.example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA… host-ca
```

A host certificate is then accepted when:

- it is a _host_ certificate signed by a matching CA key,
- it is currently valid,
- it names the host among its principals, and
- it carries no critical options.

No per-host prompt is needed for servers with valid certificates.

## Revoked keys

Entries marked `@revoked` block a key everywhere — including a certificate's signing CA. Use them to retire a compromised key without waiting for every server to be fixed.

## Algorithm preferences and key changes

When a host is already known, ssh-client prefers the host-key algorithms it already trusts for that host during negotiation. If a server then presents a key of a different type, that is treated as a **change** rather than silently accepted. See [Host-key policy](/security/host-keys) for the full decision order.
