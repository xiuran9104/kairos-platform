//! PostgreSQL-backed Toon ToolCatalog — reads toonflow.tool_definitions.
use kairos_platform_framework_database::PgPool;
use kairos_platform_toon_api::tooling::{
    ToonToolDescriptor, ToonToolExecutorSpec, ToonToolHealthStatus, ToonToolLifecycleStatus,
    ToonToolResourceProfile, ToonToolSource,
};
use serde_json::Value;

pub struct ToonPgCatalog {
    pool: PgPool,
}

impl ToonPgCatalog {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List all active tools (lifecycle_status = 'active').
    pub async fn active_tools(&self) -> anyhow::Result<Vec<ToonToolDescriptor>> {
        let rows = sqlx::query_as::<_, DescriptorRow>(
            "SELECT tool_id, version, manifest_version, namespace, name, description,
                    capabilities, tags, source, input_schema, output_schema,
                    risk_level, executor, resource_profile, lifecycle_status,
                    search_document, revision, created_at, updated_at
             FROM toonflow.tool_definitions
             WHERE lifecycle_status = 'active'
             ORDER BY namespace, name",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut tools = Vec::with_capacity(rows.len());
        for row in rows {
            tools.push(descriptor_from_row(row)?);
        }
        Ok(tools)
    }

    /// Get one tool by id + version.
    pub async fn get(&self, tool_id: &str, version: &str) -> anyhow::Result<Option<ToonToolDescriptor>> {
        let row = sqlx::query_as::<_, DescriptorRow>(
            "SELECT tool_id, version, manifest_version, namespace, name, description,
                    capabilities, tags, source, input_schema, output_schema,
                    risk_level, executor, resource_profile, lifecycle_status,
                    search_document, revision, created_at, updated_at
             FROM toonflow.tool_definitions
             WHERE tool_id = $1 AND version = $2",
        )
        .bind(tool_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?;
        row.map(descriptor_from_row).transpose()
    }

    /// Registry revision (max revision across all tools).
    pub async fn revision(&self) -> anyhow::Result<i64> {
        let rev: Option<i64> = sqlx::query_scalar(
            "SELECT max(revision) FROM toonflow.tool_definitions",
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(rev.unwrap_or(0))
    }

    /// Upsert a tool definition (used by management API).
    pub async fn upsert(
        &self,
        tool_id: &str,
        version: &str,
        name: &str,
        description: &str,
        namespace: &str,
        capabilities: &Value,
        source: &str,
        input_schema: &Value,
        output_schema: &Value,
        risk_level: &str,
        executor: &Value,
    ) -> anyhow::Result<()> {
        let search_doc = format!("{name} {description} {}", serde_json::to_string(capabilities)?);
        sqlx::query(
            "INSERT INTO toonflow.tool_definitions(
                 tool_id, version, namespace, name, description, capabilities, source,
                 input_schema, output_schema, risk_level, executor, search_document, revision
             ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
             ON CONFLICT(tool_id, version) DO UPDATE SET
                 name=EXCLUDED.name, description=EXCLUDED.description,
                 capabilities=EXCLUDED.capabilities, source=EXCLUDED.source,
                 input_schema=EXCLUDED.input_schema, output_schema=EXCLUDED.output_schema,
                 risk_level=EXCLUDED.risk_level, executor=EXCLUDED.executor,
                 search_document=EXCLUDED.search_document,
                 revision=toonflow.tool_definitions.revision + 1,
                 updated_at=now()",
        )
        .bind(tool_id)
        .bind(version)
        .bind(namespace)
        .bind(name)
        .bind(description)
        .bind(capabilities)
        .bind(source)
        .bind(input_schema)
        .bind(output_schema)
        .bind(risk_level)
        .bind(executor)
        .bind(&search_doc)
        .bind(1i32)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Update lifecycle status.
    pub async fn set_status(
        &self,
        tool_id: &str,
        version: &str,
        status: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE toonflow.tool_definitions
             SET lifecycle_status=$3, revision=revision+1, updated_at=now()
             WHERE tool_id=$1 AND version=$2",
        )
        .bind(tool_id)
        .bind(version)
        .bind(status)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct DescriptorRow {
    tool_id: String,
    version: String,
    manifest_version: i32,
    namespace: String,
    name: String,
    description: String,
    capabilities: Value,
    tags: Value,
    source: String,
    input_schema: Value,
    output_schema: Value,
    risk_level: String,
    executor: Value,
    resource_profile: Value,
    lifecycle_status: String,
    search_document: String,
    revision: i64,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

fn descriptor_from_row(row: DescriptorRow) -> anyhow::Result<ToonToolDescriptor> {
    Ok(ToonToolDescriptor {
        tool_id: row.tool_id,
        version: row.version,
        manifest_version: row.manifest_version,
        namespace: row.namespace,
        name: row.name,
        description: row.description,
        capabilities: json_to_vec(&row.capabilities),
        tags: json_to_vec(&row.tags),
        source: parse_source(&row.source),
        input_schema: row.input_schema,
        output_schema: row.output_schema,
        risk_level: row.risk_level,
        executor: ToonToolExecutorSpec {
            kind: parse_source(&row.source),
            image: row.executor.get("image").and_then(Value::as_str).map(String::from),
            image_digest: row.executor.get("image_digest").and_then(Value::as_str).map(String::from),
            endpoint: row.executor.get("endpoint").and_then(Value::as_str).map(String::from),
            handler: row.executor.get("handler").and_then(Value::as_str).map(String::from),
        },
        resource_profile: ToonToolResourceProfile {
            cpu_cores: row.resource_profile.get("cpu_cores").and_then(Value::as_f64).unwrap_or(0.5) as f32,
            memory_mb: row.resource_profile.get("memory_mb").and_then(Value::as_u64).unwrap_or(256),
            timeout_seconds: row.resource_profile.get("timeout_seconds").and_then(Value::as_u64).unwrap_or(120),
            max_concurrency: row.resource_profile.get("max_concurrency").and_then(Value::as_u64).unwrap_or(4) as u32,
        },
        lifecycle_status: parse_lifecycle(&row.lifecycle_status),
        health_status: ToonToolHealthStatus::Ready,
        search_document: row.search_document,
        revision: row.revision,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn json_to_vec(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

fn parse_source(value: &str) -> ToonToolSource {
    match value {
        "http" => ToonToolSource::Http,
        "container" => ToonToolSource::Container,
        "mcp" => ToonToolSource::Mcp,
        _ => ToonToolSource::Builtin,
    }
}

fn parse_lifecycle(value: &str) -> ToonToolLifecycleStatus {
    match value {
        "inactive" => ToonToolLifecycleStatus::Inactive,
        "deprecated" => ToonToolLifecycleStatus::Deprecated,
        _ => ToonToolLifecycleStatus::Active,
    }
}

// ── Tool routing pipeline: hard filter → FTS → deterministic score → top_k ──

use kairos_platform_toon_api::tooling::{ToonToolRouteCandidate, ToonToolRouteDecision, ToonToolRouteRequest};

impl ToonPgCatalog {
    /// Route tools by intent and capabilities. Pipeline:
    /// 1. Hard filter (lifecycle, namespace, capabilities, risk)
    /// 2. FTS recall (PostgreSQL ts_rank on search_document)
    /// 3. Deterministic score (capability match + keyword + quality)
    /// 4. Top-k truncation
    ///
    /// Every step degrades deterministically: FTS failure → full candidate set;
    /// scoring always produces a ranking.
    pub async fn route(
        &self,
        request: &ToonToolRouteRequest,
    ) -> anyhow::Result<ToonToolRouteDecision> {
        // Step 1: Hard filter
        let rows = sqlx::query_as::<_, DescriptorRow>(
            "SELECT tool_id, version, manifest_version, namespace, name, description,
                    capabilities, tags, source, input_schema, output_schema,
                    risk_level, executor, resource_profile, lifecycle_status,
                    search_document, revision, created_at, updated_at
             FROM toonflow.tool_definitions
             WHERE lifecycle_status = 'active'
               AND ($1::text[] IS NULL OR namespace = ANY($1))
               AND ($2::text = 'critical' OR risk_level != 'critical')",
        )
        .bind(if request.allowed_namespaces.is_empty() { None::<Vec<String>> } else { Some(request.allowed_namespaces.clone()) })
        .bind(&request.max_risk)
        .fetch_all(&self.pool)
        .await?;

        let mut candidates: Vec<(ToonToolDescriptor, f64)> = rows
            .into_iter()
            .filter_map(|row| {
                let desc = descriptor_from_row(row).ok()?;
                // Capability filter
                if !request.required_capabilities.is_empty()
                    && !request.required_capabilities.iter().all(|c| desc.capabilities.contains(c))
                {
                    return None;
                }
                // Completed tools filter
                if request.completed_tools.contains(&desc.tool_id) {
                    return None;
                }
                Some((desc, 0.0))
            })
            .collect();

        if candidates.is_empty() {
            return Ok(ToonToolRouteDecision {
                request_id: request.request_id.clone(),
                selected: vec![],
                rejected_count: 0,
                registry_revision: self.revision().await.unwrap_or(0),
                strategy: "hard_filter_only".into(),
            });
        }

        // Step 2: FTS recall boost
        let fts_scores = self.fts_search(&request.intent, candidates.len()).await;
        for (desc, score) in &mut candidates {
            if let Some(fts) = fts_scores.get(&desc.tool_id) {
                *score = *fts as f64 * 0.4; // FTS weight
            }
        }

        // Step 3: Deterministic scoring
        let intent_lower = request.intent.to_lowercase();
        for (desc, score) in &mut candidates {
            // Capability match
            if !request.required_capabilities.is_empty()
                && request.required_capabilities.iter().all(|c| desc.capabilities.contains(c))
            {
                *score += 0.3;
            }
            // Keyword match
            let kw_count = desc.tags.iter()
                .chain(std::iter::once(&desc.name))
                .filter(|s| intent_lower.contains(&s.to_lowercase()))
                .count();
            *score += (kw_count as f64 * 0.1).min(0.2);
            // Namespace match
            if request.allowed_namespaces.is_empty()
                || request.allowed_namespaces.contains(&desc.namespace)
            {
                *score += 0.1;
            }
        }

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let rejected = candidates.len().saturating_sub(request.top_k);
        candidates.truncate(request.top_k);

        let selected: Vec<ToonToolRouteCandidate> = candidates
            .into_iter()
            .map(|(desc, score)| ToonToolRouteCandidate {
                tool_id: desc.tool_id,
                version: desc.version,
                namespace: desc.namespace,
                score,
                reasons: vec![format!("score={score:.3}")],
            })
            .collect();

        Ok(ToonToolRouteDecision {
            request_id: request.request_id.clone(),
            selected,
            rejected_count: rejected,
            registry_revision: self.revision().await.unwrap_or(0),
            strategy: "hard_filter+fts+deterministic_score".into(),
        })
    }

    /// FTS search returning (tool_id → rank) for boosting.
    async fn fts_search(&self, query: &str, top_k: usize) -> std::collections::HashMap<String, f32> {
        if query.trim().is_empty() {
            return std::collections::HashMap::new();
        }
        let result = sqlx::query_as::<_, (String, f64)>(
            "SELECT tool_id, ts_rank(
                to_tsvector('simple', search_document),
                plainto_tsquery('simple', $1)
            ) AS rank
             FROM toonflow.tool_definitions
             WHERE lifecycle_status = 'active'
               AND search_document IS NOT NULL
               AND search_document != ''
             ORDER BY rank DESC
             LIMIT $2",
        )
        .bind(query.chars().take(500).collect::<String>())
        .bind(top_k as i64)
        .fetch_all(&self.pool)
        .await;

        match result {
            Ok(rows) => rows.into_iter()
                .filter(|(_, r)| *r > 0.0)
                .map(|(id, r)| (id, r as f32))
                .collect(),
            Err(_) => std::collections::HashMap::new(),
        }
    }
}
