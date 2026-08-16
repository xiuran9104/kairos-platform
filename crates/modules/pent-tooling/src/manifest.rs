//! Pure projection from persisted tool manifests to canonical descriptors.

use chrono::{DateTime, Utc};
use kairos_platform_pent_api::{
    ToolDescriptor, ToolExecutionClass, ToolExecutorSpec, ToolHealthStatus, ToolLifecycleStatus,
    ToolQualityMetrics, ToolResourceProfile, ToolSource,
};
use serde_json::Value;

pub struct ToolManifestRecord {
    pub tool_id: String,
    pub version: String,
    pub manifest_version: i32,
    pub namespace: String,
    pub name: String,
    pub description: String,
    pub capabilities: Value,
    pub tags: Value,
    pub source: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub risk_level: String,
    pub approval_policy: String,
    pub supported_target_types: Value,
    pub executor: Value,
    pub resource_profile: Value,
    pub embedding_document: String,
    pub revision: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ToolManifestRecord {
    pub fn into_descriptor(self) -> anyhow::Result<ToolDescriptor> {
        let source = descriptor_source(&self.source, &self.executor)?;
        Ok(ToolDescriptor {
            tool_id: self.tool_id,
            version: self.version,
            manifest_version: self.manifest_version,
            namespace: self.namespace,
            name: self.name,
            description: self.description,
            capabilities: string_list(&self.capabilities),
            tags: string_list(&self.tags),
            source: source.clone(),
            input_schema: self.input_schema,
            output_schema: self.output_schema,
            risk_level: self.risk_level,
            approval_policy: self.approval_policy,
            supported_target_types: string_list(&self.supported_target_types),
            executor: ToolExecutorSpec {
                kind: source,
                endpoint: string_field(&self.executor, "endpoint"),
                server_id: string_field(&self.executor, "server_id"),
                handler: string_field(&self.executor, "handler"),
                execution_class: explicit_execution_class(&self.executor),
            },
            resource_profile: ToolResourceProfile {
                cpu_cores: number_field(&self.resource_profile, "cpu_cores", 0.5) as f32,
                memory_mb: integer_field(&self.resource_profile, "memory_mb", 256),
                timeout_seconds: integer_field(&self.resource_profile, "timeout_seconds", 300),
                max_concurrency: integer_field(&self.resource_profile, "max_concurrency", 1) as u32,
                network_policy: self
                    .resource_profile
                    .get("network_policy")
                    .and_then(Value::as_str)
                    .unwrap_or("scope_only")
                    .to_string(),
            },
            health_status: ToolHealthStatus::Ready,
            lifecycle_status: ToolLifecycleStatus::Active,
            embedding_document: self.embedding_document,
            embedding_model: None,
            embedding_version: None,
            vector_record_id: None,
            quality: ToolQualityMetrics::default(),
            revision: self.revision,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

pub fn descriptor_execution_class(
    risk_level: &str,
    approval_policy: &str,
    executor: &Value,
) -> ToolExecutionClass {
    if let Some(class) = explicit_execution_class(executor) {
        return class;
    }
    if approval_policy.eq_ignore_ascii_case("always")
        || matches!(
            risk_level.to_ascii_lowercase().as_str(),
            "high" | "critical"
        )
    {
        ToolExecutionClass::Destructive
    } else {
        ToolExecutionClass::ReadOnly
    }
}

pub fn explicit_execution_class(executor: &Value) -> Option<ToolExecutionClass> {
    match executor.get("execution_class").and_then(Value::as_str) {
        Some("inline") => Some(ToolExecutionClass::Inline),
        Some("read_only") => Some(ToolExecutionClass::ReadOnly),
        Some("destructive") => Some(ToolExecutionClass::Destructive),
        Some(other) => {
            tracing::warn!(%other, "invalid tool execution_class; using safe default");
            Some(ToolExecutionClass::Destructive)
        }
        None => None,
    }
}

fn descriptor_source(source: &str, executor: &Value) -> anyhow::Result<ToolSource> {
    let executor_kind = executor
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or(source);
    if source == "builtin" {
        return Ok(ToolSource::Builtin);
    }
    match executor_kind {
        "builtin" | "host" | "container" => Ok(ToolSource::Builtin),
        "mcp" => Ok(ToolSource::Mcp),
        "http" => Ok(ToolSource::Http),
        other => anyhow::bail!("未知工具执行类型: {other}"),
    }
}

fn string_list(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn string_field(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(str::to_owned)
}

fn number_field(value: &Value, field: &str, default: f64) -> f64 {
    value.get(field).and_then(Value::as_f64).unwrap_or(default)
}

fn integer_field(value: &Value, field: &str, default: u64) -> u64 {
    value.get(field).and_then(Value::as_u64).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_execution_class_fails_closed() {
        assert_eq!(
            explicit_execution_class(&serde_json::json!({"execution_class": "unknown"})),
            Some(ToolExecutionClass::Destructive)
        );
    }

    #[test]
    fn risk_fallback_is_conservative() {
        assert_eq!(
            descriptor_execution_class("high", "conditional", &serde_json::json!({})),
            ToolExecutionClass::Destructive
        );
        assert_eq!(
            descriptor_execution_class("low", "conditional", &serde_json::json!({})),
            ToolExecutionClass::ReadOnly
        );
    }
}
