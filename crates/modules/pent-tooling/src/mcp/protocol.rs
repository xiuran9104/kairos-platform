//! MCP (Model Context Protocol) and JSON-RPC 2.0 data types.
//! Implements the 2024-11-05 protocol version for tool discovery and invocation.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── JSON-RPC envelopes ──────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str, // "2.0"
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    pub id: u64,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

// ── MCP initialization ──────────────────────────────────────────────────

pub const PROTOCOL_VERSION: &str = "2024-11-05";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeRequest {
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    pub client_info: Implementation,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    #[serde(default)]
    pub roots: Option<RootsCapability>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootsCapability {
    #[serde(default)]
    pub list_changed: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Implementation {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: Implementation,
    #[serde(default)]
    pub instructions: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerCapabilities {
    #[serde(default)]
    pub tools: Option<ToolsCapability>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCapability {
    #[serde(default)]
    pub list_changed: bool,
}

// ── Tool discovery ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct ListToolsResult {
    pub tools: Vec<ToolDef>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub input_schema: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
}

/// Optional behavioral hints advertised by an MCP tool.
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

// ── Tool invocation ─────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CallToolParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    #[serde(rename = "resource")]
    Resource {
        #[serde(default)]
        uri: String,
        #[serde(rename = "mimeType", default)]
        mime_type: Option<String>,
        #[serde(default)]
        text: Option<String>,
    },
}

// ── Server config ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    /// Command to spawn for stdio transport. Ignored when `url` is set.
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub auto_start: bool,
    /// Optional HTTP/SSE endpoint. When present, the client uses HTTP/SSE
    /// transport instead of spawning a local child process.
    #[serde(default)]
    pub url: Option<String>,
    /// Extra HTTP headers for HTTP/SSE transport (e.g., authorization).
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,
}

fn default_timeout_secs() -> u64 {
    120
}

fn default_max_concurrency() -> usize {
    4
}

impl McpServerConfig {
    /// True when this server should be reached over HTTP/SSE instead of stdio.
    pub fn is_http(&self) -> bool {
        self.url.as_ref().is_some_and(|u| !u.is_empty())
    }
}

impl Default for ClientCapabilities {
    fn default() -> Self {
        Self { roots: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_remote_transport_from_server_config() {
        let config: McpServerConfig = serde_json::from_value(serde_json::json!({
            "id": "browser",
            "command": "",
            "url": "https://mcp.example.test/sse",
            "headers": {"authorization": "Bearer redacted"}
        }))
        .unwrap();

        assert!(config.is_http());
        assert_eq!(config.headers.len(), 1);
    }

    #[test]
    fn tool_schema_uses_mcp_camel_case_wire_format() {
        let tool = ToolDef {
            name: "inspect".into(),
            description: "Inspect a target".into(),
            input_schema: serde_json::json!({"type": "object"}),
            annotations: Some(ToolAnnotations {
                read_only_hint: Some(true),
                ..Default::default()
            }),
        };

        let value = serde_json::to_value(tool).unwrap();
        assert_eq!(value["inputSchema"]["type"], "object");
        assert!(value.get("input_schema").is_none());
        assert_eq!(value["annotations"]["readOnlyHint"], true);
    }
}
