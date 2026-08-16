//! MCP JSON-RPC client — supports stdio and HTTP/SSE transports.
//!
//! Communication patterns:
//!   stdio:  spawn → initialize → tools/list (cache) → tools/call (on demand)
//!   HTTP/SSE: GET /sse → receive endpoint → POST messages → receive responses via SSE
//!
//! Framing (stdio): Content-Length: N\r\n\r\n{json}

use super::protocol::*;
use super::transport::{HttpSseTransport, McpTransport, StdioTransport};
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tracing::debug;

pub struct McpClient {
    server_id: String,
    transport: Arc<dyn McpTransport>,
    next_id: AtomicU64,
    initialized: AtomicBool,
    call_timeout: Duration,
}

impl McpClient {
    /// Create a new client from server configuration.
    pub fn new(config: &McpServerConfig) -> Self {
        let transport: Arc<dyn McpTransport> = if config.is_http() {
            Arc::new(HttpSseTransport::new(
                config.id.clone(),
                config.url.clone().unwrap_or_default(),
                config.headers.clone(),
            ))
        } else {
            Arc::new(StdioTransport::new(
                config.id.clone(),
                config.command.clone(),
                config.args.clone(),
                config.env.clone(),
            ))
        };

        Self {
            server_id: config.id.clone(),
            transport,
            next_id: AtomicU64::new(1),
            initialized: AtomicBool::new(false),
            call_timeout: Duration::from_secs(config.timeout_secs.max(1)),
        }
    }

    /// Connect to the MCP server and complete the initialize handshake.
    pub async fn start(&self) -> anyhow::Result<InitializeResult> {
        let init_result = self
            .transport
            .clone()
            .initialize(Implementation {
                name: "kairos-pent-server".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            })
            .await?;
        self.initialized
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(init_result)
    }

    /// Discover tools from the MCP server.
    pub async fn list_tools(&self) -> anyhow::Result<Vec<ToolDef>> {
        let result: ListToolsResult = self
            .call_raw("tools/list", None::<Value>, self.call_timeout)
            .await?;
        Ok(result.tools)
    }

    /// Call a tool on the MCP server.
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Value,
    ) -> anyhow::Result<CallToolResult> {
        let params = CallToolParams {
            name: tool_name.into(),
            arguments,
        };
        let result: CallToolResult = self
            .call_raw(
                "tools/call",
                Some(serde_json::to_value(params)?),
                self.call_timeout,
            )
            .await?;
        Ok(result)
    }

    /// Check if the transport is still connected.
    pub async fn is_alive(&self) -> bool {
        self.transport.is_alive().await
    }

    // ── Internals ───────────────────────────────────────────────────────

    fn next_request_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    async fn call_raw<R: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: Option<Value>,
        timeout: Duration,
    ) -> anyhow::Result<R> {
        let id = self.next_request_id();
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        };

        debug!(
            server_id = %self.server_id,
            method = %method,
            request_id = id,
            "MCP request"
        );

        let response = self.transport.call(request, timeout).await?;
        if let Some(err) = response.error {
            anyhow::bail!("MCP error {}: {}", err.code, err.message);
        }
        let value = response
            .result
            .ok_or_else(|| anyhow::anyhow!("MCP response missing result"))?;
        Ok(serde_json::from_value(value)?)
    }
}
