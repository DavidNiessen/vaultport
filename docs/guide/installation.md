---
title: Installation
description: Install ssh-client on Windows, macOS, Linux, iOS, iPadOS and Android, including Linux runtime dependencies and permissions for security keys and serial ports.
---

# Installation

Grab the file for your platform from the [download page](/download), which picks the right one for your device, or from [GitHub Releases](https://github.com/davidniessen/ssh-client/releases). Before installing, you can [verify the download](/download#verify-your-download).

## Windows

**Requirements:** Windows 10 or 11, x64 or ARM64.

| File                                     | Use it for                                                         |
| ---------------------------------------- | ------------------------------------------------------------------ |
| `ssh-client_<version>_x64-setup.exe`     | Recommended. Installs for the current user, no admin rights needed |
| `ssh-client_<version>_x64_en-US.msi`     | Managed or scripted deployments (Intune, Group Policy, `msiexec`)  |
| `ssh-client_<version>_x64_de-DE.msi`     | The same, German user interface for the installer                  |
| `…_arm64-setup.exe`, `…_arm64_en-US.msi` | Windows on ARM devices                                             |

The `.exe` installer speaks English and German. Builds are Authenticode-signed when the project's code-signing certificate is configured for the release; otherwise Windows SmartScreen may warn about an unknown publisher. ssh-client uses the Microsoft Edge WebView2 runtime; if it is missing, the installer downloads it silently.

## macOS

**Requirements:** macOS 13 Ventura or later.

Download `ssh-client_<version>_universal.dmg` (one build for Apple silicon and Intel), open it and drag **ssh-client** into **Applications**. The app uses the hardened runtime, and release builds are signed and notarized when the project's Apple credentials are configured for the release; if a build is not notarized, macOS asks you to confirm opening it in **System Settings → Privacy & Security**.

## Linux

**Requirements:** x86_64 or aarch64, a desktop with WebKitGTK 4.1 (any current distribution).

| File                                  | Distribution                           |
| ------------------------------------- | -------------------------------------- |
| `ssh-client_<version>_amd64.deb`      | Debian, Ubuntu, Linux Mint, Pop!\_OS … |
| `ssh-client-<version>-1.x86_64.rpm`   | Fedora, RHEL and derivatives, openSUSE |
| `ssh-client_<version>_amd64.AppImage` | Any distribution, no installation      |
| `aarch64` / `arm64` variants          | 64-bit ARM machines                    |

```bash
# Debian / Ubuntu
sudo apt install ./ssh-client_<version>_amd64.deb

# Fedora
sudo dnf install ./ssh-client-<version>-1.x86_64.rpm

# AppImage
chmod +x ssh-client_<version>_amd64.AppImage
./ssh-client_<version>_amd64.AppImage
```

### Runtime dependencies

The `.deb` and `.rpm` packages declare their dependencies (WebKitGTK 4.1, GTK 3), so your package manager installs them. The AppImage bundles most libraries but needs **FUSE 2** to mount itself:

```bash
sudo apt install libfuse2      # Ubuntu 22.04 and older, Debian
sudo apt install libfuse2t64   # Ubuntu 24.04 and newer
```

The vault's "unlock with this device" option stores its key through the Secret Service API, so you need a running keyring such as GNOME Keyring or KWallet. Without one, use a master password.

### Security keys and serial ports

- **FIDO2 security keys** are accessed over USB HID. Your user needs access to the key's `hidraw` device; most distributions ship suitable udev rules (for example with the `libfido2` package). If enrollment says no device was found, check those rules.
- **Serial ports** (`/dev/ttyUSB*`, `/dev/ttyACM*`) usually require membership in the `dialout` group (Debian, Ubuntu, Fedora) or `uucp` group (Arch). Log out and back in after `sudo usermod -aG dialout $USER`.

## iOS and iPadOS

**Requirements:** iOS or iPadOS 16 or later.

ssh-client is not distributed through the App Store. The release workflow only publishes `ssh-client_<version>_ios.ipa` when the project's Apple signing credentials are configured; the IPA is exported for App Store Connect, so it reaches devices through TestFlight when a beta is offered, or by re-signing it with a sideloading tool such as AltStore or Sideloadly. You can always [build it yourself](/development/building) with your own Apple developer account.

## Android

**Requirements:** Android 8.0 (API level 26) or later.

Download `ssh-client_<version>_android-universal.apk` on the device, allow **Install unknown apps** for your browser or file manager when asked, and open the file. The universal APK contains native code for all common CPU architectures. The release also contains an Android App Bundle (`.aab`) for app stores; it cannot be installed directly.

## Updating

Desktop builds contain a self-updater: open **Settings → About → Check for updates**. ssh-client never checks in the background. Updates are signature-checked before they are installed. On mobile, install the newer APK or IPA over the existing app.

## Uninstalling

On desktop, uninstalling the app keeps your vaults by default: they live in the app's data directory, which you can delete separately to remove all local data. On Android, iOS and iPadOS, removing the app also removes its data, including vaults. Either way, make sure you have a [backup](/guide/sync#encrypted-backups) or another synced device first.

## Building from source

Prefer to compile it yourself? See [Build from source](/development/building).
