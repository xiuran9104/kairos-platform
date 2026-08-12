//! Asset library — paginated search over `pent_assets`, bulk import with
//! upsert semantics, project binding, batch patch/delete/merge and
//! coverage statistics.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post, put},
};
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{ApiResponse, PentState, security};

const SORTABLE_COLUMNS: [&str; 5] =
    ["target", "risk_level", "first_seen", "last_seen", "updated_at"];

pub fn routes() -> Router<PentState> {
    Router::new()
        .route("/pent/assets", get(list_assets))
        .route("/pent/assets/import", post(import_assets))
        .route("/pent/assets/stats", get(asset_stats))
        .route("/pent/assets/project-binding", post(bind_project))
        .route("/pent/assets/bulk", put(bulk_update))
        .route("/pent/assets/batch-delete", post(batch_delete))
        .route("/pent/assets/merge", post(merge_assets))
        .route(
            "/pent/assets/{id}",
            get(get_asset).put(update_asset).delete(delete_asset),
        )
}

fn str_field<'a>(item: &'a Value, key: &str) -> Option<&'a str> {
    item.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

fn tags_field(item: &Value) -> Option<Value> {
    item.get("tags").filter(|v| v.is_array()).cloned()
}

fn asset_ids(body: &Value) -> Vec<String> {
    body.get("asset_ids")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

async fn list_assets(
    State(s): State<PentState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let page = params
        .get("page")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(1)
        .max(1);
    let page_size = params
        .get("page_size")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(20)
        .clamp(1, 200);
    let filter = |key: &str| {
        params
            .get(key)
            .map(String::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
    };
    let q = filter("q");
    let status = filter("status").filter(|v| matches!(*v, "active" | "inactive"));
    let project_id = filter("project_id");
    let risk_level = filter("risk_level");
    let protocol = filter("protocol");
    let port = params.get("port").and_then(|v| v.parse::<i32>().ok());
    let source = filter("source");
    let tag = filter("tag");
    let sort_by = params
        .get("sort_by")
        .map(String::as_str)
        .filter(|v| SORTABLE_COLUMNS.contains(v))
        .unwrap_or("updated_at");
    let sort_order = match params.get("sort_order").map(String::as_str) {
        Some("asc") => "ASC",
        _ => "DESC",
    };
    let offset = (page - 1) * page_size;

    const FILTERS: &str = "FROM pent_assets a
         WHERE tenant_id=$1
           AND ($2::text IS NULL OR target ILIKE '%'||$2||'%' OR host ILIKE '%'||$2||'%'
                OR ip ILIKE '%'||$2||'%' OR domain ILIKE '%'||$2||'%'
                OR title ILIKE '%'||$2||'%' OR server ILIKE '%'||$2||'%'
                OR responsible_person ILIKE '%'||$2||'%')
           AND ($3::text IS NULL OR status=$3)
           AND ($4::text IS NULL OR project_id=$4)
           AND ($5::text IS NULL OR risk_level=$5)
           AND ($6::text IS NULL OR protocol=$6)
           AND ($7::int IS NULL OR port=$7)
           AND ($8::text IS NULL OR source=$8)
           AND ($9::text IS NULL OR tags ? $9)";

    let assets = sqlx::query_scalar::<_, Value>(&format!(
        "SELECT to_jsonb(a) {FILTERS} ORDER BY {sort_by} {sort_order} LIMIT $10 OFFSET $11"
    ))
    .bind(tenant_id)
    .bind(q)
    .bind(status)
    .bind(project_id)
    .bind(risk_level)
    .bind(protocol)
    .bind(port)
    .bind(source)
    .bind(tag)
    .bind(page_size)
    .bind(offset)
    .fetch_all(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;

    let total: i64 = sqlx::query_scalar(&format!("SELECT count(*) {FILTERS}"))
        .bind(tenant_id)
        .bind(q)
        .bind(status)
        .bind(project_id)
        .bind(risk_level)
        .bind(protocol)
        .bind(port)
        .bind(source)
        .bind(tag)
        .fetch_one(&s.pool)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;

    let total_pages = if total == 0 {
        0
    } else {
        (total + page_size - 1) / page_size
    };
    Ok(Json(ApiResponse::new(json!({
        "assets": assets,
        "total": total,
        "total_pages": total_pages,
        "page": page,
    }))))
}

async fn import_assets(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let source = str_field(&body, "source").unwrap_or("manual").to_owned();
    let items = body
        .get("assets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let (mut created, mut updated, mut skipped) = (0i64, 0i64, 0i64);

    for item in &items {
        let Some(target) = str_field(item, "target") else {
            skipped += 1;
            continue;
        };
        let port = item
            .get("port")
            .and_then(Value::as_i64)
            .map(|v| v as i32);
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT id FROM pent_assets
             WHERE tenant_id=$1 AND target=$2 AND COALESCE(port,-1)=COALESCE($3,-1)
             LIMIT 1",
        )
        .bind(tenant_id)
        .bind(target)
        .bind(port)
        .fetch_optional(&s.pool)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;

        if let Some(id) = existing {
            sqlx::query(
                "UPDATE pent_assets SET
                     project_id=COALESCE($3,project_id), host=COALESCE($4,host),
                     ip=COALESCE($5,ip), domain=COALESCE($6,domain),
                     protocol=COALESCE($7,protocol), title=COALESCE($8,title),
                     server=COALESCE($9,server), tags=COALESCE($10,tags),
                     risk_level=COALESCE($11,risk_level),
                     responsible_person=COALESCE($12,responsible_person),
                     department=COALESCE($13,department),
                     business_system=COALESCE($14,business_system),
                     environment=COALESCE($15,environment),
                     criticality=COALESCE($16,criticality),
                     country=COALESCE($17,country), province=COALESCE($18,province),
                     city=COALESCE($19,city),
                     last_seen=now(), updated_at=now()
                 WHERE id=$1 AND tenant_id=$2",
            )
            .bind(&id)
            .bind(tenant_id)
            .bind(str_field(item, "project_id"))
            .bind(str_field(item, "host"))
            .bind(str_field(item, "ip"))
            .bind(str_field(item, "domain"))
            .bind(str_field(item, "protocol"))
            .bind(str_field(item, "title"))
            .bind(str_field(item, "server"))
            .bind(tags_field(item))
            .bind(str_field(item, "risk_level"))
            .bind(str_field(item, "responsible_person"))
            .bind(str_field(item, "department"))
            .bind(str_field(item, "business_system"))
            .bind(str_field(item, "environment"))
            .bind(str_field(item, "criticality"))
            .bind(str_field(item, "country"))
            .bind(str_field(item, "province"))
            .bind(str_field(item, "city"))
            .execute(&s.pool)
            .await
            .map_err(|error| AppError::internal(error.to_string()))?;
            updated += 1;
        } else {
            sqlx::query(
                "INSERT INTO pent_assets(
                     id,tenant_id,project_id,target,host,ip,domain,port,protocol,
                     title,server,source,tags,risk_level,responsible_person,
                     department,business_system,environment,criticality,
                     country,province,city
                 ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(tenant_id)
            .bind(str_field(item, "project_id"))
            .bind(target)
            .bind(str_field(item, "host"))
            .bind(str_field(item, "ip"))
            .bind(str_field(item, "domain"))
            .bind(port)
            .bind(str_field(item, "protocol"))
            .bind(str_field(item, "title"))
            .bind(str_field(item, "server"))
            .bind(&source)
            .bind(tags_field(item).unwrap_or_else(|| json!([])))
            .bind(str_field(item, "risk_level").unwrap_or("info"))
            .bind(str_field(item, "responsible_person"))
            .bind(str_field(item, "department"))
            .bind(str_field(item, "business_system"))
            .bind(str_field(item, "environment"))
            .bind(str_field(item, "criticality"))
            .bind(str_field(item, "country"))
            .bind(str_field(item, "province"))
            .bind(str_field(item, "city"))
            .execute(&s.pool)
            .await
            .map_err(|error| AppError::internal(error.to_string()))?;
            created += 1;
        }
    }
    Ok(Json(ApiResponse::new(json!({
        "created": created,
        "updated": updated,
        "skipped": skipped,
    }))))
}

async fn get_asset(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let asset = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(a) FROM pent_assets a WHERE id=$1 AND tenant_id=$2",
    )
    .bind(&id)
    .bind(tenant_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .ok_or_else(|| AppError::not_found("资产不存在"))?;
    Ok(Json(ApiResponse::new(asset)))
}

async fn update_asset(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    if let Some(status) = str_field(&body, "status") {
        if !matches!(status, "active" | "inactive") {
            return Err(AppError::bad_request("status 必须是 active 或 inactive"));
        }
    }
    let port = body.get("port").and_then(Value::as_i64).map(|v| v as i32);
    let affected = sqlx::query(
        "UPDATE pent_assets SET
             project_id=COALESCE($3,project_id), target=COALESCE($4,target),
             host=COALESCE($5,host), ip=COALESCE($6,ip),
             domain=COALESCE($7,domain), port=COALESCE($8,port),
             protocol=COALESCE($9,protocol), title=COALESCE($10,title),
             server=COALESCE($11,server), source=COALESCE($12,source),
             tags=COALESCE($13,tags), risk_level=COALESCE($14,risk_level),
             status=COALESCE($15,status),
             responsible_person=COALESCE($16,responsible_person),
             department=COALESCE($17,department),
             business_system=COALESCE($18,business_system),
             environment=COALESCE($19,environment),
             criticality=COALESCE($20,criticality),
             country=COALESCE($21,country), province=COALESCE($22,province),
             city=COALESCE($23,city), updated_at=now()
         WHERE id=$1 AND tenant_id=$2",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(str_field(&body, "project_id"))
    .bind(str_field(&body, "target"))
    .bind(str_field(&body, "host"))
    .bind(str_field(&body, "ip"))
    .bind(str_field(&body, "domain"))
    .bind(port)
    .bind(str_field(&body, "protocol"))
    .bind(str_field(&body, "title"))
    .bind(str_field(&body, "server"))
    .bind(str_field(&body, "source"))
    .bind(tags_field(&body))
    .bind(str_field(&body, "risk_level"))
    .bind(str_field(&body, "status"))
    .bind(str_field(&body, "responsible_person"))
    .bind(str_field(&body, "department"))
    .bind(str_field(&body, "business_system"))
    .bind(str_field(&body, "environment"))
    .bind(str_field(&body, "criticality"))
    .bind(str_field(&body, "country"))
    .bind(str_field(&body, "province"))
    .bind(str_field(&body, "city"))
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::not_found("资产不存在"));
    }
    get_asset(State(s), Path(id), user).await
}

async fn delete_asset(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let affected = sqlx::query("DELETE FROM pent_assets WHERE id=$1 AND tenant_id=$2")
        .bind(&id)
        .bind(tenant_id)
        .execute(&s.pool)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::not_found("资产不存在"));
    }
    Ok(Json(ApiResponse::new(json!({"deleted": true}))))
}

async fn bind_project(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let ids = asset_ids(&body);
    if ids.is_empty() {
        return Err(AppError::bad_request("asset_ids 不能为空"));
    }
    let project_id = str_field(&body, "project_id")
        .ok_or_else(|| AppError::bad_request("project_id 不能为空"))?;
    let updated = sqlx::query(
        "UPDATE pent_assets SET project_id=$3, updated_at=now()
         WHERE tenant_id=$1 AND id=ANY($2)",
    )
    .bind(tenant_id)
    .bind(&ids)
    .bind(project_id)
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    Ok(Json(ApiResponse::new(json!({"updated": updated}))))
}

async fn bulk_update(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let ids = asset_ids(&body);
    if ids.is_empty() {
        return Err(AppError::bad_request("asset_ids 不能为空"));
    }
    let patch = body.get("patch").cloned().unwrap_or_else(|| json!({}));
    if let Some(status) = str_field(&patch, "status") {
        if !matches!(status, "active" | "inactive") {
            return Err(AppError::bad_request("status 必须是 active 或 inactive"));
        }
    }
    let add_tags = body.get("add_tags").filter(|v| v.is_array()).cloned();
    let remove_tags = body
        .get("remove_tags")
        .filter(|v| v.is_array())
        .cloned();
    let updated = sqlx::query(
        "UPDATE pent_assets SET
             status=COALESCE($3,status),
             responsible_person=COALESCE($4,responsible_person),
             department=COALESCE($5,department),
             business_system=COALESCE($6,business_system),
             environment=COALESCE($7,environment),
             criticality=COALESCE($8,criticality),
             tags=(
                 SELECT COALESCE(jsonb_agg(val), '[]'::jsonb)
                 FROM (
                     SELECT DISTINCT val
                     FROM jsonb_array_elements_text(tags || COALESCE($9,'[]'::jsonb)) AS val
                     WHERE val <> ALL(COALESCE(
                         (SELECT array_agg(x) FROM jsonb_array_elements_text(COALESCE($10,'[]'::jsonb)) AS x),
                         '{}'::text[]
                     ))
                     ORDER BY val
                 ) kept
             ),
             updated_at=now()
         WHERE tenant_id=$1 AND id=ANY($2)",
    )
    .bind(tenant_id)
    .bind(&ids)
    .bind(str_field(&patch, "status"))
    .bind(str_field(&patch, "responsible_person"))
    .bind(str_field(&patch, "department"))
    .bind(str_field(&patch, "business_system"))
    .bind(str_field(&patch, "environment"))
    .bind(str_field(&patch, "criticality"))
    .bind(add_tags)
    .bind(remove_tags)
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    Ok(Json(ApiResponse::new(json!({"updated": updated}))))
}

async fn batch_delete(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let ids = asset_ids(&body);
    if ids.is_empty() {
        return Err(AppError::bad_request("asset_ids 不能为空"));
    }
    let deleted = sqlx::query("DELETE FROM pent_assets WHERE tenant_id=$1 AND id=ANY($2)")
        .bind(tenant_id)
        .bind(&ids)
        .execute(&s.pool)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?
        .rows_affected();
    Ok(Json(ApiResponse::new(json!({"deleted": deleted}))))
}

async fn merge_assets(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let ids = asset_ids(&body);
    let primary_id = str_field(&body, "primary_id")
        .ok_or_else(|| AppError::bad_request("primary_id 不能为空"))?;
    if ids.len() < 2 || !ids.iter().any(|id| id == primary_id) {
        return Err(AppError::bad_request(
            "asset_ids 至少需要两个元素且必须包含 primary_id",
        ));
    }
    let mut tx = s
        .pool
        .begin()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    let kept = sqlx::query(
        "UPDATE pent_assets SET
             tags=(
                 SELECT COALESCE(jsonb_agg(DISTINCT val), '[]'::jsonb)
                 FROM pent_assets x, jsonb_array_elements_text(x.tags) AS val
                 WHERE x.tenant_id=$1 AND x.id=ANY($2)
             ),
             last_seen=(
                 SELECT max(last_seen) FROM pent_assets
                 WHERE tenant_id=$1 AND id=ANY($2)
             ),
             updated_at=now()
         WHERE tenant_id=$1 AND id=$3",
    )
    .bind(tenant_id)
    .bind(&ids)
    .bind(primary_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    if kept == 0 {
        return Err(AppError::not_found("primary 资产不存在"));
    }
    let merged = sqlx::query(
        "DELETE FROM pent_assets WHERE tenant_id=$1 AND id=ANY($2) AND id<>$3",
    )
    .bind(tenant_id)
    .bind(&ids)
    .bind(primary_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    tx.commit()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"merged": merged}))))
}

