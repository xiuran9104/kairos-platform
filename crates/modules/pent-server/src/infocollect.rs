//! 信息收集 — FOFA / Quake 在线测绘代理 API。
//!
//! 统一返回 `{provider, fields, results, total, page, size}`；FOFA 的数组行
//! 按 fields 展开为对象，Quake 直接透传 `data` 行的 JSON。

use axum::{
    Json, Router,
    http::StatusCode,
    extract::State,
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde_json::{Value, json};

use crate::{ApiResponse, PentState, infrastructure::secrets, security};

const DEFAULT_FIELDS: [&str; 10] = [
    "host", "ip", "port", "domain", "title", "protocol", "country", "province", "city", "server",
];
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub fn routes() -> Router<PentState> {
    Router::new()
        .route("/pent/infocollect/search", post(search))
        .route("/pent/infocollect/providers", get(providers))
        .route("/pent/infocollect/config", get(get_config).put(update_config))
}

fn clamp_size(size: Option<i64>) -> i64 {
    size.unwrap_or(20).clamp(1, 100)
}

fn clamp_page(page: Option<i64>) -> i64 {
    page.unwrap_or(1).max(1)
}

fn env_configured(name: &str) -> bool {
    std::env::var(name).map(|v| !v.is_empty()).unwrap_or(false)
}

/// Expand FOFA array rows into objects keyed by the requested fields.
fn fofa_rows_to_objects(fields: &[String], rows: &[Value]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            let mut object = serde_json::Map::new();
            let empty = Vec::new();
            let cells = row.as_array().unwrap_or(&empty);
            for (index, field) in fields.iter().enumerate() {
                object.insert(field.clone(), cells.get(index).cloned().unwrap_or(Value::Null));
            }
            Value::Object(object)
        })
        .collect()
}

async fn stored_config(pool: &sqlx::PgPool, tenant_id: i64) -> Result<Value, AppError> {
    Ok(sqlx::query_scalar::<_, Option<Value>>(
        "SELECT agent_config->'infocollect_providers' FROM pent_runtime_policies
         WHERE tenant_id IN ($1,0) ORDER BY (tenant_id=$1) DESC LIMIT 1",
    ).bind(tenant_id).fetch_optional(pool).await
      .map_err(|e| AppError::internal(e.to_string()))?.flatten().unwrap_or_else(|| json!({})))
}

fn provider_secret(config: &Value, provider: &str, field: &str, env: &str) -> Option<String> {
    config[provider][field].as_str().filter(|v| !v.is_empty())
        .and_then(|v| secrets::decrypt_api_key(v).ok())
        .or_else(|| std::env::var(env).ok().filter(|v| !v.is_empty()))
}

fn fofa_key(config: &Value) -> Option<String> {
    provider_secret(config, "fofa", "api_key", "FOFA_API_KEY")
        .or_else(|| std::env::var("FOFA_KEY").ok().filter(|v| !v.is_empty()))
}

async fn providers(State(s): State<PentState>, user: CurrentUser) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let config = stored_config(&s.pool, tenant_id).await?;
    Ok(Json(ApiResponse::new(json!([
        {"id": "fofa", "name": "FOFA", "configured": fofa_key(&config).is_some()},
        {"id": "quake", "name": "Quake", "configured": provider_secret(&config,"quake","api_key","QUAKE_KEY").is_some()},
        {"id": "zoomeye", "name": "ZoomEye", "configured": provider_secret(&config,"zoomeye","api_key","ZOOMEYE_API_KEY").is_some()},
        {"id": "shodan", "name": "Shodan", "configured": provider_secret(&config,"shodan","api_key","SHODAN_API_KEY").is_some()},
    ]))))
}

