---
title: SSH keys & hardware keys
description: Generate and import SSH keys, use OpenSSH certificates, install public keys on servers, and use FIDO2 security keys, the Secure Enclave or Windows Hello.
---

# SSH keys & hardware keys

Keys live in the **Keychain** section of the vault. Hosts and identities reference them by id.

## Generate a key

**Keychain → New key → Generate key**, then pick the type:

| Type        | Notes                                                     |
| ----------- | --------------------------------------------------------- |
| Ed25519     | Recommended: fast, small, supported by all modern servers |
| ECDSA P-256 | For environments that require NIST curves                 |
| ECDSA P-384 | Same, larger curve                                        |
| RSA 4096    | For legacy systems that do not accept the others          |

The private key is generated on your device and stored encrypted in the vault. You may add a passphrase; ssh-client can remember it in the vault or ask on each use.

## Import a key

**Import key** accepts OpenSSH private keys, PEM (PKCS#1 / PKCS#8) and PuTTY `.ppk` files, encrypted or not. Paste the key or choose the file. You can also attach an **OpenSSH certificate** (`…-cert-v01@openssh.com`) to any key so servers that trust your CA accept it.

Importing an `ssh_config` with `IdentityFile` lines imports those keys too — see [Importing](/guide/importing).

## Install a public key on a server

Select a key and choose **Install on host…**. ssh-client connects to the saved host and appends the public key to `~/.ssh/authorized_keys`, creating the file with safe permissions if needed. If the key is already there, nothing changes. **Copy public key** puts the OpenSSH public key on the clipboard for anything else.

## FIDO2 security keys

Security keys such as YubiKeys, SoloKeys or Nitrokeys can hold SSH keys in OpenSSH's FIDO formats `sk-ssh-ed25519@openssh.com` and `sk-ecdsa-sha2-nistp256@openssh.com`, which work with OpenSSH 8.4 and newer on the server.

1. **Keychain → New key → FIDO2 security key**.
2. Insert the key and touch it when it blinks; enter its PIN if it has one.
3. Install the public key on your servers as above.

Every login then requires the physical key (and a touch). The vault only stores the key _handle_; the private key never leaves the token. Enrollment over USB is available in the desktop apps (Windows, macOS, Linux).

::: tip Linux
If no security key is found, make sure your user can access the device's `hidraw` node — see [Installation](/guide/installation#security-keys-and-serial-ports).
:::

## Biometric device keys

A **biometric device key** is created inside your device's secure hardware and can never be exported or copied — not even into the vault:

| Platform   | Backend                                     | Key type                            |
| ---------- | ------------------------------------------- | ----------------------------------- |
| macOS, iOS | Secure Enclave                              | ECDSA P-256 (`ecdsa-sha2-nistp256`) |
| Windows    | Windows Hello                               | RSA-2048 (`rsa-sha2-256`)           |
| Android    | Android Keystore (StrongBox when available) | ECDSA P-256 (`ecdsa-sha2-nistp256`) |

Every signature asks for Touch ID, Face ID, Windows Hello or, on Android, the system biometric prompt (strong biometrics only). Because such a key is bound to one device, register a separate device key per computer or phone, or combine it with another key on your servers. On Android the key is invalidated when you enroll a new fingerprint or face, so you would then need to create and install a new one. Linux has no supported backend (TPM 2.0 is not implemented).

::: info Android support is new
Android Keystore keys have not yet been verified on many devices. Keep a second way to log in to your servers until you have confirmed the key works on your phone.
:::

## How hardware signatures are checked

Before a signature from a security key or the platform key store is sent to a server, ssh-client verifies it against the key's public key, so a misbehaving device can never make it send a wrong signature. PINs are held in memory that is wiped after use.

## Agent forwarding

When agent forwarding is enabled for a host, ssh-client serves a **read-only, in-process agent** to that server. It can list and use only the keys configured for that host, it cannot add, remove or export keys, and each signature request can require your confirmation. SHA-1 `ssh-rsa` signatures are refused unless legacy algorithms are enabled. Only enable agent forwarding for servers you trust.

When no keys are configured for a host, the desktop apps can also use your **system SSH agent** — see [authentication order](/concepts/identities-and-keychain#authentication-order).
