---
layout: home
markdownStyles: false
title: Download
description: Download Vaultport for Windows, macOS, Linux, iOS, iPadOS and Android, and verify the files with SHA-256 checksums, GitHub attestations and Sigstore signatures.
---

<DownloadPage />

<div class="vp-doc download-doc">

## Verify your download

Every release is built by the [release workflow](https://github.com/DavidNiessen/vaultport/blob/main/.github/workflows/release.yml) in GitHub Actions from a tagged commit. Next to the installers, each release has:

| File                           | What it is                                                                            |
| ------------------------------ | ------------------------------------------------------------------------------------- |
| `SHA256SUMS.txt`               | SHA-256 checksums of every file in the release                                        |
| `SHA256SUMS.txt.sigstore.json` | Keyless Sigstore (cosign) signature of `SHA256SUMS.txt`, made by the release workflow |
| `sbom.spdx.json`               | Software bill of materials (SPDX) of the source tree                                  |
| `*.sig`, `latest.json`         | Signatures and manifest for the desktop self-updater (when updater signing is set up) |

Every file listed in `SHA256SUMS.txt` also has a GitHub build-provenance attestation. All of them are on the [releases page](https://github.com/DavidNiessen/vaultport/releases).

### 1. Verify the checksum file's signature

The signature is keyless: it is bound to the release workflow's GitHub identity, not to a long-lived key. With [cosign](https://docs.sigstore.dev/cosign/system_config/installation/) 2.x, download `SHA256SUMS.txt` and `SHA256SUMS.txt.sigstore.json` and run:

```bash
cosign verify-blob SHA256SUMS.txt \
  --bundle SHA256SUMS.txt.sigstore.json \
  --certificate-identity-regexp '^https://github\.com/DavidNiessen/vaultport/\.github/workflows/release\.yml@' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

### 2. Check your file against the checksums

Put `SHA256SUMS.txt` in the same folder as the installer, then:

```bash
# Linux
sha256sum -c SHA256SUMS.txt --ignore-missing

# macOS
shasum -a 256 -c SHA256SUMS.txt --ignore-missing
```

```powershell
# Windows (PowerShell): compare the output with the line in SHA256SUMS.txt
Get-FileHash .\vaultport_<version>_x64-setup.exe -Algorithm SHA256
```

Together, steps 1 and 2 prove that your file is exactly what the release workflow published.

### 3. Verify the build provenance (optional)

With the [GitHub CLI](https://cli.github.com/), confirm directly that a file was built in this repository's GitHub Actions:

```bash
gh attestation verify vaultport_<version>_amd64.AppImage --repo DavidNiessen/vaultport
```

### Updates

Desktop builds include a self-updater that reads `latest.json` from the latest GitHub release. It only checks when you click **Settings → About → Check for updates**, and it verifies each update's signature against the public key built into the app before installing it. On phones and tablets, install the new APK or IPA over the old one; your vault is kept.

### Prefer to build it yourself?

Everything needed to build Vaultport from source is in the repository. See [Build from source](/development/building).

</div>