async fn get_config(State(s): State<PentState>, user: CurrentUser) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let stored = stored_config(&s.pool, tenant_id).await?;
    let defaults = [("fofa","https://fofa.info/api/v1/search/all"),("zoomeye","https://api.zoomeye.org/v2/search"),("quake","https://quake.360.net/api/v3/search/quake_service"),("shodan","https://api.shodan.io")];
    let rows: Vec<Value> = defaults.into_iter().map(|(id,url)| json!({
        "id":id,"enabled":stored[id]["enabled"].as_bool().unwrap_or(true),
        "base_url":stored[id]["base_url"].as_str().unwrap_or(url),
        "configured":stored[id]["api_key"].as_str().is_some_and(|v| !v.is_empty()) || match id {"fofa"=>env_configured("FOFA_API_KEY") || env_configured("FOFA_KEY"),"quake"=>env_configured("QUAKE_KEY"),"zoomeye"=>env_configured("ZOOMEYE_API_KEY"),_=>env_configured("SHODAN_API_KEY")}
    })).collect();
    Ok(Json(ApiResponse::new(Value::Array(rows))))
}

async fn update_config(State(s): State<PentState>, user: CurrentUser, Json(body): Json<Value>) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let providers = body["providers"].as_array().ok_or_else(|| AppError::bad_request("providers 必须是数组"))?;
    let old = stored_config(&s.pool, tenant_id).await?;
    let mut saved = serde_json::Map::new();
    for item in providers {
        let id = item["id"].as_str().filter(|v| matches!(*v,"fofa"|"zoomeye"|"quake"|"shodan")).ok_or_else(|| AppError::bad_request("不支持的数据源"))?;
        let base_url = item["base_url"].as_str().unwrap_or("");
        if !base_url.starts_with("https://") { return Err(AppError::bad_request("Base URL 必须使用 HTTPS")); }
        let new_key = item["api_key"].as_str().unwrap_or("");
        let api_key = if new_key.is_empty() { old[id]["api_key"].clone() } else { Value::String(secrets::encrypt_api_key(new_key)) };
        saved.insert(id.to_string(), json!({"enabled":item["enabled"].as_bool().unwrap_or(true),"base_url":base_url,"api_key":api_key}));
    }
    let config = Value::Object(saved);
    sqlx::query("INSERT INTO pent_runtime_policies(tenant_id,agent_config,updated_by) VALUES($1,jsonb_build_object('infocollect_providers',$2),$3) ON CONFLICT(tenant_id) DO UPDATE SET agent_config=pent_runtime_policies.agent_config || jsonb_build_object('infocollect_providers',$2),updated_by=$3,updated_at=now()")
        .bind(tenant_id).bind(&config).bind(&user.user_id).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    get_config(State(s), user).await
}

async fn search(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let config = stored_config(&s.pool, tenant_id).await?;
    let provider = body["provider"]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| AppError::bad_request("provider 不能为空"))?;
    let query = body["query"]
        .as_str()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| AppError::bad_request("query 不能为空"))?;
    let size = clamp_size(body.get("size").and_then(Value::as_i64));
    let page = clamp_page(body.get("page").and_then(Value::as_i64));
    let full = body["full"].as_bool().unwrap_or(false);
    let fields: Vec<String> = body
        .get("fields")
        .and_then(Value::as_array)
        .map(|fields| {
            fields
                .iter()
                .filter_map(|field| field.as_str().map(str::to_owned))
                .filter(|field| !field.is_empty())
                .collect()
        })
        .filter(|fields: &Vec<String>| !fields.is_empty())
        .unwrap_or_else(|| DEFAULT_FIELDS.iter().map(|field| field.to_string()).collect());

    match provider {
        "fofa" => search_fofa(&config, query, &fields, size, page, full).await,
        "quake" => search_quake(&config, query, &fields, size, page, full).await,
        _ => Err(AppError::bad_request("暂未接入该数据源")),
    }
}

fn http_client() -> Result<reqwest::Client, AppError> {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|error| AppError::internal(error.to_string()))
}

fn bad_gateway(message: impl Into<String>) -> AppError {
    AppError::new(StatusCode::BAD_GATEWAY, 502, message)
}

