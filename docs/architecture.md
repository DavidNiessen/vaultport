---
title: Architecture
description: A tour of the Vaultport code base — the Tauri app, the React UI and the Rust crates for the vault, protocols, sync, importers, cloud inventory and local AI.
---

# Architecture

Vaultport is a monorepo with a Cargo workspace (`crates/*` and the Tauri app) and a pnpm workspace (`apps/*`, `docs`).

```text
apps/desktop/            Tauri 2 app (desktop + iOS + Android)
├── src/                 React 19 + TypeScript UI (Tailwind, xterm.js, i18n: English, German)
├── isolation/           Tauri isolation hook that screens every IPC message
└── src-tauri/           Rust app shell: commands, sessions, key store, connection pool
crates/
├── core-model           domain model and configuration inheritance (pure, no I/O)
├── vault                encrypted local vault, CRDT op log on SQLite
├── ssh-engine           SSH client: jump hosts, proxies, host keys, auth, forwarding
├── sftp-engine          file systems, transfer queue, edit-and-upload
├── mosh-proto           native Mosh client
├── telnet-proto         Telnet client
├── serial-io            serial console sessions
├── term-intel           shell integration, history, autocomplete, risk rating
├── session-log          encrypted asciicast recordings
├── sync                 serverless sync, export, pairing
├── importers            OpenSSH, known_hosts, PuTTY, MobaXterm, SecureCRT, CSV
├── cloud-inventory      AWS, DigitalOcean, Azure inventory
├── hw-keys              FIDO2 and platform-bound keys
├── local-ai             optional on-device command generation
└── mcp-server           minimal Model Context Protocol server (tools over JSON-RPC)
plugins/mobile/          native Android (Kotlin) and iOS (Swift) services
docs/                    this website (VitePress)
docker/test-servers/     OpenSSH servers for integration tests
```

## Layers

```text
┌──────────────────────────── React UI ────────────────────────────┐
│ views, xterm.js terminals, state (zustand, TanStack Query), i18n │
└──────────────── typed IPC (tauri-specta bindings) ───────────────┘
                    ▲ opaque ids, never secrets ▼
┌──────────────────────── src-tauri (app shell) ───────────────────┐
│ commands · session manager · connection pool · key store · logs  │
└──────────────────────────────────────────────────────────────────┘
                                 ▼
┌──────────────────────────── Rust crates ─────────────────────────┐
│ vault · ssh-engine · sftp-engine · mosh · telnet · serial · sync │
│ importers · cloud-inventory · hw-keys · term-intel · local-ai    │
└──────────────────────────────────────────────────────────────────┘
```

The crates are **app-agnostic**: storage, prompts and platform services are injected through traits, so each can be tested on its own. Protocol cores are **sans-IO** state machines with a thin tokio driver on top, which makes them deterministic to test.

## Crates

### `core-model`

Hosts, groups, identities, keys, snippets, port-forward rules, known hosts, cloud integrations and their validation. Groups carry a sparse `HostConfig`; `resolve` computes a host's effective configuration per parameter from the closest ancestor. Pure data and logic — no I/O, no crypto, no secrets (those are referenced through `SecretRef`). See [Groups & inheritance](/concepts/groups-and-inheritance).

### `vault`

Envelope encryption (XChaCha20-Poly1305, Argon2id key wrapping, OS key-store device keys), a per-field last-writer-wins CRDT ordered by hybrid logical clocks, stored as an op log plus materialized documents in SQLite. Secrets are separate records decrypted on demand. See [Vault cryptography](/security/vault-crypto).

### `ssh-engine`

Built on `russh` with `ring` and RustCrypto. Provides `connect` through HTTP CONNECT / SOCKS5 proxies and any number of jump hosts, strict known-hosts verification with certificates and revocation, ordered authentication with partial success, shells, exec, SFTP channels, local/remote/dynamic forwarding and a read-only in-process agent. The app plugs in `Interaction` (prompts), `KnownHosts` (storage) and `ExternalSigner` (hardware keys).

### `sftp-engine`

