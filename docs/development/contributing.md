---
title: Contributing
description: Ground rules, commit conventions, the Developer Certificate of Origin and the pull-request checklist for contributing to ssh-client.
---

# Contributing

Thanks for helping build a secure, serverless SSH client! The canonical guide is [`CONTRIBUTING.md`](https://github.com/davidniessen/ssh-client/blob/main/CONTRIBUTING.md); this page summarizes it.

## Ground rules

- Follow the [Code of Conduct](https://github.com/davidniessen/ssh-client/blob/main/CODE_OF_CONDUCT.md).
- Security issues go through the [private reporting process](/security/disclosure), never public issues.
- Every feature must work **without a server operated by this project**.
- Secrets (keys, passwords, tokens) must never cross the IPC boundary into the web UI.
- No telemetry. The app only talks to hosts the user configured, plus clearly disclosed, optional endpoints (update checks, model download).
- Insecure options exist only as explicit, per-host opt-ins.

## Workflow

1. Fork the repository and create a topic branch.
2. Set up your environment as described in [Build from source](/development/building).
3. Keep pull requests focused: one logical change per PR.
4. Add tests. New parsers and decoders also need a fuzz target.
5. Update the documentation in `docs/` when behaviour changes.
6. Fill in the pull-request template, including its security checklist.

All [quality gates](/development/building#quality-gates) must pass in CI.

## Commit messages

ssh-client uses [Conventional Commits](https://www.conventionalcommits.org/). Releases, version bumps and the changelog are generated from them.

```text
feat(sftp): resume interrupted uploads
fix(vault): reject records with mismatched AAD
docs: describe sync pairing flow
```

Allowed types: `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `build`, `ci`, `chore`, `style`, `revert`. Breaking changes use `!` (for example `feat(vault)!: …`) and a `BREAKING CHANGE:` footer.

## Developer Certificate of Origin

By contributing you certify the [DCO](https://developercertificate.org/). Sign off each commit with `git commit -s`.

## Licensing

ssh-client is licensed under **GPL-3.0-or-later**, and by contributing you agree that your contributions are licensed the same way. Source files carry an SPDX header:

```ts
// SPDX-License-Identifier: GPL-3.0-or-later
```

New dependencies must use a license compatible with GPL-3.0-or-later; `cargo deny` enforces this for Rust crates.

## Governance

Maintainers review and merge pull requests, triage issues, cut releases and handle security reports. Larger changes — new protocols, storage format changes, cryptography, new network-facing features — start as an Architecture Decision Record and need maintainer approval. See [`GOVERNANCE.md`](https://github.com/davidniessen/ssh-client/blob/main/GOVERNANCE.md).
