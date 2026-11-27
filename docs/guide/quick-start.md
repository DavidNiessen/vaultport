---
title: Quick start
description: Create your encrypted vault, add a host, verify its host key and connect — then generate a key and install it on the server.
---

# Quick start

This walkthrough takes about five minutes: create a vault, connect to a server, and switch from password to key authentication.

## 1. Create your vault

On first launch ssh-client asks you to **Create your vault**. The vault is the encrypted database that holds all your hosts, keys, passwords, snippets and settings.

1. Pick a **vault name** (letters, digits, `-` and `_`).
2. Choose a **master password** of at least 8 characters. A long passphrase is best; the strength meter helps.
3. Optionally enable **Unlock with this device**. This stores a random unlock key in the system key store (Keychain, Windows Credential Manager, Secret Service), so you can unlock without typing the password.

You need at least one of the two: a master password, or device unlock.

::: warning There is no password recovery
Nobody, including the ssh-client developers, can recover a forgotten master password. Keep a second unlock method (device unlock, another synced device) or an [encrypted backup](/guide/sync#encrypted-backups).
:::

Already use ssh-client on another device? Choose **I already have a vault** to [pair this device](/guide/sync#pair-a-new-device) or restore a `.sshvault` backup.

## 2. Add a host

Open the **Hosts** section of the vault and choose **New host**:

- **Address**: a host name or IP address.
- **Port**: `22` unless your server uses another one.
- **Username** and, if you use one, a **password** (stored encrypted in your vault).

Changes are saved automatically. To bring in many hosts at once, use **Import** — for example from `~/.ssh/config` — see [Importing](/guide/importing).

::: tip Quick connect
You do not have to save a host first. Type into the search field at the top of the Hosts view (or in a new tab) using familiar syntax, then press <kbd>Enter</kbd>:

```text
ssh alice@example.com
ssh -p 2222 alice@example.com
ssh -J bastion.example.com alice@10.0.0.5
alice@example.com:2222
ssh://alice@example.com
```

:::

## 3. Connect and verify the host key

Double-click the host (or select it and choose **Connect**). A terminal tab opens.

The first time you connect, ssh-client shows the server's **host key fingerprint**. Compare it with the one published by the server administrator (on the server: `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub`) and choose:

- **Trust and connect**: remember the key in [Known hosts](/guide/known-hosts) and connect.
- **Connect once**: connect without remembering it.
- **Disconnect**: abort.

If a known server ever presents a _different_ key, ssh-client blocks the connection with a **Host key changed!** warning. Only replace the key if you can confirm the change with the administrator.

## 4. Switch to key authentication

1. Go to **Keychain → New key → Generate key**. Ed25519 is preselected and recommended. The private key is generated on your device and stored encrypted in the vault.
2. Choose **Install on host…** and select your server. ssh-client appends the public key to `~/.ssh/authorized_keys` with safe permissions.
3. Edit the host and add the key under **Keys**. You can now remove the stored password.

Prefer hardware? Create a **FIDO2 security key** or a **biometric device key** instead — see [SSH keys & hardware keys](/guide/keys).

## 5. Keep going

- Press <kbd>Mod</kbd>+<kbd>K</kbd> for the command palette (<kbd>Mod</kbd> is <kbd>⌘</kbd> on Apple devices and <kbd>Ctrl</kbd> elsewhere). All shortcuts are listed in [Keyboard shortcuts](/guide/keyboard-shortcuts).
- Organize hosts in [groups](/concepts/groups-and-inheritance) so they share settings such as the username or a jump host.
- Open the **SFTP** tab to [transfer files](/guide/sftp).
- Set up [sync](/guide/sync) to use the same vault on your laptop and phone.

The vault locks itself after 15 minutes of inactivity (configurable in **Settings → Security**). Lock it manually with <kbd>Mod</kbd>+<kbd>Shift</kbd>+<kbd>L</kbd>.
