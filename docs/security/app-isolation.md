---
title: App isolation
description: How the Tauri shell isolates the web UI from secrets — typed IPC with opaque ids, the isolation pattern, a strict content security policy and least-privilege capabilities.
---

# App isolation

ssh-client's UI is a web application (React) running in the system webview through [Tauri 2](https://v2.tauri.app/). Web UIs pull in many dependencies, so the design assumes a UI dependency _could_ be compromised and limits what it could do.

## Secrets stay in the core

The most important rule: **secrets never cross the IPC boundary**. The UI works with opaque identifiers — "connect host `0190…`", "use key `0191…`" — and the Rust core resolves them, decrypts what it needs, and wipes it after use. Passwords you type into a prompt are handed to the core and not kept by the UI. Even a fully compromised UI cannot read your private keys out of the vault through IPC, because no command returns them.

All IPC commands are typed: TypeScript bindings are generated from the Rust command signatures, so the UI and core agree on every payload.

## Isolation pattern

The app uses Tauri's [isolation pattern](https://v2.tauri.app/concept/inter-process-communication/isolation/). Every IPC message from the UI first passes through a sandboxed iframe that runs a small hook before the message is encrypted and handed to the core. The hook rejects:

- payloads that are not objects,
- command names that do not match `^(plugin:[a-z0-9-]+\|)?[a-z0-9_]+$`,
- JSON payloads larger than 8 MiB.

## Content Security Policy

The production webview runs with a strict CSP:

```text
default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline';
img-src 'self' data: blob: asset: http://asset.localhost; font-src 'self' data:;
connect-src ipc: http://ipc.localhost; object-src 'none'; base-uri 'none';
form-action 'none'; frame-ancestors 'none'; worker-src 'self' blob:
```

Notably, `connect-src` only allows IPC: **the UI cannot talk to the internet at all**. Every network connection — SSH, sync, cloud APIs, the model download — is made by the Rust core. Scripts can only come from the app bundle; there is no remote code.

`freezePrototype` is enabled, which freezes JavaScript built-in prototypes against prototype-pollution attacks.

## Least-privilege capabilities

The main window is granted only the Tauri permissions it needs: window controls (minimize, maximize, close, drag, fullscreen), OS information, opening URLs, open/save/message dialogs, clipboard text read and write, and restarting the app after an update. Desktop builds additionally get the updater and window-state permissions. There is no general file-system or shell access from the UI.

## Other safeguards

- **Links** printed in the terminal open only after confirmation.
- **OSC 52** clipboard writes from remote programs are _Ask_ by default.
- The **local terminal** is only available on desktop and runs your own shell.
- On macOS, the app is built with the **hardened runtime**.
