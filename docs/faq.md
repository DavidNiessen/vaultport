---
title: FAQ
description: Frequently asked questions about ssh-client — cost, accounts, privacy, sync, recovery, security keys, platforms and contributing.
---

# Frequently asked questions

## Is ssh-client free?

Yes. It is free and open source under the GNU GPL v3.0 or later. There are no paid tiers, no accounts and no feature gates.

## Do I need an account?

No. There is nothing to sign up for. Install the app, create a vault, connect.

## Does ssh-client collect any data?

No. There is no telemetry, analytics or crash reporting, and no server operated by the project. The app only connects to the hosts you configure, the sync storage and cloud APIs you set up, GitHub Releases when you click _Check for updates_, and Hugging Face if you choose to download the local AI model.

## Where is my data stored?

In an encrypted vault file in the app's data directory on each device. If you set up sync, encrypted copies of your changes are stored in the folder, WebDAV server or S3 bucket you chose. See [The vault](/concepts/vault).

## I forgot my master password. Can you reset it?

No — nobody can. The vault is end-to-end encrypted and the project has no copy of your keys. If you enabled **Unlock with this device**, you can still unlock on that device and set a new password in **Settings → Security**. Otherwise, use another synced device or restore an [encrypted backup](/guide/sync#encrypted-backups).

## How does sync work without a server?

Your vault is a CRDT: every change is a small encrypted operation, and devices exchange these through storage you own. Because operations merge deterministically, there are no conflicts and no central server is needed. See [Sync, backup & pairing](/guide/sync) and the [sync protocol](/security/sync).

## Can my sync provider read my data?

No. It only stores encrypted segment files. It also cannot forge, reorder or roll back your data unnoticed — ssh-client detects and reports such tampering.

## Can I use iCloud Drive, Dropbox, Google Drive or OneDrive?

Yes, through the **synced folder** target: choose a folder that the provider's app keeps in sync. ssh-client ignores conflict copies those tools sometimes create.

## Is Git sync supported?

The sync engine includes a Git backend, but the Git-over-SSH transport is not connected in the app yet. Use a folder, WebDAV or S3 target for now.

## Does it support post-quantum cryptography?

Yes. The hybrid `mlkem768x25519-sha256` key exchange (the OpenSSH 9.9+ default) is preferred by default and falls back automatically for servers that do not support it.

## Can I use my YubiKey?

Yes. FIDO2 security keys work as `sk-ssh-ed25519@openssh.com` or `sk-ecdsa-sha2-nistp256@openssh.com` keys (servers need OpenSSH 8.4+). Enrollment is available in the desktop apps. See [SSH keys & hardware keys](/guide/keys).

## Does ssh-client use my existing `~/.ssh/config` and keys?

It does not read them live, but you can [import](/guide/importing) your config — including `Include` files, `ProxyJump`, forwards and `IdentityFile` keys — and your `known_hosts` in one step. On desktop, if a host has no keys configured, your system SSH agent is used.

## Is the AI feature sending my terminal to the cloud?

No. Local AI is off by default, and when enabled it runs a small model entirely on your device. Nothing you type is sent anywhere, and generated commands are never run automatically. See [Local AI](/guide/local-ai).

## Can Claude Code, Cursor or other AI agents use ssh-client?

Yes, on desktop, if you allow it. Turn on **Settings → AI agents** and add ssh-client as an MCP server to your tool. Agents can list hosts and read files, and — after you approve each change in the app — run commands, write files and start tunnels. They never see your passwords or keys. Keep in mind that whatever the agent reads from your servers goes to the AI tool's model. See [AI agents (MCP)](/guide/ai-agents).

## Is autocomplete AI-based?

No. Autocomplete is a deterministic engine based on your history, snippets, paths and built-in command specs. It never uses the AI model.

## Which platforms are supported?

Windows 10 and 11 (x64, ARM64), macOS 13+ (Apple silicon and Intel), Linux (x86_64 and aarch64; AppImage, deb, rpm), iOS and iPadOS 16+, and Android 8.0+. See [Installation](/guide/installation).

## Why isn't ssh-client in the App Store or Play Store?

Releases are published on GitHub. On iOS you can sideload the IPA or use TestFlight when a beta is offered; on Android you install the APK directly.

## Can I run it without installing anything system-wide?

On Windows, the default installer installs for your user only, without administrator rights. On Linux, the AppImage runs without installation.

## How do I verify a download?

Every release has SHA-256 checksums, Sigstore signatures and GitHub build-provenance attestations. See [Verify your download](/download#verify-your-download).

## Can I use this documentation with an AI assistant?

Yes. The site publishes [`llms.txt`](/llms.txt) and [`llms-full.txt`](/llms-full.txt), and every page is also available as Markdown — append `.md` to its URL (for example `/guide/sftp.md`).

## How can I contribute?

See [Contributing](/development/contributing). Bug reports, translations, documentation and code are all welcome; security issues go through [private reporting](/security/disclosure).