async fn asset_stats(
    State(s): State<PentState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let days = params
        .get("days")
        .and_then(|v| v.parse::<i32>().ok())
        .filter(|v| matches!(v, 7 | 30 | 90))
        .unwrap_or(7);

    let counts = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object(
             'total', count(*),
             'ips', count(*) FILTER (WHERE ip IS NOT NULL AND ip<>''),
             'domains', count(*) FILTER (WHERE domain IS NOT NULL AND domain<>''),
             'ports', count(*) FILTER (WHERE port IS NOT NULL),
             'recent', count(*) FILTER (WHERE first_seen >= now() - $2 * interval '1 day')
         ) FROM pent_assets WHERE tenant_id=$1",
    )
    .bind(tenant_id)
    .bind(days)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;

    let protocols = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(bucket ORDER BY (bucket->>'count')::int DESC), '[]'::jsonb)
         FROM (
             SELECT jsonb_build_object('protocol', protocol, 'count', count(*)) AS bucket
             FROM pent_assets
             WHERE tenant_id=$1 AND protocol IS NOT NULL AND protocol<>''
             GROUP BY protocol
             ORDER BY count(*) DESC
             LIMIT 10
         ) t",
    )
    .bind(tenant_id)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;

    let asset_trend = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(bucket ORDER BY bucket->>'date'), '[]'::jsonb)
         FROM (
             SELECT jsonb_build_object(
                 'date', to_char(date_trunc('day', first_seen), 'YYYY-MM-DD'),
                 'added', count(*)
             ) AS bucket
             FROM pent_assets
             WHERE tenant_id=$1 AND first_seen >= now() - $2 * interval '1 day'
             GROUP BY date_trunc('day', first_seen)
         ) t",
    )
    .bind(tenant_id)
    .bind(days)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;

    let coverage = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object(
             'scanned', count(*) FILTER (WHERE last_scan_at IS NOT NULL),
             'scanned_30d', count(*) FILTER (WHERE last_scan_at >= now() - interval '30 days'),
             'never_scanned', count(*) FILTER (WHERE last_scan_at IS NULL),
             'overdue_30d', count(*) FILTER (WHERE last_scan_at < now() - interval '30 days')
         ) FROM pent_assets WHERE tenant_id=$1",
    )
    .bind(tenant_id)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;

    let mut result = counts;
    if let Some(object) = result.as_object_mut() {
        object.insert("protocols".into(), protocols);
        object.insert("asset_trend".into(), asset_trend);
        object.insert("coverage".into(), coverage);
    }
    Ok(Json(ApiResponse::new(result)))
}
