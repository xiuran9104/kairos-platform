//! Catalog composition independent of persistence infrastructure.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;

use crate::{
    contract::{ToolDescriptor, ToolRouteRequest},
    ports::ToolCatalog,
};
use kairos_platform_pent_api::ToolExecutionClass;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub schema: Value,
    pub execution_class: ToolExecutionClass,
}

impl ToolDefinition {
    pub fn new(schema: Value, execution_class: ToolExecutionClass) -> Self {
        Self {
            schema,
            execution_class,
        }
    }

    pub fn name(&self) -> Option<&str> {
        self.schema
            .pointer("/function/name")
            .and_then(Value::as_str)
    }
}

/// Combines catalog adapters while deduplicating tools by id and version.
pub struct CompositeToolCatalog {
    catalogs: Vec<Arc<dyn ToolCatalog>>,
}

impl CompositeToolCatalog {
    pub fn new(catalogs: Vec<Arc<dyn ToolCatalog>>) -> Self {
        Self { catalogs }
    }

    pub fn push(&mut self, catalog: Arc<dyn ToolCatalog>) {
        self.catalogs.push(catalog);
    }
}

#[async_trait]
impl ToolCatalog for CompositeToolCatalog {
    async fn active_tools(
        &self,
        request: &ToolRouteRequest,
    ) -> anyhow::Result<Vec<ToolDescriptor>> {
        let mut tools = HashMap::new();
        for catalog in &self.catalogs {
            for tool in catalog.active_tools(request).await? {
                tools
                    .entry((tool.tool_id.clone(), tool.version.clone()))
                    .or_insert(tool);
            }
        }
        let mut tools = tools.into_values().collect::<Vec<_>>();
        tools.sort_by(|left, right| {
            (&left.namespace, &left.tool_id, &left.version).cmp(&(
                &right.namespace,
                &right.tool_id,
                &right.version,
            ))
        });
        Ok(tools)
    }

    async fn get(&self, tool_id: &str, version: &str) -> anyhow::Result<Option<ToolDescriptor>> {
        for catalog in &self.catalogs {
            if let Some(tool) = catalog.get(tool_id, version).await? {
                return Ok(Some(tool));
            }
        }
        Ok(None)
    }

    async fn registry_revision(&self) -> anyhow::Result<i64> {
        let mut revision = 0;
        for catalog in &self.catalogs {
            revision = revision.max(catalog.registry_revision().await?);
        }
        Ok(revision)
    }
}

impl Default for CompositeToolCatalog {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct RevisionCatalog(i64);

    #[async_trait]
    impl ToolCatalog for RevisionCatalog {
        async fn active_tools(
            &self,
            _request: &ToolRouteRequest,
        ) -> anyhow::Result<Vec<ToolDescriptor>> {
            Ok(Vec::new())
        }

        async fn get(
            &self,
            _tool_id: &str,
            _version: &str,
        ) -> anyhow::Result<Option<ToolDescriptor>> {
            Ok(None)
        }

        async fn registry_revision(&self) -> anyhow::Result<i64> {
            Ok(self.0)
        }
    }

    #[tokio::test]
    async fn composite_uses_highest_registry_revision() {
        let catalog = CompositeToolCatalog::new(vec![
            Arc::new(RevisionCatalog(4)),
            Arc::new(RevisionCatalog(9)),
        ]);
        assert_eq!(catalog.registry_revision().await.unwrap(), 9);
    }

    #[tokio::test]
    async fn empty_composite_is_a_valid_catalog() {
        let catalog = CompositeToolCatalog::default();
        assert_eq!(catalog.registry_revision().await.unwrap(), 0);
        assert!(catalog.get("missing", "1").await.unwrap().is_none());
    }
}
