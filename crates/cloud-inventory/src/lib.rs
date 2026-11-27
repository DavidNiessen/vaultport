// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only cloud inventory clients that call provider APIs directly from the device.

#![forbid(unsafe_code)]

mod error;
pub mod http;
pub mod sigv4;

pub use error::InventoryError;
