# Security Policy

ssh-client handles private keys, passwords and access to production infrastructure. We treat
security reports as our highest priority.

## Supported versions

| Version        | Supported                                             |
| -------------- | ----------------------------------------------------- |
| Latest release | :white_check_mark:                                    |
| Previous minor | :white_check_mark: (security fixes only, for 90 days) |
| Anything older | :x:                                                   |

## Reporting a vulnerability

**Please do not open a public issue.**

Report vulnerabilities privately through
[GitHub Security Advisories](https://github.com/davidniessen/ssh-client/security/advisories/new).
Include:

- affected version(s) and platform(s),
- a description of the issue and its impact,
- steps to reproduce or a proof of concept,
- any suggested mitigation.

We will acknowledge your report within **72 hours**, provide an initial assessment within
**7 days**, and aim to ship a fix within **30 days** for high and critical issues. We will keep
you informed and credit you in the advisory unless you prefer to stay anonymous.

## Scope

In scope:

- the desktop and mobile applications in this repository,
- the Rust crates under `crates/` and the native plugins under `plugins/`,
- the release pipeline (build provenance, signing, updater manifests).

Out of scope:

- vulnerabilities in SSH servers you connect to,
- issues that require a fully compromised operating system or a malicious, already-installed
  application with the same privileges as ssh-client,
- denial of service by the user against their own device.

## Security design

The threat model and cryptographic design are documented in
[`docs/security/`](docs/security/). Highlights:

- **No servers.** ssh-client has no backend, no accounts and no telemetry.
- **Secrets never leave the Rust core.** The UI only handles opaque identifiers.
- **Vault encryption:** XChaCha20-Poly1305 per record, keys wrapped with Argon2id-derived keys
  and/or the OS key store (Keychain, DPAPI/Credential Manager, Secret Service, Android Keystore).
- **Strict host key verification** by default, post-quantum key exchange preferred.
- **Signed, attested releases** with SBOMs and build provenance.

## Verifying releases

Every release artifact ships with a SHA-256 checksum, a Sigstore (cosign) signature and a
GitHub build-provenance attestation:

```bash
gh attestation verify <artifact> --repo davidniessen/ssh-client
```
