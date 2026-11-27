---
title: Sync protocol
description: How Vaultport replicates the vault through untrusted user-owned storage with encrypted segment files, per-device hash chains, integrity issues, compaction, export and pairing.
---

# Sync protocol

The `sync` crate replicates a vault through **storage the user owns** — a synced folder, WebDAV, an S3-compatible bucket, or (in the engine) a Git branch. The storage is treated as **untrusted**: it may read, delete, replay or modify anything it stores.

## Layers

1. **Vault ops** are already sealed with the vault key (see [Vault cryptography](/security/vault-crypto)).
2. They travel in immutable **segment files** that add an outer AEAD layer and a **per-device hash chain**.
3. Because the vault is a CRDT, segments from different devices can be applied in any order.

## Segment files

```text
vault-<vault-uuid>/segments/<device-hex16>/<first:016x>-<last:016x>-<random16>.seg
```

Every device numbers its segments 1, 2, 3 … on a backend. A **delta** segment has `first == last`; a **snapshot** written by compaction has `first == 0` and contains every op the device knew. The random suffix makes names collision-free, so writers never overwrite each other.

```text
file  = "SSHSEG01" ‖ XChaCha20-Poly1305(segment key, aad, json(body))
aad   = "vaultport/segment/v1|<vault id>|<device>|<counter>|<delta|snapshot>"
body  = { vault, device, counter, kind, prev = SHA-256(previous file of this device), ops, covers? }
```

- The segment key is an HKDF subkey of the vault key.
- The AAD is reconstructed from the **file name**, so renaming a segment to another device, counter or kind makes it fail authentication.
- `prev` links each segment to the complete previous file of the same device, forming a hash chain.
- Object names are restricted to `[A-Za-z0-9._-]` path segments, so a hostile listing cannot cause path traversal in any backend.

## Integrity checks

For each device, the engine remembers the newest verified `(counter, hash)`. While syncing it reports — and does not apply anything suspicious from that device:

| Issue        | Meaning                                                                                           |
| ------------ | ------------------------------------------------------------------------------------------------- |
| **Gap**      | A segment is missing in the middle of a device's chain (truncation, or a file not yet propagated) |
| **Rollback** | The storage shows an older state of a device than this device has already seen                    |
| **Tampered** | A segment fails authentication or validation                                                      |
| **Fork**     | Two different segments claim the same position in a device's chain (e.g. a restored backup)       |

Other devices continue to sync. If this device's own uploads were rolled back, its next upload is a full snapshot that re-seeds the storage.

**Limitation:** hiding the newest segments of a device (tail truncation) cannot be detected by a device that has never seen them; a device that has seen them reports a rollback.

## Compaction

Compaction first syncs, then writes a snapshot containing all ops this device has applied, recording for every other device the head it covers. Only after the snapshot is stored are older segments deleted: always this device's own, and other devices' only when explicitly requested, when the preceding sync reported no issue, and only up to the covered head. Readers that later meet such a gap apply the snapshot first, which keeps every chain verifiable.

## Backends

| Backend | Notes                                                                                                                                                                   |
| ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Folder  | Atomic writes (temporary file, flush, rename); create-only writes via hard links; sync-tool conflict copies are skipped                                                 |
| WebDAV  | `PROPFIND`, conditional `PUT` (`If-None-Match` / `If-Match`), Basic authentication over rustls, redirects not followed                                                  |
| S3      | AWS Signature V4, any S3-compatible endpoint over HTTPS, conditional writes                                                                                             |
| Git     | Pure Rust (`gix`), one branch (`refs/heads/vaultport-sync`), set-of-files three-way merge that is always conflict-free; the SSH transport is not wired into the app yet |

## Encrypted export (`.vaultport`)

```text
"VAULTPRT" ‖ version u16 ‖ header length u32 ‖ header JSON ‖ payload
header  = { vaultId, kdf: { memoryKib, iterations, parallelism, salt }, createdAt, opCount, wrappedKey }
payload = XChaCha20-Poly1305(export key, "vaultport/export/v1|" ‖ SHA-256(header), json([SealedOp]))
```

`wrappedKey` is the vault key wrapped with `Argon2id(passphrase)`. Any change to the header or payload is detected. Because the payload contains the vault's own sealed ops, an imported file is the same CRDT replica and can keep syncing with the original devices.

## Device pairing

1. The unlocked device creates an offer: a QR payload and, shown separately, an 8-character code (40 bits of OS randomness, e.g. `K7QM-2XPA`) that expires after 10 minutes.
2. `transfer key = Argon2id(code, salt ‖ SHA-256(vault id | expiry | backend config)[..16])`. The vault key — and, if the app includes it, the backend secret — are wrapped with the transfer key.
3. The new device scans the payload, the user types the code, and the device unwraps the key, creates its replica and syncs.

Because the expiry and backend configuration are part of the salt, editing them in the QR payload makes the code useless. An attacker needs **both** the QR payload and the code; with the payload alone, every guess of the code costs one Argon2id evaluation (64 MiB by default). Expiry is enforced by honest clients only, which is why code entropy and KDF cost matter. Someone who sees both halves (shoulder surfing, screen recording) obtains the vault key — pair in private.

No full PAKE is used because both halves are displayed locally on the same, already trusted device.
