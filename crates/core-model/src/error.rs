// SPDX-License-Identifier: GPL-3.0-or-later
use thiserror::Error;

/// Errors produced by model validation and resolution.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ModelError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} is too long (max {max} characters)")]
    TooLong { field: &'static str, max: usize },
    #[error("invalid hostname or address: {0}")]
    InvalidHost(String),
    #[error("invalid port: {0}")]
    InvalidPort(String),
    #[error("invalid value for {field}: {reason}")]
    Invalid { field: &'static str, reason: String },
    #[error("group hierarchy contains a cycle")]
    GroupCycle,
    #[error("host chain contains a cycle or references the target host")]
    ChainCycle,
    #[error("unknown reference: {0}")]
    UnknownReference(String),
    #[error("could not parse connection string: {0}")]
    Parse(String),
}
