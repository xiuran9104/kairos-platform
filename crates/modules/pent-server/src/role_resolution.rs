use kairos_platform_framework_database::PgPool;
use serde::Serialize;
use serde_json::{Value, json};

pub const PLATFORM_DEFAULT_ROLE_ID: &str = "role-pentest";
const FALLBACK_ROLE_ID: &str = "role-default";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleSource {
    Session,
    Project,
    TenantDefault,
    PlatformDefault,
    Fallback,
}

impl RoleSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Project => "project",
            Self::TenantDefault => "tenant_default",
            Self::PlatformDefault => "platform_default",
            Self::Fallback => "fallback",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedRole {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub user_prompt: String,
    pub tool_whitelist: Value,
    pub hitl_policy: Value,
    pub source: RoleSource,
}

impl ResolvedRole {
    pub fn public_view(&self) -> Value {
        json!({
            "resolved_role_id": self.id,
            "resolved_role_name": self.name,
            "resolved_role_icon": self.icon,
            "role_source": self.source.as_str(),
        })
    }

    pub fn audit_snapshot(&self, requested_role_id: Option<&str>) -> Value {
        json!({
            "requested_role_id": requested_role_id,
            "resolved_role_id": self.id,
            "resolved_role_name": self.name,
            "name": self.name,
            "icon": self.icon,
            "source": self.source.as_str(),
            "tool_whitelist": self.tool_whitelist,
            "hitl_policy": self.hitl_policy,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RoleCandidate {
    id: String,
    source: RoleSource,
}

fn legacy_role_id(value: &str) -> &str {
    match value {
        "penetration_tester" => PLATFORM_DEFAULT_ROLE_ID,
        "security_analyst" | "coordinator" => FALLBACK_ROLE_ID,
        other => other,
    }
}

fn candidates(
    requested_role_id: Option<&str>,
    project_role_id: Option<&str>,
    tenant_default_role_id: Option<&str>,
) -> Vec<RoleCandidate> {
    let values = [
        (requested_role_id, RoleSource::Session),
        (project_role_id, RoleSource::Project),
        (tenant_default_role_id, RoleSource::TenantDefault),
        (Some(PLATFORM_DEFAULT_ROLE_ID), RoleSource::PlatformDefault),
        (Some(FALLBACK_ROLE_ID), RoleSource::Fallback),
    ];
    let mut result = Vec::new();
    for (value, source) in values {
        let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
            continue;
        };
        let id = legacy_role_id(value).to_string();
        if result
            .iter()
            .all(|candidate: &RoleCandidate| candidate.id != id)
        {
            result.push(RoleCandidate { id, source });
        }
    }
    result
}

pub async fn configured_default_role_id(
    pool: &PgPool,
    tenant_id: i64,
) -> anyhow::Result<Option<String>> {
    let value = sqlx::query_scalar::<_, Option<String>>(
        "SELECT COALESCE(
             agent_config #>> '{platform_settings,robots,default_role_id}',
             agent_config #>> '{platform_settings,robots,default_role}'
         )
         FROM pent_runtime_policies WHERE tenant_id=$1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await?
    .flatten()
    .map(|value| legacy_role_id(value.trim()).to_string())
    .filter(|value| !value.is_empty());
    Ok(value)
}

pub async fn ensure_visible_role(
    pool: &PgPool,
    tenant_id: i64,
    role_id: &str,
) -> anyhow::Result<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM pent_agent_roles
             WHERE id=$1 AND (tenant_id=$2 OR tenant_id=0) AND enabled=true
         )",
    )
    .bind(role_id)
    .bind(tenant_id)
    .fetch_one(pool)
    .await?)
}

pub async fn resolve_role(
    pool: &PgPool,
    tenant_id: i64,
    requested_role_id: Option<&str>,
    project_id: Option<&str>,
) -> anyhow::Result<ResolvedRole> {
    let project_role_id = match project_id {
        Some(project_id) => sqlx::query_scalar::<_, Option<String>>(
            "SELECT default_role_id FROM pent_projects
             WHERE id=$1 AND tenant_id=$2 AND deleted=0",
        )
        .bind(project_id)
        .bind(tenant_id)
        .fetch_optional(pool)
        .await?
        .flatten(),
        None => None,
    };
    let tenant_default_role_id = configured_default_role_id(pool, tenant_id).await?;

    for candidate in candidates(
        requested_role_id,
        project_role_id.as_deref(),
        tenant_default_role_id.as_deref(),
    ) {
        let role: Option<(String, String, String, String, Value, Value)> = sqlx::query_as(
            "SELECT id,name,icon,user_prompt,tool_whitelist,hitl_policy
             FROM pent_agent_roles
             WHERE id=$1 AND (tenant_id=$2 OR tenant_id=0) AND enabled=true",
        )
        .bind(&candidate.id)
        .bind(tenant_id)
        .fetch_optional(pool)
        .await?;
        if let Some((id, name, icon, user_prompt, tool_whitelist, hitl_policy)) = role {
            return Ok(ResolvedRole {
                id,
                name,
                icon,
                user_prompt,
                tool_whitelist,
                hitl_policy,
                source: candidate.source,
            });
        }
    }

    anyhow::bail!("没有可用的 Agent 角色")
}

pub async fn decorate_session(
    pool: &PgPool,
    tenant_id: i64,
    mut session: Value,
) -> anyhow::Result<Value> {
    let requested_role_id = session
        .get("constraints")
        .and_then(|value| value.get("role_id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let project_id = session
        .get("project_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let role = resolve_role(
        pool,
        tenant_id,
        requested_role_id.as_deref(),
        project_id.as_deref(),
    )
    .await?;
    if let Some(object) = session.as_object_mut() {
        object.insert("requested_role_id".into(), json!(requested_role_id));
        if let Some(role_fields) = role.public_view().as_object() {
            object.extend(role_fields.clone());
        }
    }
    Ok(session)
}

pub async fn decorate_project(
    pool: &PgPool,
    tenant_id: i64,
    mut project: Value,
) -> anyhow::Result<Value> {
    let project_id = project.get("id").and_then(Value::as_str);
    let role = resolve_role(pool, tenant_id, None, project_id).await?;
    if let Some(object) = project.as_object_mut()
        && let Some(role_fields) = role.public_view().as_object()
    {
        object.extend(role_fields.clone());
    }
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::{RoleSource, candidates};

    #[test]
    fn precedence_is_session_project_tenant_platform_fallback() {
        let values = candidates(Some("session"), Some("project"), Some("tenant"));
        assert_eq!(values[0].source, RoleSource::Session);
        assert_eq!(values[1].source, RoleSource::Project);
        assert_eq!(values[2].source, RoleSource::TenantDefault);
        assert_eq!(values[3].source, RoleSource::PlatformDefault);
        assert_eq!(values[4].source, RoleSource::Fallback);
    }

    #[test]
    fn legacy_default_is_normalized_and_duplicates_are_removed() {
        let values = candidates(None, None, Some("penetration_tester"));
        assert_eq!(values[0].id, "role-pentest");
        assert_eq!(values[0].source, RoleSource::TenantDefault);
        assert_eq!(values.len(), 2);
    }
}
