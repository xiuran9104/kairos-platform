//! Project-level authorization boundary for assessment and red-team Runs.

use kairos_platform_framework_database::PgPool;
use serde_json::{Value, json};

type AuthorizationRow = (
    String,
    String,
    Option<chrono::DateTime<chrono::Utc>>,
    Option<chrono::DateTime<chrono::Utc>>,
    Value,
);

pub(crate) async fn authorized_scope(
    pool: &PgPool,
    tenant_id: i64,
    project_id: Option<&str>,
    scope: Value,
) -> Result<Value, String> {
    let Some(project_id) = project_id else {
        return Ok(scope);
    };
    let row: AuthorizationRow = sqlx::query_as(
        "SELECT engagement_mode,authorization_status,authorization_confirmed_at,
                authorization_expires_at,rules_of_engagement
         FROM pent_projects
         WHERE id=$1 AND tenant_id=$2 AND deleted=0",
    )
    .bind(project_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "项目不存在或无权访问".to_string())?;
    let (mode, status, confirmed_at, expires_at, rules) = row;
    if mode == "red_team"
        && (status != "authorized"
            || confirmed_at.is_none()
            || expires_at.is_none_or(|expires| expires <= chrono::Utc::now()))
    {
        return Err("红队 Engagement 尚未授权、已撤销或授权已过期".into());
    }
    Ok(merge_authorization(
        scope,
        project_id,
        &mode,
        &status,
        confirmed_at,
        expires_at,
        rules,
    ))
}

fn merge_authorization(
    scope: Value,
    project_id: &str,
    mode: &str,
    status: &str,
    confirmed_at: Option<chrono::DateTime<chrono::Utc>>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    rules: Value,
) -> Value {
    let mut scope = match scope {
        Value::Object(object) => object,
        Value::Array(targets) => serde_json::Map::from_iter([("targets".into(), targets.into())]),
        _ => serde_json::Map::new(),
    };
    scope.insert("engagement_mode".into(), mode.into());
    scope.insert("engagement_project_id".into(), project_id.into());
    scope.insert("authorization_status".into(), status.into());
    scope.insert("authorization_confirmed_at".into(), json!(confirmed_at));
    scope.insert("authorization_expires_at".into(), json!(expires_at));
    scope.insert("rules_of_engagement".into(), rules);
    Value::Object(scope)
}

pub(crate) fn is_pre_authorized_red_team(scope: &Value) -> bool {
    scope.get("engagement_mode").and_then(Value::as_str) == Some("red_team")
        && scope.get("authorization_status").and_then(Value::as_str) == Some("authorized")
        && scope
            .get("authorization_expires_at")
            .and_then(Value::as_str)
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .is_some_and(|expires| expires > chrono::Utc::now())
}

pub(crate) fn apply_metadata(mut scope: Value, metadata: &Value) -> Value {
    let Some(scope) = scope.as_object_mut() else {
        return scope;
    };
    let Some(metadata) = metadata.as_object() else {
        return Value::Object(scope.clone());
    };
    for (key, value) in metadata {
        if key != "targets" {
            scope.insert(key.clone(), value.clone());
        }
    }
    Value::Object(scope.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_only_live_red_team_authorization() {
        let live = json!({
            "engagement_mode": "red_team",
            "authorization_status": "authorized",
            "authorization_expires_at": "2999-01-01T00:00:00Z"
        });
        let expired = json!({
            "engagement_mode": "red_team",
            "authorization_status": "authorized",
            "authorization_expires_at": "2000-01-01T00:00:00Z"
        });
        assert!(is_pre_authorized_red_team(&live));
        assert!(!is_pre_authorized_red_team(&expired));
    }
}
