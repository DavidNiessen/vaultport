// SPDX-License-Identifier: GPL-3.0-or-later
//! Domain model for ssh-client.
//!
//! The model mirrors how people think about infrastructure: **groups** (nestable, not folders)
//! contain **hosts**; both carry a sparse [`HostConfig`] whose fields are inherited per
//! parameter from the closest ancestor that sets them. Credentials live in the keychain
//! ([`Identity`], [`KeyMeta`]) and are *referenced* by id, never copied.
//!
//! This crate is pure data + logic: no I/O, no crypto, no secrets. Secret material is stored by
//! the `vault` crate and referenced through [`SecretRef`].

#![forbid(unsafe_code)]

mod config;
mod entities;
mod error;
mod id;
mod omnibox;
mod resolve;
mod validate;

pub use config::*;
pub use entities::*;
pub use error::ModelError;
pub use id::*;
pub use omnibox::{QuickConnect, parse_quick_connect};
pub use resolve::{ResolvedConfig, Resolver};
pub use validate::{Validate, validate_hostname, validate_label, validate_port};

#[cfg(feature = "specta")]
pub use specta;
