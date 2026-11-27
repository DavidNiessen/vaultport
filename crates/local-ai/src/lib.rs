// SPDX-License-Identifier: GPL-3.0-or-later
//! Optional, fully local natural-language → shell command generation.
//!
//! The user flips one switch ("Enable local command generation"). The application then uses
//! this crate to
//!
//! 1. pick the default model for the platform from a **pinned catalog** ([`catalog`]):
//!    Qwen2.5-Coder 1.5B Instruct on desktops, 0.5B on Android/iOS (both Apache-2.0, Q4_K_M
//!    GGUF builds from the official `Qwen/…-GGUF` repositories),
//! 2. **download** it once over HTTPS from an allow-listed host, resumable, with SHA-256 and
//!    size verification ([`download`]),
//! 3. **load** it on-device with llama.cpp ([`engine`], cargo feature `llama`), and
//! 4. turn requests such as "list files sorted by size" plus the session context (remote OS,
//!    shell, cwd, recent commands, selection) into **one** validated command with a short
//!    explanation, or into a multi-line snippet.
//!
//! AI is never used for autocomplete; that is a separate, deterministic engine. Suggestions are
//! only ever inserted into the prompt for the user to review — this crate executes nothing.
//!
//! # Privacy
//!
//! * **No data leaves the device for inference.** Prompts, terminal context and generated
//!   commands stay in process memory; nothing is logged at `info` level or above, and nothing
//!   is persisted by this crate.
//! * The **only network traffic** is the one-time, user-initiated model download: anonymous
//!   HTTPS `GET`s to Hugging Face (and the CDN hosts it redirects to), with a generic
//!   `User-Agent` and no cookies or tokens. There are no API keys, no servers operated by this
//!   project, no telemetry, no update pings.
//!
//! # Security
//!
//! * Pinned revision + SHA-256 + size for every model file; verification fails closed.
//! * HTTPS-only download with a host allow-list that is re-checked on every redirect hop.
//! * Model output is grammar-constrained (GBNF) to a fixed JSON shape and then validated:
//!   control characters, NUL, bidi overrides, multiple lines (in single-command mode) and
//!   oversized output are rejected ([`prompt`]).
//! * Context from the remote host is treated as untrusted data: sanitized, size-capped and
//!   framed as data in the prompt.
//!
//! # Cargo features
//!
//! * `llama` (default): on-device inference via llama.cpp (built from source; needs CMake and a
//!   C/C++ toolchain, plus libclang for bindgen). Without it the API stays the same and
//!   [`Engine::load`] returns [`Error::Unavailable`].
//! * `metal`: Apple GPU offload (enabled automatically on Apple Silicon macOS).
//! * `vulkan`: Vulkan GPU offload (needs the Vulkan SDK / `glslc` at build time).

#![forbid(unsafe_code)]

pub mod catalog;
pub mod download;
pub mod engine;
mod error;
pub mod prompt;

pub use catalog::{MODELS, ModelEntry, Tier, default_model};
pub use download::{DownloadSpec, Downloader, Progress, UrlPolicy};
pub use engine::{Engine, EngineOptions, is_available};
pub use error::{Error, Result};
pub use prompt::{CommandContext, GenerateRequest, Snippet, SnippetRequest, Suggestion};
pub use tokio_util::sync::CancellationToken;
