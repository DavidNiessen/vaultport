---
title: Supply chain
description: How ssh-client keeps its dependencies and releases trustworthy — license and advisory checks, pinned toolchains and actions, signed and attested releases with SBOMs.
---

# Supply chain

## Dependencies

- **Rust:** [`cargo-deny`](https://embarkstudios.github.io/cargo-deny/) runs in CI and rejects crates with known security advisories, yanked versions, licenses that are not compatible with GPL-3.0-or-later, wildcard version requirements, unknown registries and any Git dependency. OpenSSL (`openssl-sys`) is banned outright; TLS uses rustls.
- **JavaScript:** dependencies are locked with `pnpm-lock.yaml`; only explicitly allowed packages may run install scripts.
- **Updates:** Dependabot proposes Cargo, npm, GitHub Actions and Gradle updates weekly or monthly, grouped so they can be reviewed together.
- **Toolchain:** the Rust version is pinned in `rust-toolchain.toml`; Node.js and pnpm versions are pinned in `package.json`.
- **CI actions** are pinned by full commit SHA, not by mutable tags.
- **Licensing:** the repository follows the [REUSE](https://reuse.software/) specification; every source file carries an SPDX license identifier.

## Code

- `unsafe` Rust is denied across the workspace.
- Clippy runs in pedantic mode with warnings as errors; `unwrap()` is flagged.
- Protocol engines are tested against real OpenSSH servers in Docker; parsers have property-based tests.

## Optional downloads at runtime

The only file ssh-client downloads on its own initiative is the [local AI model](/guide/local-ai), and only after you ask for it. Each model is pinned to an immutable Hugging Face commit with its exact size and SHA-256; anything else is rejected. Updating a model is a reviewed code change.

## Releases

Every release is built by GitHub Actions from a tagged commit. Each artifact ships with:

| What                        | How to check it                                                               |
| --------------------------- | ----------------------------------------------------------------------------- |
| SHA-256 checksum            | `sha256sum -c SHA256SUMS.txt --ignore-missing`                                |
| GitHub build provenance     | `gh attestation verify <file> --repo davidniessen/ssh-client`                 |
| Sigstore (cosign) signature | `cosign verify-blob <file> --signature <file>.sig --certificate <file>.pem …` |
| SBOM                        | `sbom.spdx.json` (SPDX) attached to the release                               |

The full commands are on the [download page](/download#verify-your-download). The desktop self-updater additionally verifies each update's signature against the project's public key before installing it.

See also the [release process](/development/releasing).
