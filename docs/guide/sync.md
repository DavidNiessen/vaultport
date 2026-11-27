---
title: Sync, backup & pairing
description: Sync your encrypted vault between devices through a folder, WebDAV or S3 storage you own, pair new devices with a QR code, and make encrypted backups.
---

# Sync, backup & pairing

Vaultport has no cloud of its own. To use the same vault on several devices, you point it at **storage you already control**. That storage only ever receives encrypted, integrity-protected files.

Open **Settings → Sync & backup**.

## Sync targets

Choose **Add sync target** and a storage type:

| Storage type                                                                               | Settings                                                                                       |
| ------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------- |
| **Synced folder** (iCloud Drive, Dropbox, Syncthing, Nextcloud desktop, a network share …) | The folder. Any folder that another app keeps in sync works                                    |
| **WebDAV** (Nextcloud, ownCloud, many NAS)                                                 | URL, username, password                                                                        |
| **S3-compatible** (AWS S3, MinIO, Cloudflare R2, Backblaze B2 …)                           | Endpoint (empty for AWS), bucket, region, optional prefix, access key ID and secret access key |
| **Git repository over SSH**                                                                | Repository and branch — see the note below                                                     |

Credentials for the storage are stored as vault secrets that stay on this device. S3 endpoints must use HTTPS.

Press **Sync now** to sync a target. Each sync downloads the changes of your other devices, merges them, and uploads this device's new changes. The list shows when each target last synced and any problem it found.

::: info Git
The sync engine contains a complete Git backend (pure Rust, no `git` binary, objects on a dedicated `vaultport-sync` branch), but the Git-over-SSH transport is not connected in the app yet. Adding a Git target works; syncing it currently reports that it is not supported. Use a folder, WebDAV or S3 meanwhile.
:::

## Conflict-free by design

The vault is a CRDT: every change is an operation with a timestamp from a hybrid logical clock, and every field is resolved last-writer-wins. Two devices that have seen the same operations always end up with identical data, whatever order the operations arrived in. You can edit on your laptop and phone while offline and sync later — there are no merge dialogs and no "conflicted copy" files.

## What the storage sees

Only immutable **segment files**, one series per device, named by vault id, device id and counter. Each segment is encrypted with XChaCha20-Poly1305 under a key derived from your vault key, and chained to the device's previous segment by a SHA-256 hash.

The storage provider therefore cannot read your data, and it cannot forge, reorder or silently drop changes either: Vaultport detects and reports missing segments (gaps), rollbacks to an older state, tampered files and forks, and stops applying that device's changes until the problem is resolved. Details are in the [sync protocol](/security/sync).

Conflict copies made by sync tools and unexpected files are ignored.

## Pair a new device

Pairing transfers the vault to a new phone, tablet or computer without the vault key ever touching a server.

**On the device that has the vault:**

1. **Settings → Sync & backup → Pair a device.**
2. A **QR code** and, separately, an 8-character **pairing code** such as `K7QM-2XPA` appear. They are valid for 10 minutes.

**On the new device:**

1. On the lock screen choose **I already have a vault → Pair with another device**.
2. Scan the QR code with the camera and paste the **pairing text**, or copy the text under the QR code across.
3. Enter the pairing code, a name for the vault on this device and a master password.

The new device receives the vault key and — if you have one — your **first sync target with its credentials**, then joins the sync like any other replica.

::: warning Pair in private
Someone who sees _both_ the QR code and the pairing code can obtain your vault key. The QR code alone is useless without the code: guessing it costs one Argon2id evaluation (64 MiB) per attempt over 40 bits of randomness. Do not share screenshots or screen recordings of the pairing screen.
:::

## Encrypted backups

**Export** writes the whole vault to one `.vaultport` file, protected by a passphrase of its own (at least 8 characters). The file is encrypted with Argon2id and XChaCha20-Poly1305, and any modification is detected.

To restore, choose **Restore from backup** on the lock screen (or **Import** in Sync & backup) and pick the file. It becomes a new vault on this device, protected by the backup passphrase, which you can change afterwards. Because the file contains the vault's own operations, a restored vault is the same replica and can keep syncing with your other devices.

Keep at least one backup somewhere safe: without your master password, a backup passphrase or another device, nobody can recover your vault.
