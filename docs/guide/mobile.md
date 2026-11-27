---
title: Phones & tablets
description: Using ssh-client on iPhone, iPad and Android — the phone and tablet layouts, the terminal key bar, locking, and what differs from desktop.
---

# Phones & tablets

ssh-client on iOS, iPadOS and Android is the same app with the same Rust core and the same vault format as on desktop. The interface adapts to the screen size.

## Layouts

| Width                | Layout                                                                                       |
| -------------------- | -------------------------------------------------------------------------------------------- |
| Phone (< 640 px)     | A bottom tab bar with **Vault**, **Connections** and **Settings**, and full-screen terminals |
| Tablet (640–1023 px) | The tabbed title bar from desktop, with the vault navigation as a compact icon rail          |
| Desktop (≥ 1024 px)  | Full layout with labelled navigation                                                         |

On an iPad with a hardware keyboard, the [keyboard shortcuts](/guide/keyboard-shortcuts) work with <kbd>⌘</kbd>.

## The terminal key bar

On phones, a key bar sits above the on-screen keyboard with the keys that touch keyboards lack:

- <kbd>esc</kbd>, <kbd>tab</kbd>, and **sticky** <kbd>ctrl</kbd> and <kbd>alt</kbd>, which apply to the next key you tap on the bar;
- arrow keys;
- a **symbols** page: ``| / - ~ _ * & $ > < : " ' ` { } [ ]``;
- a **function keys** page: F1–F10, PgUp, PgDn, Home, End;
- buttons to make the text **larger** or **smaller**.

## Locking and privacy

- **Lock when the app is in the background** is on by default, so the vault is locked as soon as you switch apps. Decrypted data is wiped from memory.
- The vault's Argon2id parameters are tuned for phones (64 MiB) so unlocking stays quick.

On iOS and iPadOS you can enable **Unlock with this device**, which keeps the unlock key in the Keychain. On Android, use a master password; device unlock is not available there yet.

## Keys on mobile

- Create a **biometric device key** in the Secure Enclave of your iPhone or iPad; every login asks for Face ID or Touch ID. See [SSH keys & hardware keys](/guide/keys#biometric-device-keys).
- FIDO2 security-key enrollment over USB is a desktop feature.
- Regular Ed25519/ECDSA/RSA keys from the vault work everywhere, including keys created on another device and [synced](/guide/sync).

## What differs from desktop

| Feature                  | Phones & tablets                               |
| ------------------------ | ---------------------------------------------- |
| Local terminal           | Not available (there is no local shell to run) |
| Serial consoles          | Desktop only                                   |
| System SSH agent         | Desktop only                                   |
| Self-update              | Install new APK/IPA versions instead           |
| Edit-and-upload watching | Desktop only                                   |

Everything else — SSH, SFTP, Mosh, Telnet, port forwarding, workspaces, snippets, sync and pairing, session logs, cloud inventory and local AI (with a smaller model) — works the same.

## Getting your vault onto the phone

The quickest way is [pairing](/guide/sync#pair-a-new-device): on your computer open **Settings → Sync & backup → Pair a device**, then on the phone choose **I already have a vault → Pair with another device** and scan the QR code.
