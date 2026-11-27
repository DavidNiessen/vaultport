---
title: Release process
description: How ssh-client is versioned with release-please and built, signed, attested and published by the release workflow, including every repository secret it reads and the updater setup.
---

# Release process

Releases are fully automated by GitHub Actions. This page describes the workflows in `.github/workflows/` as they are.

## Workflows at a glance

| Workflow                      | Trigger                                                              | What it does                                                                                                                                                                                          |
| ----------------------------- | -------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ci.yml`                      | pushes to `main`, pull requests, merge queue                         | Prettier, ESLint, `tsc`, Vitest, Playwright (mocked core), `cargo fmt`/`clippy`/`test`, generated bindings check, `cargo deny`, `pnpm audit`, gitleaks, dependency review, Conventional Commits check |
| `build.yml`                   | pushes to `main`, app pull requests                                  | Unsigned test bundles (Linux x64, Windows x64, macOS arm64) and a debug Android APK as workflow artifacts (kept 7 days)                                                                               |
| `release-please.yml`          | pushes to `main`, manual                                             | Maintains the release pull request; when it is merged, tags the release and calls `release.yml`                                                                                                       |
| `release.yml`                 | called by release-please, or manual with an existing tag             | Builds, signs and uploads all platforms, then publishes checksums, signature, SBOM and attestations                                                                                                   |
| `docs.yml`                    | `docs/` changes on `main`, pull requests, releases, manual, callable | Builds this website and deploys it to GitHub Pages                                                                                                                                                    |
| `integration.yml`             | protocol pull requests, nightly, manual                              | SSH/SFTP engine tests against OpenSSH in Docker, Telnet and Mosh protocol tests                                                                                                                       |
| `fuzz.yml`                    | parser pull requests, nightly, manual                                | `cargo-fuzz` targets for the parsers                                                                                                                                                                  |
| `codeql.yml`, `scorecard.yml` | pushes, pull requests (CodeQL) and weekly                            | Static analysis and OpenSSF Scorecard                                                                                                                                                                 |

## Versioning with release-please

[release-please](https://github.com/googleapis/release-please) reads the [Conventional Commits](/development/contributing#commit-messages) on `main` and keeps a release pull request open with the next version and a generated `CHANGELOG.md`.

- Configuration: `release-please-config.json` (release type `simple`, tags `v<version>`), current version in `.release-please-manifest.json`.
- Before 1.0, `feat` commits bump the minor version (`bump-minor-pre-major`) and everything else the patch version.
- The version is written to the root `Cargo.toml` (`workspace.package.version`), `apps/desktop/package.json` (which `tauri.conf.json` reads) and `docs/package.json`.
- Changelog sections: Features, Bug Fixes, Performance, Security and Documentation; other types are hidden.

Merging the release pull request creates the tag and GitHub release, and the `release` job of `release-please.yml` calls `release.yml` with the tag and version.

To rebuild an existing tag by hand, run **Actions → Release → Run workflow** with the tag (for example `v0.1.0`) and the version (`0.1.0`).

## What `release.yml` builds

| Job                                        | Runner             | Output                                                                                                                                                                          |
| ------------------------------------------ | ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Desktop (Linux x64)                        | `ubuntu-24.04`     | `ssh-client_<v>_amd64.AppImage`, `ssh-client_<v>_amd64.deb`, `ssh-client-<v>-1.x86_64.rpm`                                                                                      |
| Desktop (Linux arm64)                      | `ubuntu-24.04-arm` | `ssh-client_<v>_aarch64.AppImage`, `ssh-client_<v>_arm64.deb`, `ssh-client-<v>-1.aarch64.rpm`                                                                                   |
| Desktop (Windows x64)                      | `windows-2025`     | `ssh-client_<v>_x64-setup.exe`, `ssh-client_<v>_x64_en-US.msi`, `ssh-client_<v>_x64_de-DE.msi`                                                                                  |
| Desktop (Windows arm64)                    | `windows-11-arm`   | `ssh-client_<v>_arm64-setup.exe`, `ssh-client_<v>_arm64_en-US.msi`, `ssh-client_<v>_arm64_de-DE.msi`                                                                            |
| Desktop (macOS universal)                  | `macos-15`         | `ssh-client_<v>_universal.dmg` (built with Metal GPU support for local AI)                                                                                                      |
| Android                                    | `ubuntu-24.04`     | `ssh-client_<v>_android-universal.apk`, `ssh-client_<v>_android-universal.aab` (built without local AI)                                                                         |
| iOS                                        | `macos-15`         | `ssh-client_<v>_ios.ipa` (App Store Connect export) — only when iOS signing is configured; otherwise an unsigned simulator build is compiled as a check and nothing is uploaded |
| Checksums, SBOM, signatures and provenance | `ubuntu-24.04`     | `SHA256SUMS.txt`, `SHA256SUMS.txt.sigstore.json`, `sbom.spdx.json`, build-provenance attestations                                                                               |

The desktop jobs use [`tauri-action`](https://github.com/tauri-apps/tauri-action), which uploads the bundles to the release under the names above. When updater signing is configured they also produce updater bundles with `.sig` files (for example `ssh-client_universal.app.tar.gz.sig`) and a `latest.json` manifest; the Windows entries in `latest.json` point to the NSIS installer.

The final job runs once the desktop builds succeed (mobile failures do not block it). It downloads every release asset, then:

1. generates an SPDX **SBOM** of the source tree with Syft (`anchore/sbom-action`) → `sbom.spdx.json`;
2. writes **`SHA256SUMS.txt`** for every asset except `latest.json`;
3. signs `SHA256SUMS.txt` **keylessly with cosign**, using the workflow's GitHub OIDC identity → `SHA256SUMS.txt.sigstore.json`;
4. creates a **build-provenance attestation** for every file listed in `SHA256SUMS.txt` (`actions/attest-build-provenance`);
5. uploads the checksums, the signature bundle and the SBOM to the release.

The [download page](/download) recognizes exactly these file names, so keep them stable. Users verify a download as described in [Verify your download](/download#verify-your-download).

## Repository secrets

Every secret is optional: without it the corresponding signing step is skipped and the artifacts are published unsigned (checksums, Sigstore signature, SBOM and attestations are always produced). Configure them under **Settings → Secrets and variables → Actions**; `release-please.yml` passes them to `release.yml` with `secrets: inherit`.

| Secret                                                                                       | Used by             | Purpose                                                                                        |
| -------------------------------------------------------------------------------------------- | ------------------- | ---------------------------------------------------------------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`                            | desktop jobs        | Sign updater bundles (minisign key from `tauri signer generate`)                               |
| `TAURI_UPDATER_PUBKEY`                                                                       | desktop jobs        | The matching public key, injected into the app at build time (see below)                       |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`                                            | macOS and iOS jobs  | Base64-encoded `.p12` signing certificate and its password                                     |
| `APPLE_SIGNING_IDENTITY`                                                                     | macOS job           | The certificate's identity, for example `Developer ID Application: … (TEAMID)`                 |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`                                                | macOS job (and iOS) | Notarization with an app-specific password; `APPLE_TEAM_ID` also enables the iOS build         |
| `IOS_PROVISIONING_PROFILE`                                                                   | iOS job             | Provisioning profile for the App Store Connect export; with `APPLE_TEAM_ID` it enables the IPA |
| `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD`                                        | Windows jobs        | Base64-encoded `.pfx` for Authenticode signing (SHA-256, DigiCert timestamp)                   |
| `ANDROID_KEYSTORE`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` | Android job         | Base64-encoded release keystore and its credentials                                            |

No secret is needed for Sigstore or attestations; the jobs request `id-token: write` and `attestations: write`.

::: warning Android without a keystore
Android only installs signed APKs. Without the `ANDROID_KEYSTORE*` secrets the job uploads an unsigned release APK, which phones refuse to install. Configure the keystore before the first public release, and keep it safe: updates must be signed with the same key.
:::

## Setting up the self-updater

`apps/desktop/src-tauri/tauri.conf.json` already points the updater at

```text
https://github.com/davidniessen/ssh-client/releases/latest/download/latest.json
```

with an empty `pubkey` and `bundle.createUpdaterArtifacts: false`. That is deliberate: the release workflow turns updater artifacts on and injects the public key at build time, but only when both `TAURI_UPDATER_PUBKEY` and `TAURI_SIGNING_PRIVATE_KEY` are set. To enable updates:

1. Generate a key pair once, on a trusted machine:

   ```bash
   pnpm --filter @ssh-client/desktop tauri signer generate -w ~/.tauri/ssh-client.key
   ```

2. Store the **private key** file's content as `TAURI_SIGNING_PRIVATE_KEY` and its password as `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
3. Store the **public key** (the content of `~/.tauri/ssh-client.key.pub`) as `TAURI_UPDATER_PUBKEY`. Alternatively commit it to `plugins.updater.pubkey` in `tauri.conf.json`; it is not secret.
4. Back up the private key offline. If it is lost, installed apps can no longer verify updates and users must reinstall manually.

Releases built without these secrets have no `latest.json`, and **Check for updates** reports that no update is available.

## Publishing the website

`docs.yml` deploys this site to GitHub Pages on pushes to `main` that touch `docs/`, on manual runs and on `release: published`. Releases created by release-please with the default `GITHUB_TOKEN` do not trigger other workflows, so the workflow can also be called from another workflow (`workflow_call`) — for example as a job after `release` in `release-please.yml`. The download page fetches release data live in the browser, so it is current even without a redeploy.

Enable Pages once under **Settings → Pages → Source: GitHub Actions**.

## Release checklist

1. Review and merge the release-please pull request.
2. Watch the **Release Please** run: all desktop jobs must pass; check the Android and iOS jobs.
3. Confirm the release lists the installers, `SHA256SUMS.txt`, `SHA256SUMS.txt.sigstore.json` and `sbom.spdx.json` (and `latest.json` if updater signing is configured).
4. Spot-check one artifact per platform with the [verification commands](/download#verify-your-download).
