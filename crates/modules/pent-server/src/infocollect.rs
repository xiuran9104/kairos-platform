//! 信息收集 — FOFA / Quake 在线测绘代理 API。
//!
//! 统一返回 `{provider, fields, results, total, page, size}`；FOFA 的数组行
//! 按 fields 展开为对象，Quake 直接透传 `data` 行的 JSON。

use axum::{
    Json, Router,
    http::StatusCode,
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde_json::{Value, json};

use crate::{ApiResponse, PentState, security};

const DEFAULT_FIELDS: [&str; 10] = [
    "host", "ip", "port", "domain", "title", "protocol", "country", "province", "city", "server",
];
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub fn routes() -> Router<PentState> {
    Router::new()
        .route("/pent/infocollect/search", post(search))
        .route("/pent/infocollect/providers", get(providers))
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

async fn providers(user: CurrentUser) -> Result<Json<ApiResponse<Value>>, AppError> {
    let _tenant_id = security::tenant_id(&user)?;
    Ok(Json(ApiResponse::new(json!([
        {"id": "fofa", "name": "FOFA", "configured": env_configured("FOFA_EMAIL") && env_configured("FOFA_KEY")},
        {"id": "quake", "name": "Quake", "configured": env_configured("QUAKE_KEY")},
        {"id": "zoomeye", "name": "ZoomEye", "configured": false},
        {"id": "shodan", "name": "Shodan", "configured": false},
    ]))))
}

async fn search(
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let _tenant_id = security::tenant_id(&user)?;
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
        "fofa" => search_fofa(query, &fields, size, page, full).await,
        "quake" => search_quake(query, &fields, size, page, full).await,
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
    query: &str,
    fields: &[String],
    size: i64,
    page: i64,
    full: bool,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let email = std::env::var("FOFA_EMAIL").ok().filter(|v| !v.is_empty());
    let key = std::env::var("FOFA_KEY").ok().filter(|v| !v.is_empty());
    let (Some(email), Some(key)) = (email, key) else {
        return Err(AppError::bad_request("未配置 FOFA 凭据（FOFA_EMAIL/FOFA_KEY）"));
    };
    let payload: Value = http_client()?
        .get("https://fofa.info/api/v1/search/all")
        .query(&[
            ("email", email),
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
    query: &str,
    fields: &[String],
    size: i64,
    page: i64,
    full: bool,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let key = std::env::var("QUAKE_KEY").ok().filter(|v| !v.is_empty());
    let Some(key) = key else {
        return Err(AppError::bad_request("未配置 Quake 凭据（QUAKE_KEY）"));
    };
    let payload: Value = http_client()?
        .post("https://quake.360.net/api/v3/search/quake_service")
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
