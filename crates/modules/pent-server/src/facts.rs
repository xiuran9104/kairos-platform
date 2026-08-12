//! Project fact blackboard / attack path — `pent_facts` + `pent_fact_edges`
//! CRUD, project statistics, fact graph and session-project binding.

use std::collections::HashSet;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
};
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{ApiResponse, PentState, ensure_project_access, security};

const CONFIDENCES: [&str; 3] = ["confirmed", "tentative", "deprecated"];

pub fn routes() -> Router<PentState> {
    Router::new()
        .route("/pent/projects/{id}/stats", get(project_stats))
        .route("/pent/projects/{id}/facts", get(list_facts).post(create_fact))
        .route("/pent/projects/{id}/fact-graph", get(fact_graph))
        .route(
            "/pent/projects/{id}/conversations",
            get(project_conversations),
        )
        .route(
            "/pent/facts/{id}",
            get(get_fact).put(update_fact).delete(delete_fact),
        )
        .route("/pent/facts/{id}/deprecate", post(deprecate_fact))
        .route("/pent/facts/{id}/restore", post(restore_fact))
        .route("/pent/fact-edges", post(create_fact_edge))
        .route("/pent/fact-edges/{id}", delete(delete_fact_edge))
        .route(
            "/pent/conversations/{id}/project",
            put(bind_conversation_project),
        )
}

fn str_field<'a>(item: &'a Value, key: &str) -> Option<&'a str> {
    item.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

fn validate_confidence(confidence: Option<&str>) -> Result<Option<&str>, AppError> {
    match confidence {
        Some(value) if !CONFIDENCES.contains(&value) => Err(AppError::bad_request(
            "confidence 必须是 confirmed/tentative/deprecated 之一",
        )),
        other => Ok(other),
    }
}

fn validate_new_edge(
    source_fact_key: &str,
    target_fact_key: &str,
    known_fact_keys: &HashSet<String>,
) -> Result<(), AppError> {
    if source_fact_key == target_fact_key {
        return Err(AppError::bad_request("不允许自环边"));
    }
    for key in [source_fact_key, target_fact_key] {
        if !known_fact_keys.contains(key) {
            return Err(AppError::bad_request(format!(
                "fact_key 不存在于该项目: {key}"
            )));
        }
    }
    Ok(())
}

async fn project_stats(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    ensure_project_access(&s.pool, &id, &user).await?;
    let stats = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object(
             'fact_count',
                 (SELECT count(*) FROM pent_facts WHERE project_id=$1 AND tenant_id=$2),
             'vuln_count',
                 (SELECT count(*) FROM pent_findings WHERE project_id=$1),
             'conversation_count',
                 (SELECT count(*) FROM pent_agent_sessions
                  WHERE project_id=$1 AND tenant_id=$2),
             'sparse_fact_count',
                 (SELECT count(*) FROM pent_facts
                  WHERE project_id=$1 AND tenant_id=$2
                    AND NOT deprecated AND char_length(body)<50)
         )",
    )
    .bind(&id)
    .bind(tenant_id)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(stats)))
}

async fn list_facts(
    State(s): State<PentState>,
    Path(id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    ensure_project_access(&s.pool, &id, &user).await?;
    let filter = |key: &str| {
        params
            .get(key)
            .map(String::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
    };
    let search = filter("search");
    let category = filter("category");
    let confidence = validate_confidence(filter("confidence"))?;
    let sparse_only = filter("sparse_only").is_some_and(|v| v == "true" || v == "1");
    let exclude_deprecated =
        filter("exclude_deprecated").is_some_and(|v| v == "true" || v == "1");

    let facts = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(f) FROM pent_facts f
         WHERE project_id=$1 AND tenant_id=$2
           AND ($3::text IS NULL OR fact_key ILIKE '%'||$3||'%'
                OR summary ILIKE '%'||$3||'%' OR body ILIKE '%'||$3||'%')
           AND ($4::text IS NULL OR category=$4)
           AND ($5::text IS NULL OR confidence=$5)
           AND (NOT $6 OR (char_length(body)<50 AND NOT deprecated))
           AND (NOT $7 OR NOT deprecated)
         ORDER BY pinned DESC, updated_at DESC",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(search)
    .bind(category)
    .bind(confidence)
    .bind(sparse_only)
    .bind(exclude_deprecated)
    .fetch_all(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(facts)))
}

async fn create_fact(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    ensure_project_access(&s.pool, &id, &user).await?;
    let fact_key = str_field(&body, "fact_key")
        .ok_or_else(|| AppError::bad_request("fact_key 不能为空"))?;
    let summary = str_field(&body, "summary")
        .ok_or_else(|| AppError::bad_request("summary 不能为空"))?;
    let category = str_field(&body, "category").unwrap_or("note");
    let confidence = validate_confidence(str_field(&body, "confidence"))?
        .unwrap_or("tentative");
    let pinned = body
        .get("pinned")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let vuln_id = str_field(&body, "vuln_id");
    let body_text = str_field(&body, "body").unwrap_or("");
    let links_text = str_field(&body, "links_text").unwrap_or("");

    let duplicate: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pent_facts WHERE project_id=$1 AND fact_key=$2)",
    )
    .bind(&id)
    .bind(fact_key)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    if duplicate {
        return Err(AppError::bad_request("同一项目下 fact_key 已存在"));
    }

    let fact_id = Uuid::new_v4().to_string();
    let fact = sqlx::query_scalar::<_, Value>(
        "INSERT INTO pent_facts(
             id,tenant_id,project_id,fact_key,category,summary,body,
             confidence,pinned,vuln_id,links_text
         ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
         RETURNING to_jsonb(pent_facts.*)",
    )
    .bind(&fact_id)
    .bind(tenant_id)
    .bind(&id)
    .bind(fact_key)
    .bind(category)
    .bind(summary)
    .bind(body_text)
    .bind(confidence)
    .bind(pinned)
    .bind(vuln_id)
    .bind(links_text)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(fact)))
}

