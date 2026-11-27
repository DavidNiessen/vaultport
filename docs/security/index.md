---
title: Security overview & threat model
description: The security architecture of ssh-client — what it protects, against whom, and the design decisions behind the vault, sync, SSH engine and app shell.
---

# Security overview & threat model

ssh-client handles private keys, passwords and access to production infrastructure. This section summarizes how it protects them. The design principles:

1. **No servers.** There is no backend, no account system and no telemetry. Nothing can be breached on our side because there is nothing on our side.
2. **Secrets never leave the Rust core.** The web UI only handles opaque identifiers; private keys and passwords are decrypted inside the core, used, and wiped.
3. **Untrusted by default.** Servers, sync storage, cloud APIs, import files and model output are all treated as potentially hostile input.
4. **Secure defaults.** Strict host-key verification, post-quantum key exchange preferred, modern algorithms only unless you opt in, no input recording.

## Assets

| Asset                               | Where it lives                                                             |
| ----------------------------------- | -------------------------------------------------------------------------- |
| Private keys, passwords, API tokens | Vault secrets (encrypted), hardware tokens, Secure Enclave / Windows Hello |
| Hosts, groups, snippets, settings   | Vault records (encrypted)                                                  |
| Trusted host keys                   | Vault records (encrypted)                                                  |
| Session output                      | Encrypted session logs on the recording device                             |

## Threats considered

| Adversary                                              | Mitigation                                                                                                                                                      |
| ------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Someone with a copy of your vault file or backup       | XChaCha20-Poly1305 encryption, Argon2id (256 MiB desktop / 64 MiB mobile) against offline guessing                                                              |
| Your sync storage provider or anyone with access to it | Ciphertext only; per-device hash chains detect forged, reordered, rolled-back or dropped data — see [Sync protocol](/security/sync)                             |
| A network attacker (man-in-the-middle)                 | Host-key verification on every hop, `@revoked` and CA support, strict KEX (Terrapin mitigation) — see [Host-key policy](/security/host-keys)                    |
| A future quantum adversary recording traffic           | `mlkem768x25519-sha256` hybrid key exchange preferred by default                                                                                                |
| A malicious or compromised SSH server                  | Read-only in-process agent, validated SFTP file names, no symlink following on delete, OSC 52 clipboard writes need consent                                     |
| A compromised dependency in the web UI                 | Secrets never reach the UI; Tauri isolation pattern, strict CSP, frozen prototypes, least-privilege capabilities — see [App isolation](/security/app-isolation) |
| Malicious import files                                 | Size limits, bounded nesting, panic-free parsers, validation of every entity                                                                                    |
| Prompt injection into the local AI                     | Remote text framed as data, grammar-constrained output, validation, never auto-executed                                                                         |
| A tampered download                                    | SHA-256 checksums, Sigstore signatures, build-provenance attestations, SBOM — see [Supply chain](/security/supply-chain)                                        |

## Out of scope

- Vulnerabilities in the SSH servers you connect to.
- A fully compromised operating system, or a malicious application already running with the same privileges as ssh-client. Such an attacker can read process memory while the vault is unlocked.
- Denial of service by the user against their own device.

## Hardening in the Rust core

- `unsafe` code is denied across the workspace.
- Pure-Rust cryptography (`ring` and RustCrypto); no OpenSSL.
- Secret material is held in zeroizing buffers and redacted from `Debug` output and logs.
- Parsers for untrusted input are written not to panic, use bounded memory, and are covered by property-based tests.

## Read more

- [Vault cryptography](/security/vault-crypto)
- [Sync protocol](/security/sync)
- [Host-key policy](/security/host-keys)
- [App isolation](/security/app-isolation)
- [Supply chain](/security/supply-chain)
- [Reporting a vulnerability](/security/disclosure)
