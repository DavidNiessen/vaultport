// SPDX-License-Identifier: GPL-3.0-or-later
//! MCP message types (the subset used by tool servers).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Newest protocol revision this server speaks.
pub const LATEST_PROTOCOL_VERSION: &str = "2025-11-25";

/// Protocol revisions this server accepts, newest first. The tool subset is unchanged
/// between them.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// The client that connected, from `initialize`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientInfo {
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub version: String,
}

impl ClientInfo {
    /// A human-readable name ("Claude Code 2.1").
    pub fn display_name(&self) -> String {
        let name =
            self.title
                .as_deref()
                .filter(|t| !t.is_empty())
                .unwrap_or(if self.name.is_empty() {
                    "An AI agent"
                } else {
                    &self.name
                });
        if self.version.is_empty() {
            name.to_owned()
        } else {
            format!("{name} {}", self.version)
        }
    }
}

/// Behaviour hints shown to clients (they are hints, not guarantees).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

/// A tool definition for `tools/list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub description: String,
    /// JSON Schema of the arguments (`type: object`).
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
}

/// A content block in a tool result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Content {
    Text { text: String },
}

/// The result of `tools/call`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub content: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<Value>,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub is_error: bool,
}

impl ToolResult {
    /// A plain-text success.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![Content::Text { text: text.into() }],
            structured_content: None,
            is_error: false,
        }
    }

    /// A structured success; the JSON is also included as text for older clients.
    pub fn json(value: Value) -> Self {
        let text = serde_json::to_string_pretty(&value).unwrap_or_default();
        Self {
            content: vec![Content::Text { text }],
            structured_content: value.is_object().then_some(value),
            is_error: false,
        }
    }

    /// A failure the model should see.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![Content::Text {
                text: message.into(),
            }],
            structured_content: None,
            is_error: true,
        }
    }
}

/// Picks the protocol version to answer `initialize` with.
pub fn negotiate(requested: Option<&str>) -> &'static str {
    requested
        .and_then(|r| SUPPORTED_PROTOCOL_VERSIONS.iter().find(|v| **v == r))
        .copied()
        .unwrap_or(LATEST_PROTOCOL_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiates_known_versions_and_falls_back_to_latest() {
        assert_eq!(negotiate(Some("2025-06-18")), "2025-06-18");
        assert_eq!(negotiate(Some("1999-01-01")), LATEST_PROTOCOL_VERSION);
        assert_eq!(negotiate(None), LATEST_PROTOCOL_VERSION);
    }

    #[test]
    fn results_serialize_like_the_spec() {
        let r = serde_json::to_value(ToolResult::error("nope")).unwrap_or_default();
        assert_eq!(
            r,
            serde_json::json!({"content":[{"type":"text","text":"nope"}],"isError":true})
        );
        let ok =
            serde_json::to_value(ToolResult::json(serde_json::json!({"a":1}))).unwrap_or_default();
        assert_eq!(ok["structuredContent"]["a"], 1);
        assert!(ok.get("isError").is_none());
    }

    #[test]
    fn client_display_name() {
        let c = ClientInfo {
            name: "claude-code".into(),
            title: Some("Claude Code".into()),
            version: "2.1".into(),
        };
        assert_eq!(c.display_name(), "Claude Code 2.1");
        assert_eq!(ClientInfo::default().display_name(), "An AI agent");
    }
}