async fn get_fact(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let fact = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(f) FROM pent_facts f WHERE id=$1 AND tenant_id=$2",
    )
    .bind(&id)
    .bind(tenant_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .ok_or_else(|| AppError::not_found("事实不存在"))?;
    Ok(Json(ApiResponse::new(fact)))
}

async fn update_fact(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let confidence = validate_confidence(str_field(&body, "confidence"))?;
    let pinned = body.get("pinned").and_then(Value::as_bool);
    let affected = sqlx::query(
        "UPDATE pent_facts SET
             category=COALESCE($3,category), summary=COALESCE($4,summary),
             body=COALESCE($5,body), confidence=COALESCE($6,confidence),
             pinned=COALESCE($7,pinned), vuln_id=COALESCE($8,vuln_id),
             links_text=COALESCE($9,links_text), updated_at=now()
         WHERE id=$1 AND tenant_id=$2",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(str_field(&body, "category"))
    .bind(str_field(&body, "summary"))
    .bind(str_field(&body, "body"))
    .bind(confidence)
    .bind(pinned)
    .bind(str_field(&body, "vuln_id"))
    .bind(str_field(&body, "links_text"))
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::not_found("事实不存在"));
    }
    get_fact(State(s), Path(id), user).await
}

async fn delete_fact(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let mut tx = s
        .pool
        .begin()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    let fact_key: Option<String> = sqlx::query_scalar(
        "DELETE FROM pent_facts WHERE id=$1 AND tenant_id=$2 RETURNING fact_key",
    )
    .bind(&id)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    let Some(fact_key) = fact_key else {
        return Err(AppError::not_found("事实不存在"));
    };
    sqlx::query(
        "DELETE FROM pent_fact_edges
         WHERE tenant_id=$1 AND (source_fact_key=$2 OR target_fact_key=$2)",
    )
    .bind(tenant_id)
    .bind(&fact_key)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    tx.commit()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"deleted": true}))))
}

async fn deprecate_fact(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let affected = sqlx::query(
        "UPDATE pent_facts SET deprecated=true, confidence='deprecated', updated_at=now()
         WHERE id=$1 AND tenant_id=$2",
    )
    .bind(&id)
    .bind(tenant_id)
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::not_found("事实不存在"));
    }
    get_fact(State(s), Path(id), user).await
}

async fn restore_fact(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let affected = sqlx::query(
        "UPDATE pent_facts SET deprecated=false, confidence='tentative', updated_at=now()
         WHERE id=$1 AND tenant_id=$2",
    )
    .bind(&id)
    .bind(tenant_id)
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::not_found("事实不存在"));
    }
    get_fact(State(s), Path(id), user).await
}

