//! MCP (Model Context Protocol) types — JSON-RPC 2.0 over stdio.
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
pub struct InitializeRequest {
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    pub client_info: Implementation,
}

#[derive(Debug, Serialize)]
pub struct ClientCapabilities {
    #[serde(default)]
    pub roots: Option<RootsCapability>,
}

#[derive(Debug, Serialize)]
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
pub struct ToolDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub input_schema: Value,
}

// ── Tool invocation ─────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CallToolParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Deserialize)]
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

// ── Server config (from env) ────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub auto_start: bool,
}

impl Default for ClientCapabilities {
    fn default() -> Self {
        Self { roots: None }
    }
}
