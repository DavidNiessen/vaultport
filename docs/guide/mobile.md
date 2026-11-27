---
title: Phones & tablets
description: Using Vaultport on iPhone, iPad and Android — the phone and tablet layouts, the terminal key bar, locking, and what differs from desktop.
---

# Phones & tablets

Vaultport on iOS, iPadOS and Android is the same app with the same Rust core and the same vault format as on desktop. The interface adapts to the screen size.

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

- **Lock when the app is in the background** is on by default, so the vault is locked as soon as you switch apps. Decrypted data is wiped from memory and open sessions are closed.
- **Screen privacy:** on Android the app window is marked secure (`FLAG_SECURE`), so screenshots and screen recordings show nothing and the app switcher shows a blank preview. On iOS and iPadOS the app covers itself with a blur when it leaves the foreground, so the app switcher does not show your terminal.
- The vault's Argon2id parameters are tuned for phones (64 MiB) so unlocking stays quick.

**Unlock with this device** works on both platforms: on iOS and iPadOS the unlock key is kept in the Keychain with "this device only" access; on Android it is encrypted with an AES-256-GCM key inside the Android Keystore (StrongBox when available). See [The vault](/concepts/vault#unlocking).

## Staying connected in the background

Mobile systems suspend apps in the background, which drops network connections. To keep sessions and tunnels alive:

- **Android:** while at least one session or port forward is open, Vaultport runs a foreground service with a persistent notification ("1 connection is open"). Tapping it returns to the app; it disappears when the last connection closes. On Android 13 and newer, allow notifications for the service to be shown.
- **iOS and iPadOS:** the app asks the system for extra background time when it leaves the foreground. iOS still suspends it after a short while, so long-running work is best done in a [Mosh](/guide/mosh) session, which survives suspension and network changes.

This only helps when **Lock when the app is in the background** is turned off — locking closes all sessions. Decide per device which matters more to you.

::: info New on mobile
The Android Keystore integration, the keep-alive service and screen privacy are new and have not yet been tested on many real devices. Please [report problems](https://github.com/DavidNiessen/vaultport/issues) with your device model and OS version.
:::

## Keys on mobile

- Create a **biometric device key**: on iPhone and iPad it lives in the Secure Enclave and every login asks for Face ID or Touch ID; on Android it is a P-256 key in the Android Keystore (StrongBox when available) and every login shows the system biometric prompt. See [SSH keys & hardware keys](/guide/keys#biometric-device-keys).
- FIDO2 security-key enrollment over USB is a desktop feature.
- Regular Ed25519/ECDSA/RSA keys from the vault work everywhere, including keys created on another device and [synced](/guide/sync).

## What differs from desktop

| Feature                  | Phones & tablets                                                                                |
| ------------------------ | ----------------------------------------------------------------------------------------------- |
| Local terminal           | Not available (there is no local shell to run)                                                  |
| Serial consoles          | Desktop only                                                                                    |
| System SSH agent         | Desktop only                                                                                    |
| Self-update              | Install new APK/IPA versions instead                                                            |
| Edit-and-upload watching | Desktop only                                                                                    |
| Local AI                 | iOS and iPadOS use the smaller model; release builds for Android are currently built without it |
| AI agents (MCP)          | Desktop only                                                                                    |

Everything else — SSH, SFTP, Mosh, Telnet, port forwarding, workspaces, snippets, sync and pairing, session logs and cloud inventory — works the same.

## Getting your vault onto the phone

The quickest way is [pairing](/guide/sync#pair-a-new-device): on your computer open **Settings → Sync & backup → Pair a device**, then on the phone choose **I already have a vault → Pair with another device** and scan the QR code.