async fn fact_graph(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    ensure_project_access(&s.pool, &id, &user).await?;
    let nodes = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(node ORDER BY node->>'fact_key'), '[]'::jsonb)
         FROM (
             SELECT jsonb_build_object(
                 'fact_key', fact_key, 'category', category, 'summary', summary,
                 'confidence', confidence, 'deprecated', deprecated, 'pinned', pinned
             ) AS node
             FROM pent_facts
             WHERE project_id=$1 AND tenant_id=$2
         ) t",
    )
    .bind(&id)
    .bind(tenant_id)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    let edges = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(edge ORDER BY edge->>'id'), '[]'::jsonb)
         FROM (
             SELECT jsonb_build_object(
                 'id', id, 'source_fact_key', source_fact_key,
                 'target_fact_key', target_fact_key, 'edge_type', edge_type
             ) AS edge
             FROM pent_fact_edges
             WHERE project_id=$1 AND tenant_id=$2
         ) t",
    )
    .bind(&id)
    .bind(tenant_id)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(json!({
        "nodes": nodes,
        "edges": edges,
    }))))
}

async fn create_fact_edge(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let project_id = str_field(&body, "project_id")
        .ok_or_else(|| AppError::bad_request("project_id 不能为空"))?;
    let source_fact_key = str_field(&body, "source_fact_key")
        .ok_or_else(|| AppError::bad_request("source_fact_key 不能为空"))?;
    let target_fact_key = str_field(&body, "target_fact_key")
        .ok_or_else(|| AppError::bad_request("target_fact_key 不能为空"))?;
    let edge_type = str_field(&body, "edge_type").unwrap_or("related");
    ensure_project_access(&s.pool, project_id, &user).await?;

    let known: HashSet<String> = sqlx::query_scalar(
        "SELECT fact_key FROM pent_facts WHERE project_id=$1 AND tenant_id=$2",
    )
    .bind(project_id)
    .bind(tenant_id)
    .fetch_all(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .into_iter()
    .collect();
    validate_new_edge(source_fact_key, target_fact_key, &known)?;

    let edge = sqlx::query_scalar::<_, Value>(
        "INSERT INTO pent_fact_edges(
             id,tenant_id,project_id,source_fact_key,target_fact_key,edge_type
         ) VALUES($1,$2,$3,$4,$5,$6)
         RETURNING to_jsonb(pent_fact_edges.*)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(tenant_id)
    .bind(project_id)
    .bind(source_fact_key)
    .bind(target_fact_key)
    .bind(edge_type)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(edge)))
}

async fn delete_fact_edge(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let affected = sqlx::query("DELETE FROM pent_fact_edges WHERE id=$1 AND tenant_id=$2")
        .bind(&id)
        .bind(tenant_id)
        .execute(&s.pool)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::not_found("边不存在"));
    }
    Ok(Json(ApiResponse::new(json!({"deleted": true}))))
}

async fn project_conversations(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    ensure_project_access(&s.pool, &id, &user).await?;
    let conversations = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object(
             'id', s.id, 'objective', s.objective, 'status', s.status,
             'created_at', s.created_at, 'updated_at', s.updated_at
         )
         FROM pent_agent_sessions s
         WHERE s.project_id=$1 AND s.tenant_id=$2 AND ($3 OR s.created_by=$4)
         ORDER BY s.created_at DESC",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .fetch_all(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(conversations)))
}

async fn bind_conversation_project(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let project_id = str_field(&body, "project_id");
    if let Some(project_id) = project_id {
        ensure_project_access(&s.pool, project_id, &user).await?;
    }
    let affected = sqlx::query(
        "UPDATE pent_agent_sessions SET project_id=$3, updated_at=now()
         WHERE id=$1 AND tenant_id=$2 AND ($4 OR created_by=$5)",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(project_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::not_found("对话不存在或无权访问"));
    }
    Ok(Json(ApiResponse::new(json!({"updated": true}))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known_keys() -> HashSet<String> {
        ["host.web", "vuln.sqli", "cred.admin"]
            .iter()
            .map(|key| (*key).to_owned())
            .collect()
    }

    #[test]
    fn edge_validation_rejects_self_loop() {
        let result = validate_new_edge("host.web", "host.web", &known_keys());
        assert!(result.is_err());
    }

    #[test]
    fn edge_validation_rejects_unknown_fact_key() {
        assert!(validate_new_edge("host.web", "missing.key", &known_keys()).is_err());
        assert!(validate_new_edge("missing.key", "host.web", &known_keys()).is_err());
    }

    #[test]
    fn edge_validation_accepts_known_distinct_keys() {
        assert!(validate_new_edge("host.web", "vuln.sqli", &known_keys()).is_ok());
    }

    #[test]
    fn confidence_validation_accepts_known_values() {
        assert!(validate_confidence(None).unwrap().is_none());
        assert_eq!(validate_confidence(Some("confirmed")).unwrap(), Some("confirmed"));
        assert!(validate_confidence(Some("bogus")).is_err());
    }
}
