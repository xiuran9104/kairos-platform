//! MCP server registry — manages connections to multiple MCP servers,
//! caches discovered tools, and provides tool lookup for execution.

use super::client::McpClient;
use super::protocol::*;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Load MCP server configs from the environment.
pub fn load_configs() -> Vec<McpServerConfig> {
    let raw = std::env::var("PENTA_MCP_SERVERS").unwrap_or_default();
    if raw.is_empty() {
        return Vec::new();
    }
    match serde_json::from_str::<Vec<McpServerConfig>>(&raw) {
        Ok(configs) => configs,
        Err(e) => {
            warn!(error = %e, raw, "failed to parse PENTA_MCP_SERVERS; MCP disabled");
            Vec::new()
        }
    }
}

/// Resolve a server config by id. Fails if the server is not configured.
pub fn server_config(server_id: &str) -> anyhow::Result<McpServerConfig> {
    let configs = load_configs();
    configs
        .into_iter()
        .find(|c| c.id == server_id)
        .ok_or_else(|| anyhow::anyhow!("MCP server '{server_id}' not found in PENTA_MCP_SERVERS"))
}

// ── Cached entry ─────────────────────────────────────────────────────────

struct McpServerEntry {
    #[allow(dead_code)]
    config: McpServerConfig,
    client: McpClient,
    tools: Vec<ToolDef>,
    #[allow(dead_code)]
    capabilities: ServerCapabilities,
}

// ── Registry ─────────────────────────────────────────────────────────────

pub struct McpRegistry {
    servers: RwLock<HashMap<String, Arc<McpServerEntry>>>,
}

impl McpRegistry {
    /// Create and initialize all configured MCP servers.
    pub async fn init() -> Self {
        let configs = load_configs();
        let mut servers: HashMap<String, Arc<McpServerEntry>> = HashMap::new();

        for config in configs {
            if !config.auto_start {
                info!(server_id = %config.id, "MCP server auto_start=false; skipping");
                continue;
            }
            match Self::connect(&config).await {
                Ok(entry) => {
                    info!(
                        server_id = %config.id,
                        tool_count = entry.tools.len(),
                        "MCP server connected"
                    );
                    servers.insert(config.id.clone(), Arc::new(entry));
                }
                Err(e) => {
                    warn!(
                        server_id = %config.id,
                        error = %e,
                        "MCP server connection failed; tools will not be available"
                    );
                }
            }
        }

        Self {
            servers: RwLock::new(servers),
        }
    }

    async fn connect(config: &McpServerConfig) -> anyhow::Result<McpServerEntry> {
        let client = McpClient::new(
            config.id.clone(),
            config.command.clone(),
            config.args.clone(),
        );
        let init_result = client.start().await?;
        let tools = client.list_tools().await?;
        Ok(McpServerEntry {
            config: config.clone(),
            client,
            tools,
            capabilities: init_result.capabilities,
        })
    }

    /// Get all tools from a specific MCP server.
    pub async fn tools(&self, server_id: &str) -> anyhow::Result<Vec<ToolDef>> {
        let servers = self.servers.read().await;
        let entry = servers
            .get(server_id)
            .ok_or_else(|| anyhow::anyhow!("MCP server '{server_id}' not connected"))?;
        Ok(entry.tools.clone())
    }

    /// Call a tool on a specific MCP server.
    pub async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: Value,
    ) -> anyhow::Result<CallToolResult> {
        let servers = self.servers.read().await;
        let entry = servers
            .get(server_id)
            .ok_or_else(|| anyhow::anyhow!("MCP server '{server_id}' not connected"))?;
        entry.client.call_tool(tool_name, arguments).await
    }

    /// Get all MCP tools across all servers.
    pub async fn all_tools(&self) -> Vec<(String, ToolDef)> {
        let servers = self.servers.read().await;
        let mut result = Vec::new();
        for (server_id, entry) in servers.iter() {
            for tool in &entry.tools {
                result.push((server_id.clone(), tool.clone()));
            }
        }
        result
    }

    /// Check if a specific MCP server is connected and healthy.
    pub async fn is_healthy(&self, server_id: &str) -> bool {
        let servers = self.servers.read().await;
        if let Some(entry) = servers.get(server_id) {
            entry.client.is_alive().await
        } else {
            false
        }
    }

    /// Number of connected servers.
    pub async fn server_count(&self) -> usize {
        self.servers.read().await.len()
    }
}
