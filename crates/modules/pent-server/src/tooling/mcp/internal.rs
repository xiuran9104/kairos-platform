//! In-process MCP provider for Pent-owned tools.
//!
//! It is intentionally protocol-shaped: callers discover `ToolDef`s and call
//! them through the same contract as external MCP servers.  The provider does
//! not execute arbitrary YAML commands; recipes are constrained by Pent's
//! lifecycle allow-list and process sandbox.

use std::{collections::HashMap, sync::Arc};

use serde_json::Value;

use crate::infrastructure::process::NetworkPolicy;

use super::{
    protocol::{CallToolResult, ToolDef},
    recipes::{self, ToolRecipe},
};

pub const INTERNAL_SERVER_ID: &str = "kairos-local";

#[derive(Clone)]
pub struct InternalMcpServer {
    recipes: Arc<HashMap<String, ToolRecipe>>,
}

impl InternalMcpServer {
    pub fn load_configured() -> Result<Self, String> {
        Ok(Self {
            recipes: Arc::new(recipes::load_dir(recipes::configured_dir())?),
        })
    }

    pub fn tools(&self) -> Vec<ToolDef> {
        let mut tools = self.recipes.values().map(ToolRecipe::definition).collect::<Vec<_>>();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        tools
    }

    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Value,
        network_policy: NetworkPolicy,
    ) -> anyhow::Result<CallToolResult> {
        let recipe = self
            .recipes
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("内部 MCP 工具 '{name}' 未找到"))?;
        Ok(recipes::execute(recipe, arguments, network_policy).await)
    }

    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }
}
