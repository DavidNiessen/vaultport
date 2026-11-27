---
title: Introduction
description: What ssh-client is, what it does, and the principles behind it — serverless, private and open source.
---

# Introduction

**ssh-client** is a free, open-source terminal and file-transfer client for **SSH, SFTP, Mosh, Telnet and serial consoles**. It runs on Windows, macOS, Linux, iOS, iPadOS and Android from one code base: a Rust core for everything security-relevant and a React UI, packaged with [Tauri 2](https://v2.tauri.app/).

It is built around three promises:

- **Serverless.** There is no account, no backend and no cloud operated by this project. Every feature works without a server of ours.
- **Private.** Hosts, keys, passwords, snippets and settings live in a local vault encrypted with XChaCha20-Poly1305. There is no telemetry. Data only leaves your device when you connect to a server, call a provider API you configured, or sync to storage you own.
- **Open.** The whole application is licensed GPL-3.0-or-later, and releases are signed and attested so you can check that a download was built from this repository.

## What you can do with it

| Area            | Highlights                                                                                                                                                                                                            |
| --------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Connections     | SSH with jump hosts, HTTP CONNECT and SOCKS5 proxies, agent forwarding and environment variables; Mosh; Telnet; serial consoles; a local terminal on desktop                                                          |
| Security        | Strict host-key verification, `@cert-authority` and `@revoked` support, post-quantum `mlkem768x25519-sha256` key exchange, FIDO2 and biometric hardware keys                                                          |
| Files           | A dual-pane SFTP browser with drag and drop, a resumable transfer queue and edit-and-upload                                                                                                                           |
| Productivity    | Groups with inherited settings, tags and favorites, workspaces with broadcast input, snippets with variables, deterministic autocomplete, a command palette                                                           |
| Your data       | Importers for OpenSSH, PuTTY, MobaXterm, SecureCRT and CSV; read-only cloud inventory for AWS, DigitalOcean and Azure; encrypted session logs; encrypted backups                                                      |
| Sync            | End-to-end encrypted, conflict-free sync through a synced folder, WebDAV or S3-compatible storage, plus QR-code device pairing                                                                                        |
| Optional extras | Local AI that turns plain language into a shell command, running entirely on your device; and an MCP server that lets AI agents such as Claude Code work with your hosts under your approval. Both are off by default |

## How it fits together

```text
┌───────────────────────── your device ──────────────────────────┐
│  React UI (webview)  ── typed IPC, opaque ids ──▶  Rust core   │
│  strict CSP · isolation             vault · ssh · sftp · sync  │
└────────────────────────────────────────────────────────────────┘
         │ SSH · Mosh · Telnet · serial      │ ciphertext only
         ▼                                   ▼
    your servers          storage you own (folder, WebDAV, S3)
```

Secrets never cross into the web UI: it handles opaque identifiers and asks the core to act on them. See the [architecture overview](/architecture) and the [security model](/security/) for the details.

## A native feel on every platform

ssh-client draws its own title bar with tabs, but follows the conventions of each system: macOS keeps its traffic-light buttons, Windows gets Fluent-style caption buttons, and Linux honours your GNOME `button-layout` setting (order and side). On phones and tablets the layout switches to a touch-first design with a terminal key bar — see [Phones & tablets](/guide/mobile).

## Next steps

1. [Install ssh-client](/guide/installation) for your platform.
2. Follow the [quick start](/guide/quick-start) to create your vault and connect to your first server.
3. Learn how the [vault](/concepts/vault) and [groups & inheritance](/concepts/groups-and-inheritance) work.
