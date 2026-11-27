// SPDX-License-Identifier: GPL-3.0-or-later
//! Optional, fully local natural-language → shell command generation.
//!
//! This first part provides the pinned model [`catalog`] and the verifying, resumable
//! [`download`]er.
//!
//! # Privacy
//!
//! The only network traffic is the one-time, user-initiated model download: anonymous HTTPS
//! `GET`s to Hugging Face (and the CDN hosts it redirects to), with a generic `User-Agent` and
//! no cookies or tokens. No API keys, no servers operated by this project, no telemetry.

#![forbid(unsafe_code)]

pub mod catalog;
pub mod download;
mod error;

pub use catalog::{MODELS, ModelEntry, Tier, default_model};
pub use download::{DownloadSpec, Downloader, Progress, UrlPolicy};
pub use error::{Error, Result};
pub use tokio_util::sync::CancellationToken;
