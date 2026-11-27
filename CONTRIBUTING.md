# Contributing to Vaultport

Thanks for helping build a secure, serverless SSH client! This guide covers everything you
need to get a development build running and a pull request merged.

## Ground rules

- Read the [Code of Conduct](CODE_OF_CONDUCT.md).
- Security issues go through [SECURITY.md](SECURITY.md), never public issues.
- Every feature must work **without a server operated by this project**.
- Secrets (keys, passwords, tokens) must never cross the IPC boundary into the web UI.

## Prerequisites

| Tool    | Version                                                             |
| ------- | ------------------------------------------------------------------- |
| Rust    | pinned in `rust-toolchain.toml` (installed automatically by rustup) |
| Node.js | 22.12 or newer                                                      |
| pnpm    | 10 (`corepack enable`)                                              |
| Docker  | for the protocol integration tests                                  |

Platform build dependencies follow the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). On Debian/Ubuntu:

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev \
  libssl-dev libayatana-appindicator3-dev librsvg2-dev libudev-dev
```

For mobile targets you additionally need Android Studio (SDK + NDK) and, on macOS, Xcode.

## Getting started

```bash
pnpm install
pnpm dev            # desktop app with hot reload
pnpm --filter @vaultport/desktop tauri android dev
pnpm --filter @vaultport/desktop tauri ios dev
```

## Quality gates

All of these run in CI and must pass before merge:

```bash
pnpm format:check   # prettier
pnpm lint           # eslint (zero warnings)
pnpm typecheck      # tsc --noEmit
pnpm test           # vitest
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
```

Protocol integration tests need the Docker fixtures:

```bash
docker compose -f docker/test-servers/compose.yml up -d --build
cargo test -p ssh-engine --features integration -- --test-threads=1
```

## Commit messages

We use [Conventional Commits](https://www.conventionalcommits.org/). Releases, version bumps
and the changelog are generated from them by release-please.

```text
feat(sftp): resume interrupted uploads
fix(vault): reject records with mismatched AAD
docs: describe sync pairing flow
```

Allowed types: `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `build`, `ci`, `chore`,
`style`, `revert`. Breaking changes use `!` (e.g. `feat(vault)!: ...`) and a
`BREAKING CHANGE:` footer.

## Developer Certificate of Origin

By contributing you certify the [DCO](https://developercertificate.org/). Sign off each
commit with `git commit -s`.

## Pull requests

1. Fork and create a topic branch.
2. Keep PRs focused; one logical change per PR.
3. Add tests. Parsers and decoders also need a fuzz target in `fuzz/`.
4. Update docs and `docs/` user guide when behaviour changes.
5. Fill in the PR template.

## Project layout

See [docs/architecture.md](docs/architecture.md) for a tour of the crates and the UI.

## License

By contributing, you agree that your contributions are licensed under GPL-3.0-or-later.