async fn search_fofa(
    config: &Value,
    query: &str,
    fields: &[String],
    size: i64,
    page: i64,
    full: bool,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let key = fofa_key(config);
    let Some(key) = key else {
        return Err(AppError::bad_request("未配置 FOFA API Key"));
    };
    let payload: Value = http_client()?
        .get(config["fofa"]["base_url"].as_str().unwrap_or("https://fofa.info/api/v1/search/all"))
        .query(&[
            ("key", key),
            ("qbase64", BASE64.encode(query)),
            ("size", size.to_string()),
            ("fields", fields.join(",")),
            ("page", page.to_string()),
            ("full", full.to_string()),
        ])
        .send()
        .await
        .map_err(|error| bad_gateway(format!("FOFA 请求失败: {error}")))?
        .json()
        .await
        .map_err(|error| bad_gateway(format!("FOFA 响应解析失败: {error}")))?;
    if payload["error"].as_bool().unwrap_or(false) {
        let errmsg = payload["errmsg"]
            .as_str()
            .unwrap_or("FOFA 查询失败")
            .to_string();
        return Err(bad_gateway(errmsg));
    }
    let rows = payload["results"].as_array().cloned().unwrap_or_default();
    let total = payload["size"].as_i64().unwrap_or(0);
    Ok(Json(ApiResponse::new(json!({
        "provider": "fofa",
        "fields": fields,
        "results": fofa_rows_to_objects(fields, &rows),
        "total": total,
        "page": page,
        "size": size,
    }))))
}

async fn search_quake(
    config: &Value,
    query: &str,
    fields: &[String],
    size: i64,
    page: i64,
    full: bool,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let key = provider_secret(config, "quake", "api_key", "QUAKE_KEY");
    let Some(key) = key else {
        return Err(AppError::bad_request("未配置 Quake 凭据（QUAKE_KEY）"));
    };
    let payload: Value = http_client()?
        .post(config["quake"]["base_url"].as_str().unwrap_or("https://quake.360.net/api/v3/search/quake_service"))
        .header("X-QuakeToken", key)
        .json(&json!({
            "query": query,
            "size": size,
            "start": (page - 1) * size,
            "latest": full,
        }))
        .send()
        .await
        .map_err(|error| bad_gateway(format!("Quake 请求失败: {error}")))?
        .json()
        .await
        .map_err(|error| bad_gateway(format!("Quake 响应解析失败: {error}")))?;
    if payload["code"].as_i64().unwrap_or(-1) != 0 {
        let message = payload["message"]
            .as_str()
            .unwrap_or("Quake 查询失败")
            .to_string();
        return Err(bad_gateway(message));
    }
    // Quake 忽略 fields，直接透传 data 行的 JSON。
    let results = payload["data"].as_array().cloned().unwrap_or_default();
    let total = payload["meta"]["pagination"]["total"]
        .as_i64()
        .unwrap_or(0);
    Ok(Json(ApiResponse::new(json!({
        "provider": "quake",
        "fields": fields,
        "results": results,
        "total": total,
        "page": page,
        "size": size,
    }))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fofa_rows_expand_by_field_order() {
        let fields = vec!["ip".to_string(), "port".to_string(), "title".to_string()];
        let rows = vec![
            json!(["1.2.3.4", "443", "example"]),
            json!(["5.6.7.8", "80"]),
            json!([]),
        ];
        let objects = fofa_rows_to_objects(&fields, &rows);
        assert_eq!(
            objects[0],
            json!({"ip": "1.2.3.4", "port": "443", "title": "example"})
        );
        assert_eq!(
            objects[1],
            json!({"ip": "5.6.7.8", "port": "80", "title": null})
        );
        assert_eq!(objects[2], json!({"ip": null, "port": null, "title": null}));
    }

    #[test]
    fn fofa_non_array_row_maps_to_nulls() {
        let fields = vec!["ip".to_string()];
        let rows = vec![json!("not-an-array")];
        assert_eq!(fofa_rows_to_objects(&fields, &rows), vec![json!({"ip": null})]);
    }

    #[test]
    fn size_defaults_and_clamps() {
        assert_eq!(clamp_size(None), 20);
        assert_eq!(clamp_size(Some(0)), 1);
        assert_eq!(clamp_size(Some(-5)), 1);
        assert_eq!(clamp_size(Some(50)), 50);
        assert_eq!(clamp_size(Some(1000)), 100);
    }

    #[test]
    fn page_defaults_and_clamps() {
        assert_eq!(clamp_page(None), 1);
        assert_eq!(clamp_page(Some(0)), 1);
        assert_eq!(clamp_page(Some(-3)), 1);
        assert_eq!(clamp_page(Some(7)), 7);
    }
}