One async `FileSystem` trait over the local disk and SFTP, a `TransferQueue` with concurrency limits, resumable part files and conflict policies, and edit-and-upload sessions with change detection. Server-supplied names are validated before use.

### `mosh-proto`

A native Mosh client, wire-compatible with `mosh-server` 1.3/1.4: AES-128-OCB3 datagrams, fragmentation and zlib, the State Synchronization Protocol, RTT estimation and roaming.

### `telnet-proto`

A sans-IO RFC 854 codec with RFC 1143 option negotiation (ECHO, SGA, BINARY, TERMINAL-TYPE, NAWS, NEW-ENVIRON) and a tokio session.

### `serial-io`

Port enumeration with USB details, validated line settings, and sessions over native ports or app-provided transports.

### `term-intel`

Deterministic terminal intelligence: shell-integration escape-sequence parsing (OSC 7, 133, 633, 1337) and password-prompt detection, opt-in bash/zsh/fish integration scripts, history import with frecency ranking, the autocomplete engine with about 60 built-in command specs, and command risk rating.

### `session-log`

Encrypted, chunked asciicast v2 recording with tamper detection, search, export, crash recovery and retention policies.

### `sync`

Serverless sync through folder, WebDAV, S3 and Git backends with encrypted, hash-chained segment files, integrity issue reporting and compaction; encrypted `.vaultport` export; QR-code pairing. See [Sync protocol](/security/sync).

### `importers`

Pure parsers for OpenSSH `ssh_config` and `known_hosts`, PuTTY registry exports, MobaXterm, SecureCRT XML and CSV, producing validated `core-model` entities with secrets kept separate.

### `cloud-inventory`

Read-only clients for AWS EC2/Lightsail (own SigV4 signer), DigitalOcean and Azure Resource Manager, plus three-way reconciliation of an inventory with a group's hosts.

### `hw-keys`

FIDO2 `sk-*` keys over CTAP2/USB HID and platform-bound keys (Secure Enclave, Windows Hello, Android Keystore bridge), all exposed as one `HardwareSigner` that returns verified SSH signatures.

### `local-ai`

A pinned model catalog, a verifying resumable downloader, a llama.cpp engine behind the `llama` feature, grammar-constrained prompting and output validation. See [Local AI](/guide/local-ai).

### `mcp-server`

A small, dependency-light [Model Context Protocol](https://modelcontextprotocol.io) server: the lifecycle (`initialize`, `ping`), `tools/list` and `tools/call` with concurrent requests and cancellation, over newline-delimited JSON-RPC on any async stream (stdio, a Unix socket or a named pipe). The app implements the tools; see [AI agents](/guide/ai-agents).

## The app shell (`src-tauri`)

- **Commands** for vault lifecycle, entities, sessions, files, forwarding, keys, logs, import, cloud, sync, AI and updates. TypeScript bindings are generated with `tauri-specta`.
- A **session manager** runs SSH, Mosh, Telnet, serial and local PTY sessions and streams their events to the UI.
- A **connection pool** shares SSH connections between SFTP panes and port forwards.
- The **key store** module keeps device unlock keys in Keychain, Credential Manager or Secret Service, and on Android through the mobile plugin.
- The **mobile plugin** (`plugins/mobile`, Kotlin and Swift) provides the Android Keystore secure store and hardware keys, the Android keep-alive foreground service and screen privacy. It exposes no commands to the web UI.
- The **agent** module serves AI agents over the Model Context Protocol (see [AI agents](/guide/ai-agents)); `vaultport mcp` is the stdio relay that MCP clients start.
- **Recorders** write session logs and collect command history.

## The UI (`apps/desktop/src`)

React 19 with TypeScript, Tailwind CSS 4, Radix primitives, xterm.js 6 (WebGL renderer), zustand and TanStack Query, and i18next with English and German translations. Feature folders (`features/hosts`, `features/sftp`, `features/sync`, …) map to the screens; the custom title bar adapts to macOS, Windows and Linux, and the shell switches between phone, tablet and desktop layouts.
