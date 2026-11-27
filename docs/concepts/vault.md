---
title: The vault
description: How the encrypted vault stores hosts, keys and secrets, how it is unlocked, when it locks, and how it makes serverless sync possible.
---

# The vault

Everything you configure in Vaultport — hosts, groups, identities, keys, passwords, known hosts, snippets, port-forwarding rules, workspaces, cloud integrations and settings — is stored in a **vault**: a single encrypted SQLite file in the app's data directory. You can have several vaults on one device (for example _work_ and _personal_) and switch between them on the lock screen.

## Encryption at a glance

- A random 256-bit **vault key** encrypts every record with **XChaCha20-Poly1305**. The vault id and the record id are bound into each ciphertext, so records cannot be swapped between entries or vaults.
- The vault key itself is only ever stored **wrapped**:
  - by a key derived from your **master password** with **Argon2id** (256 MiB of memory and 3 passes on desktop, 64 MiB on phones and tablets), and/or
  - by a random **device key** kept in the operating system's key store (macOS Keychain, iOS Keychain with "this device only" access, Windows Credential Manager/DPAPI, Linux Secret Service, or on Android encrypted with an AES-256-GCM key held in the Android Keystore).
- **Secrets** (passwords, private keys, API tokens) are separate records that are only decrypted when the Rust core needs them, held in memory that is wiped after use, and never sent to the UI.

The details are in [Vault cryptography](/security/vault-crypto).

## Unlocking

| Method             | How it works                                                                                         |
| ------------------ | ---------------------------------------------------------------------------------------------------- |
| Master password    | Derives the key-encryption key with Argon2id. Works on every device.                                 |
| Unlock with device | Reads the device key from the system key store. Convenient; tied to this device and your OS account. |

You can enable both, or either one alone, and change this later in **Settings → Security**: set or change the master password, remove it (only while device unlock is on), or turn device unlock on and off.

::: info Android
On Android, the device key is encrypted with an AES-256-GCM key that is generated inside the Android Keystore (StrongBox-backed when the phone has a StrongBox chip) and never leaves it; only the ciphertext is kept in the app's private storage. This support is new and has not yet been verified on a wide range of devices — keep a master password or another unlock method as a fallback.
:::

## Locking

When the vault locks, decrypted data is wiped from memory and open sessions are closed. Vaultport locks the vault:

- after a period of inactivity (15 minutes by default, configurable or _never_),
- when the computer wakes from sleep (desktop, on by default; **Settings → Security → Lock when the computer sleeps**) — Vaultport notices the suspend as soon as it runs again, before you can use it,
- when the app moves to the background (phones and tablets, on by default),
- when you press <kbd>Mod</kbd>+<kbd>Shift</kbd>+<kbd>L</kbd> or choose **Lock vault**.

## Why an operation log?

Internally the vault is not a set of rows that get overwritten. It is an append-only **log of operations** — "set field `/config/ssh/port` of host X to 2222 at time T" — that is materialized into documents. Each field is a _last-writer-wins register_ ordered by a hybrid logical clock, and deletions are tombstones.

This is a [CRDT](https://crdt.tech/): any two devices that have seen the same set of operations hold exactly the same data, no matter in which order the operations arrived. That property is what lets Vaultport sync through plain, untrusted storage — a folder, WebDAV or an S3 bucket — without a server to resolve conflicts. See [Sync, backup & pairing](/guide/sync) and the [sync protocol](/security/sync).

## Backups

Because the vault is end-to-end encrypted, losing every device _and_ the master password means losing the data. Protect yourself with at least one of:

- sync to a second device,
- an [encrypted `.vaultport` export](/guide/sync#encrypted-backups) kept somewhere safe.
