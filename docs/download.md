---
layout: home
markdownStyles: false
title: Download
description: Download ssh-client for Windows, macOS, Linux, iOS, iPadOS and Android, and verify the files with SHA-256 checksums, GitHub attestations and Sigstore signatures.
---

<DownloadPage />

<div class="vp-doc download-doc">

## Verify your download

Every release is built in GitHub Actions from a tagged commit. Each file is published with a SHA-256 checksum in `SHA256SUMS.txt`, a Sigstore (cosign) signature and a GitHub build-provenance attestation, and the release carries an SPDX SBOM (`sbom.spdx.json`). All of them are on the [releases page](https://github.com/davidniessen/ssh-client/releases).

### 1. Check the SHA-256 checksum

Download `SHA256SUMS.txt` into the same folder as the installer, then:

```bash
# Linux
sha256sum -c SHA256SUMS.txt --ignore-missing

# macOS
shasum -a 256 -c SHA256SUMS.txt --ignore-missing
```

```powershell
# Windows (PowerShell): compare the output with the line in SHA256SUMS.txt
Get-FileHash .\ssh-client_<version>_x64-setup.exe -Algorithm SHA256
```

### 2. Verify the build provenance

With the [GitHub CLI](https://cli.github.com/), confirm that the file was built by this repository's release workflow:

```bash
gh attestation verify ssh-client_<version>_amd64.AppImage --repo davidniessen/ssh-client
```

### 3. Verify the Sigstore signature

Signatures are keyless: they are bound to the release workflow's GitHub identity instead of a long-lived key. With [cosign](https://docs.sigstore.dev/cosign/system_config/installation/):

```bash
cosign verify-blob ssh-client_<version>_amd64.AppImage \
  --signature ssh-client_<version>_amd64.AppImage.sig \
  --certificate ssh-client_<version>_amd64.AppImage.pem \
  --certificate-identity-regexp '^https://github\.com/davidniessen/ssh-client/' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

If a release ships a Sigstore bundle (`*.sigstore.json`) instead of separate `.sig` and `.pem` files, pass it with `--bundle <file>` instead of `--signature` and `--certificate`.

### Updates

Desktop builds can update themselves from GitHub Releases. Updates are only checked when you click **Settings → About → Check for updates**, and each update is verified against the project's signing key before it is installed. On phones and tablets, install the new APK or IPA over the old one; your vault is kept.

### Prefer to build it yourself?

Everything needed to build ssh-client from source is in the repository. See [Build from source](/development/building).

</div>
