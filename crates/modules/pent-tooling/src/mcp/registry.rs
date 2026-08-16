//! MCP server registry — manages connections to multiple MCP servers,
//! caches discovered tools, and provides tool lookup for execution.

use super::client::McpClient;
use super::protocol::*;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::{RwLock, Semaphore};
use tracing::{info, warn};

static SHARED: OnceLock<Arc<McpRegistry>> = OnceLock::new();

/// Process-wide registry initialized from deployment environment settings.
pub async fn shared() -> Arc<McpRegistry> {
    if let Some(registry) = SHARED.get() {
        return registry.clone();
    }
    let registry = Arc::new(McpRegistry::from_configs(load_configs()).await);
    let _ = SHARED.set(registry.clone());
    registry
}

pub fn load_configs() -> Vec<McpServerConfig> {
    let raw = std::env::var("PENTA_MCP_SERVERS").unwrap_or_default();
    if raw.is_empty() {
        return Vec::new();
    }
    match serde_json::from_str(&raw) {
        Ok(configs) => configs,
        Err(error) => {
            warn!(%error, "failed to parse PENTA_MCP_SERVERS; MCP disabled");
            Vec::new()
        }
    }
}

// ── Cached entry ─────────────────────────────────────────────────────────

struct McpServerEntry {
    #[allow(dead_code)]
    config: McpServerConfig,
    client: McpClient,
    tools: Vec<ToolDef>,
    #[allow(dead_code)]
    capabilities: ServerCapabilities,
    concurrency: Arc<Semaphore>,
}

// ── Registry ─────────────────────────────────────────────────────────────

pub struct McpRegistry {
    servers: RwLock<HashMap<String, Arc<McpServerEntry>>>,
}

impl McpRegistry {
    /// Create an empty registry. Deployment adapters decide where configuration
    /// comes from and may add servers dynamically.
    pub fn new() -> Self {
        Self {
            servers: RwLock::new(HashMap::new()),
        }
    }

    /// Create and initialize the supplied MCP servers.
    pub async fn from_configs(configs: impl IntoIterator<Item = McpServerConfig>) -> Self {
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
        let client = McpClient::new(config);
        let init_result = client.start().await?;
        let tools = client.list_tools().await?;
        Ok(McpServerEntry {
            config: config.clone(),
            client,
            tools,
            capabilities: init_result.capabilities,
            concurrency: Arc::new(Semaphore::new(config.max_concurrency.max(1))),
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
        let entry = self
            .servers
            .read()
            .await
            .get(server_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("MCP server '{server_id}' not connected"))?;
        let _permit = entry.concurrency.acquire().await?;
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

    /// Connect and register a server at runtime (used by MCP 管理 start).
    pub async fn add_server(&self, config: McpServerConfig) -> anyhow::Result<()> {
        let entry = Self::connect(&config).await?;
        self.servers
            .write()
            .await
            .insert(config.id.clone(), Arc::new(entry));
        info!(server_id = %config.id, "MCP server added at runtime");
        Ok(())
    }

    /// Remove a server from the registry (drops the client connection).
    pub async fn remove_server(&self, server_id: &str) {
        if self.servers.write().await.remove(server_id).is_some() {
            info!(server_id, "MCP server removed");
        }
    }

    /// Runtime status snapshot for every registered server.
    pub async fn snapshot(&self) -> Vec<Value> {
        let servers = self.servers.read().await;
        let mut result = Vec::with_capacity(servers.len());
        for (server_id, entry) in servers.iter() {
            let alive = entry.client.is_alive().await;
            result.push(json!({
                "server_id": server_id,
                "status": if alive { "connected" } else { "error" },
                "tool_count": entry.tools.len(),
                "error": Value::Null,
            }));
        }
        result
    }
}

impl Default for McpRegistry {
    fn default() -> Self {
        Self::new()
    }
}
