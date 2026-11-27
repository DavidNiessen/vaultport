// SPDX-License-Identifier: GPL-3.0-or-later
//! A small, dependency-light [Model Context Protocol](https://modelcontextprotocol.io) server.
//!
//! It implements the subset vaultport needs to expose **tools** to AI agents:
//!
//! * the lifecycle (`initialize`, `notifications/initialized`, `ping`),
//! * `tools/list` and `tools/call`, with requests handled concurrently,
//! * cancellation (`notifications/cancelled`),
//!
//! over the stdio transport's framing (one JSON-RPC message per line). The transport is
//! generic over any `AsyncBufRead`/`AsyncWrite` pair, so the same server runs on stdio, a Unix
//! socket or a named pipe.
//!
//! Tool failures are reported as results with `isError: true` (so the model can see and
//! react to them); protocol errors use JSON-RPC error responses.

#![forbid(unsafe_code)]

mod protocol;
mod server;

pub use protocol::{
    ClientInfo, Content, LATEST_PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS, Tool,
    ToolAnnotations, ToolResult,
};
pub use server::{CallContext, Handler, ServerInfo, serve};
