//! PostgreSQL ToolCatalog — implements `ToolCatalog` trait (AI-01).
use crate::tooling::ports::ToolCatalog;
use async_trait::async_trait;
use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::{ToolDescriptor, ToolRouteRequest};

pub struct PgToolCatalog {
    pool: PgPool,
}

impl PgToolCatalog {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ToolCatalog for PgToolCatalog {
    async fn active_tools(&self, _req: &ToolRouteRequest) -> anyhow::Result<Vec<ToolDescriptor>> {
        let rows = sqlx::query_as::<_, (String,String,serde_json::Value,String,String)>(
            "SELECT td.tool_id, td.version, td.definition, td.status, td.namespace FROM pent_tool_definitions td WHERE td.status='active' AND td.deleted=false ORDER BY td.namespace, td.tool_id"
        ).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|(id, ver, def, _status, ns)| {
                let mut descriptor: ToolDescriptor = serde_json::from_value(def)
                    .map_err(|error| anyhow::anyhow!("工具 {id}@{ver} Manifest 无效: {error}"))?;
                descriptor.tool_id = id;
                descriptor.version = ver;
                descriptor.namespace = ns;
                Ok(descriptor)
            })
            .collect()
    }

    async fn get(&self, tool_id: &str, version: &str) -> anyhow::Result<Option<ToolDescriptor>> {
        let row = sqlx::query_as::<_, (String,String,serde_json::Value,String)>(
            "SELECT tool_id, version, definition, namespace FROM pent_tool_definitions WHERE tool_id=$1 AND version=$2 AND deleted=false"
        ).bind(tool_id).bind(version).fetch_optional(&self.pool).await?;
        row.map(|(id, ver, def, ns)| {
            let mut descriptor: ToolDescriptor = serde_json::from_value(def)
                .map_err(|error| anyhow::anyhow!("工具 {id}@{ver} Manifest 无效: {error}"))?;
            descriptor.tool_id = id;
            descriptor.version = ver;
            descriptor.namespace = ns;
            Ok(descriptor)
        })
        .transpose()
    }

    async fn registry_revision(&self) -> anyhow::Result<i64> {
        sqlx::query_scalar(
            "SELECT coalesce(max(revision),0) FROM pent_tool_definitions WHERE deleted=false",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.into())
    }
}
