---
title: Release process
description: How ssh-client releases are versioned, built, signed, attested and published, and which repository secrets and settings the pipeline needs.
---

# Release process

## Versioning

Versions follow [Semantic Versioning](https://semver.org/) and are derived from [Conventional Commits](/development/contributing#commit-messages) by **release-please**: merged `feat` commits bump the minor version, `fix` and `perf` the patch version, and `!` / `BREAKING CHANGE:` the major version. release-please keeps a release pull request open with the version bump and the generated changelog; merging it tags the release.

The workspace version lives in the root `Cargo.toml` (`[workspace.package] version`) and `apps/desktop/package.json`; `tauri.conf.json` reads the app version from `package.json`.

## Build matrix

Tagged releases are built on GitHub-hosted runners with [`tauri-action`](https://github.com/tauri-apps/tauri-action). The expected artifacts:

| Platform | Files                                                                                                                        |
| -------- | ---------------------------------------------------------------------------------------------------------------------------- |
| Windows  | `ssh-client_<v>_x64-setup.exe`, `ssh-client_<v>_x64_en-US.msi`, `…_arm64-setup.exe`                                          |
| macOS    | `ssh-client_<v>_universal.dmg`                                                                                               |
| Linux    | `ssh-client_<v>_amd64.AppImage`, `ssh-client_<v>_amd64.deb`, `ssh-client-<v>-1.x86_64.rpm`, and `aarch64` / `arm64` variants |
| Android  | `ssh-client_<v>_android-universal.apk`                                                                                       |
| iOS      | `ssh-client_<v>_ios.ipa`                                                                                                     |

Alongside them, each release carries `SHA256SUMS.txt`, Sigstore signatures (`*.sig`), GitHub build-provenance attestations and an SPDX SBOM (`sbom.spdx.json`). The [download page](/download) recognizes these names automatically, so keep them stable.

## Signing and attestation

- **Checksums:** `SHA256SUMS.txt` covers every artifact.
- **Sigstore:** artifacts are signed keyless with `cosign`, using the workflow's GitHub OIDC identity — no signing key to manage.
- **Provenance:** `actions/attest-build-provenance` records how and from which commit each file was built (`gh attestation verify`).
- **Updater:** desktop self-updates are verified with the Tauri updater's minisign key; the private key signs the update bundles in CI, the public key is compiled into the app.
- **Platform signing:** with the secrets below configured, macOS builds are code-signed and notarized, Windows installers are Authenticode-signed, and Android and iOS packages are signed with the project's keys.

## Required secrets

Configure these as repository (or `release` environment) secrets. The names follow the conventions of `tauri-action` and the Tauri mobile tooling; check `.github/workflows/` for the exact set your workflow version uses.

| Secret                                                                                       | Used for                                                         |
| -------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`                            | Signing updater bundles (`tauri signer generate`)                |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`                                            | Developer ID certificate (base64 `.p12`) for macOS               |
| `APPLE_SIGNING_IDENTITY`                                                                     | The certificate's identity name                                  |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`                                                | Notarization (app-specific password)                             |
| `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD`                                        | Optional Authenticode signing (base64 `.pfx`)                    |
| `ANDROID_KEYSTORE`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` | Signing the APK (base64 keystore)                                |
| `IOS_CERTIFICATE`, `IOS_CERTIFICATE_PASSWORD`, `IOS_MOBILE_PROVISION`                        | Signing the IPA (distribution certificate, provisioning profile) |

Not secrets, but required permissions: the release job needs `contents: write` (upload assets), `id-token: write` (Sigstore and attestations) and `attestations: write`. If release-please should trigger CI on its release PRs, give it a GitHub App or fine-grained token instead of the default `GITHUB_TOKEN`.

::: warning Updater configuration
Self-updates only work once the updater public key and endpoint are configured under `plugins.updater` in `apps/desktop/src-tauri/tauri.conf.json` and `bundle.createUpdaterArtifacts` is enabled. Never commit the private key.
:::

## The website

The website is rebuilt and deployed to GitHub Pages by `.github/workflows/docs.yml` on every push to `main` that touches `docs/`, on demand, and whenever a release is **published**, so the download page and its static fallbacks stay current. Pull requests that touch `docs/` get a build-only check. Enable Pages with the **GitHub Actions** source in the repository settings.

## Checklist

1. Merge the release-please pull request.
2. Wait for the release workflow to attach all artifacts, checksums, signatures, attestations and the SBOM.
3. Spot-check one artifact per platform with the [verification commands](/download#verify-your-download).
4. Publish the release (if it was created as a draft). The docs workflow redeploys the site.
