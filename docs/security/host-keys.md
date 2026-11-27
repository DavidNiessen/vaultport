---
title: Host-key policy
description: The exact decision order ssh-client uses to verify SSH host keys and certificates, plus the default algorithm lists and legacy fallbacks.
---

# Host-key policy & algorithms

The `ssh-engine` crate decides; the app stores the known hosts in the vault and shows the prompts. For the user-facing description see [Known hosts](/guide/known-hosts).

## Decision order

For every hop of a connection:

1. Select the known-hosts entries that match the host and port, including OpenSSH hashed patterns (`|1|salt|hash`, checked by recomputing `HMAC-SHA1(salt, hostname)`).
2. If the presented key — or, for a host certificate, the certified key or the CA key — is listed under **`@revoked`**, the connection fails **without** asking.
3. A **host certificate** signed by a matching **`@cert-authority`** key is accepted when it is a host certificate, currently valid, its signature verifies, it lists the host among its principals and it carries no critical options.
4. A plain entry with the same key data → **trusted**.
5. Plain entries exist but none matches → **changed** (for any key type — see below).
6. Otherwise → **unknown**: ask the user, or refuse under the _strict_ policy.

Algorithm negotiation prefers the host-key types already known for a host, so a server that suddenly presents a different type is treated as a changed key rather than a new one.

## Default algorithms

Most preferred first.

| Kind         | Algorithms                                                                                                                                                                                                                                                                                              |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Key exchange | `mlkem768x25519-sha256` (post-quantum hybrid, when enabled — the default), `curve25519-sha256`, `curve25519-sha256@libssh.org`, `ecdh-sha2-nistp256/384/521`, `diffie-hellman-group-exchange-sha256`, `diffie-hellman-group16-sha512`, `diffie-hellman-group18-sha512`, `diffie-hellman-group14-sha256` |
| Host key     | `ssh-ed25519`, `ecdsa-sha2-nistp256/384/521`, `rsa-sha2-512`, `rsa-sha2-256`, plus the `*-cert-v01@openssh.com` variants when a `@cert-authority` is known for the host                                                                                                                                 |
| Cipher       | `chacha20-poly1305@openssh.com`, `aes256-gcm@openssh.com`, `aes128-gcm@openssh.com`, `aes256-ctr`, `aes192-ctr`, `aes128-ctr`                                                                                                                                                                           |
| MAC          | `hmac-sha2-512-etm@openssh.com`, `hmac-sha2-256-etm@openssh.com`, `hmac-sha2-512`, `hmac-sha2-256` (unused with AEAD ciphers)                                                                                                                                                                           |
| Compression  | `none`, or `zlib@openssh.com`, `zlib` when compression is enabled                                                                                                                                                                                                                                       |

`mlkem768x25519-sha256` is the OpenSSH 9.9+ default. `sntrup761x25519-sha512` is not offered because the underlying `russh` library does not implement it.

The **strict key exchange** marker (`kex-strict-c-v00@openssh.com`, the mitigation for the Terrapin attack) and `ext-info-c` are always sent.

## Legacy algorithms

Enabling **Allow legacy algorithms** for a host appends `diffie-hellman-group14-sha1`, `diffie-hellman-group-exchange-sha1`, `ssh-rsa` (SHA-1 signatures), `aes256/192/128-cbc`, `hmac-sha1-etm@openssh.com` and `hmac-sha1` — always **after** the secure choices, so they are only used when the server offers nothing better.

## Cryptographic implementation

The SSH engine is built on [`russh`](https://github.com/Eugeny/russh) with pure-Rust cryptography (`ring` and RustCrypto), without OpenSSL, so the same code runs on desktop and mobile.
