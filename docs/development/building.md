---
title: Build from source
description: Prerequisites and commands to build ssh-client for desktop and mobile from source, run the tests and work on this documentation site.
---

# Build from source

## Prerequisites

| Tool                              | Version                                                             |
| --------------------------------- | ------------------------------------------------------------------- |
| Rust                              | pinned in `rust-toolchain.toml` (installed automatically by rustup) |
| Node.js                           | 22.12 or newer                                                      |
| pnpm                              | 10 (`corepack enable`)                                              |
| Docker                            | for the protocol integration tests                                  |
| CMake, a C/C++ compiler, libclang | for the on-device AI engine (llama.cpp), enabled by default         |

Platform build dependencies follow the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). On Debian or Ubuntu:

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev \
  libssl-dev libayatana-appindicator3-dev librsvg2-dev libudev-dev
```

For mobile targets you additionally need Android Studio (SDK and NDK) and, on macOS, Xcode.

## Get the code and run it

```bash
git clone https://github.com/davidniessen/ssh-client.git
cd ssh-client
pnpm install
pnpm dev            # desktop app with hot reload
```

Mobile development builds:

```bash
pnpm --filter @ssh-client/desktop tauri android dev
pnpm --filter @ssh-client/desktop tauri ios dev
```

A release build for your platform:

```bash
pnpm build          # = pnpm --filter @ssh-client/desktop tauri build
```

The UI also runs in a plain browser (`pnpm --filter @ssh-client/desktop dev`) against a mock backend, which is handy for UI work and screenshots.

### Without local AI

The `local-ai` cargo feature (default) compiles llama.cpp from source. To build without it — for example when CMake is not available — disable the app's default features:

```bash
cargo build -p ssh-client --no-default-features
```

The app then reports local AI as unavailable; everything else works. On Apple silicon, the `metal` feature enables GPU offload.

## Quality gates

All of these run in CI and must pass before a merge:

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

Protocol integration tests use throw-away OpenSSH servers in Docker:

```bash
docker compose -f docker/test-servers/compose.yml up -d --build
cargo test -p ssh-engine --features integration -- --test-threads=1
cargo test -p sftp-engine --features integration
```

## This website

The site lives in `docs/` and is built with [VitePress](https://vitepress.dev/):

```bash
pnpm --filter @ssh-client/docs dev       # live preview
pnpm --filter @ssh-client/docs build     # static output in docs/.vitepress/dist
pnpm --filter @ssh-client/docs preview   # serve the build
```

The build also writes `llms.txt`, `llms-full.txt`, `robots.txt` and a Markdown copy of every page. Set `DOCS_BASE=/` to build for a custom domain instead of `/ssh-client/`.
