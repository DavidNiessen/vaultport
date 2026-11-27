---
title: Jump hosts & proxies
description: Reach servers through one or more bastion hosts (ProxyJump) and through HTTP CONNECT or SOCKS5 proxies, with host-key checks on every hop.
---

# Jump hosts & proxies

## Jump hosts (ProxyJump)

Many networks only allow SSH through a **bastion** (jump host). In the host editor, **Jump hosts → Add jump host** lets you pick one or more saved hosts; Vaultport connects through them **in order**.

```text
you ──▶ bastion-eu ──▶ bastion-internal ──▶ 10.0.0.5
```

How it works:

- Vaultport connects to the first hop, then opens a `direct-tcpip` channel through it to the next hop, and so on — the same mechanism as OpenSSH's `ProxyJump`. No agent or port forward is needed on the bastion.
- **Every hop** gets its own host-key verification and authentication with that hop's own credentials, so each bastion can use a different user and key.
- The connect timeout applies per hop and pauses while a prompt (for example a host-key confirmation or a security-key touch) waits for you.

Jump hosts are an ordinary setting, so put them on a **group** and every host inside reaches the network through the same bastion. Importing an `ssh_config` with `ProxyJump` lines sets this up automatically; as in OpenSSH, the first hop's own jump chain is prepended.

For a one-off connection, quick connect understands `-J`:

```text
ssh -J bastion.example.com alice@10.0.0.5
```

## Proxies

To reach the **first** hop through a proxy, set **Proxy** on the host or group:

| Type         | Notes                                                        |
| ------------ | ------------------------------------------------------------ |
| HTTP CONNECT | Optional Basic authentication; default port 3128             |
| SOCKS5       | Optional username/password authentication; default port 1080 |

Host names are always sent to the proxy **unresolved**, so DNS resolution happens on the proxy side and does not leak from your network. The proxy password is stored encrypted in the vault.

`ProxyCommand` from `ssh_config` is not supported; the importer warns about it.

## Combining both

Proxy and jump hosts combine: the proxy is used to reach the first hop, then the chain continues through SSH. Every host-key check and every authentication still happens end-to-end between Vaultport and each SSH server — the proxy only sees encrypted SSH traffic.
