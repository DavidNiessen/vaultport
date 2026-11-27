---
title: Identities & keychain
description: The keychain holds SSH keys and identities; hosts reference them instead of copying credentials, so one change updates every host.
---

# Identities & keychain

The **Keychain** section of the vault holds two kinds of credentials that hosts and groups _reference_ rather than copy.

## Keys

A key entry stores the key's metadata (label, algorithm, public key, fingerprint, optional OpenSSH certificate) and, depending on its type, the private half:

| Type                 | Where the private key lives                                                   |
| -------------------- | ----------------------------------------------------------------------------- |
| Ed25519, ECDSA, RSA  | Encrypted in the vault (optionally also passphrase-protected)                 |
| FIDO2 security key   | On the hardware token; the vault only keeps the key handle                    |
| Biometric device key | In the Secure Enclave (macOS, iOS) or Windows Hello; it can never be exported |

Keys can be generated on the device, imported (OpenSSH, PEM PKCS#1/PKCS#8 and PuTTY formats), or enrolled on hardware. See [SSH keys & hardware keys](/guide/keys).

## Identities

An **identity** bundles a **username**, an optional **password** and any number of **keys** under a label such as _deploy (prod)_. Instead of entering credentials on each host, choose **Use an identity** in the host or group editor.

```text
Identity "deploy (prod)" = deploy + key "prod-ed25519" + key "YubiKey"
        ▲                ▲                ▲
   web-01           web-02         Production group (inherited by all its hosts)
```

Change the identity once — for example to rotate the key — and every host that uses it picks up the change.

## Inline credentials

A host or group can also carry its own username, password and key list ("Custom credentials…"). Inline and identity credentials follow the normal [inheritance rules](/concepts/groups-and-inheritance): a host without credentials uses the closest group's.

## Authentication order

On connect, ssh-client tries, in this order:

1. the keys configured for the host (directly or through its identity);
2. on desktop, if no keys are configured: your **system SSH agent** (`$SSH_AUTH_SOCK` on macOS and Linux; the Windows OpenSSH agent pipe or Pageant on Windows);
3. the stored password, if any;
4. keyboard-interactive authentication, for example one-time codes.

Servers that require several methods in a row (`AuthenticationMethods publickey,password`) are handled. If a password or key passphrase is needed but not stored, ssh-client asks for it in a prompt.

## Where secrets go

Passwords and private keys are vault **secrets**: they are only decrypted inside the Rust core at the moment they are needed and are never passed to the UI. When a host is deleted, its inline password is deleted with it; keys and identities stay in the keychain until you remove them.
