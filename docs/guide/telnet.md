---
title: Telnet
description: Connect to network equipment and legacy systems over Telnet (RFC 854) with proper option negotiation and window-size reporting.
---

# Telnet

Some switches, routers, embedded devices and legacy systems still only speak Telnet. Vaultport includes a Telnet client for them.

::: danger Telnet is unencrypted
Everything — including your username and password — travels in clear text. Only use Telnet on trusted networks, for example a dedicated management VLAN, or through an SSH tunnel.
:::

## Enabling Telnet for a host

In the host (or group) editor, **Enable Telnet** and set:

- **Port** — 23 by default,
- optional **username** and **password** (stored encrypted in the vault).

CSV, MobaXterm, PuTTY and SecureCRT imports create Telnet hosts automatically where the source says so.

## What is supported

Vaultport implements RFC 854 with RFC 1143 "Q method" option negotiation, which cannot get stuck in negotiation loops. Supported options:

| Option            | RFC  | Purpose                             |
| ----------------- | ---- | ----------------------------------- |
| ECHO              | 857  | Server-side echo                    |
| SUPPRESS-GO-AHEAD | 858  | Full-duplex operation               |
| BINARY            | 856  | 8-bit clean transmission            |
| TERMINAL-TYPE     | 1091 | Reports `xterm-256color` by default |
| NAWS              | 1073 | Window size, sent on every resize   |
| NEW-ENVIRON       | 1572 | Sends `USER`                        |

Everything else is refused. Line endings follow the NVT rules: a carriage return you type is sent as `CR LF`.

## Logging

Telnet sessions are recorded to [encrypted session logs](/guide/session-logs) like SSH sessions.
