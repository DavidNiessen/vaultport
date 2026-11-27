---
title: Vault cryptography
description: Envelope encryption with XChaCha20-Poly1305, Argon2id and OS key-store key wrapping, associated data, subkeys and the operation log of the ssh-client vault.
---

# Vault cryptography

This page describes the `vault` crate. For a user-level introduction see [The vault](/concepts/vault).

## Primitives

| Purpose           | Algorithm                                               |
| ----------------- | ------------------------------------------------------- |
| Record encryption | XChaCha20-Poly1305, random 192-bit nonce per encryption |
| Password KDF      | Argon2id v1.3                                           |
| Subkeys           | HKDF-SHA256 (salt = vault id)                           |
| Randomness        | Operating-system CSPRNG                                 |

Ciphertext blobs have the layout `version (1 byte) ‖ nonce (24) ‖ ciphertext ‖ tag (16)`. All key material lives in zeroizing buffers.

## Key hierarchy

```text
master password ──Argon2id──▶ KEK ──┐
                                    ├──wraps──▶ vault key (random, 256-bit)
device key (OS key store) ──────────┘                 │
                                                      ├──▶ encrypts records and ops
                                                      └──HKDF──▶ subkeys: session logs,
                                                                 sync segments, exports, pairing
```

- The **vault key** (VK) is generated randomly when the vault is created and never stored in the clear.
- It is stored **wrapped** (encrypted) by one or both of:
  - a key-encryption key derived from the **master password** with Argon2id;
  - a random 256-bit **device key** kept in the OS key store — macOS/iOS Keychain (this-device-only on iOS), Windows Credential Manager (DPAPI), the Secret Service on Linux, or on Android wrapped by an AES-256-GCM key that lives in the Android Keystore (StrongBox when available).
- At least one wrap always exists. Changing the master password re-wraps the VK; it does not re-encrypt the data.
- Each wrap is authenticated with associated data `ssh-client/vk-wrap/v1|<method>|<vault id>`.

## Argon2id parameters

| Device class       | Memory  | Iterations | Parallelism |
| ------------------ | ------- | ---------- | ----------- |
| Desktop            | 256 MiB | 3          | 1           |
| Phones and tablets | 64 MiB  | 3          | 1           |

Parameters and the random 16-byte salt are stored next to the wrapped key. When a vault is opened, parameters read from the file are checked against hard bounds (at least 19 MiB, at most 4 GiB of memory, 1–64 iterations, 1–16 lanes), so an attacker who modifies the file cannot downgrade the KDF to make guessing cheap — or upgrade it to exhaust memory.

## Records, ops and associated data

Data is stored as an append-only log of **operations** plus materialized **documents**, both encrypted with the VK. Associated data binds every ciphertext to its place:

```text
ops:        ssh-client/op/v1|<vault id>|<op id>
documents:  ssh-client/doc/v1|<vault id>|<record id>
```

A ciphertext copied to another record, another op or another vault fails authentication.

The CRDT is a per-field last-writer-wins document: every leaf path (for example `/config/ssh/port`) is a register ordered by a hybrid logical clock `(wall ms, counter, device)`, and record deletion is a tombstone. Applying the same set of operations in any order yields the same documents.

## Secrets

Passwords, private keys and tokens are separate secret records. They are decrypted only when the core needs them — for example to authenticate a connection — and never cached in memory. Secrets can be marked **local-only** (sync-target and cloud credentials are), in which case they are not replicated to other devices.

## Subkeys

Other components never use the VK directly. They derive domain-separated subkeys with HKDF-SHA256 (`info = "ssh-client/subkey/v1|<purpose>"` or `"ssh-client/sync/<purpose>/v1"`, `salt = vault id`):

- **session logs** — see [Session logs](/guide/session-logs),
- **sync segments**, **exports** and **pairing** — see [Sync protocol](/security/sync).

Subkeys cannot decrypt vault records.

## What is not encrypted

The vault is an SQLite file. Its schema, the vault id, the KDF parameters, the wrapped keys, the number of records and their record identifiers (an entity kind and a random UUID, such as `host:…`) are visible to someone who has the file. Labels, addresses, usernames, settings and all secrets are encrypted.

## Locking

Locking drops the VK and all decrypted documents from memory, closes sessions, stops port forwards and closes pooled connections. The UI locks after inactivity and, on mobile, when the app goes to the background (both configurable).
