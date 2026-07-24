use std::{collections::HashMap, env, io::Write, time::Instant};

use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{Multipart, Path, Query, State},
    http::header,
    response::Response,
    routing::{delete, get, post, put},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::{Datelike, Timelike, Utc};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_database::PgPool;
use kairos_platform_framework_web::AppError;
use kairos_platform_infra_api::InfraCapability;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod excel;
mod monitor;

#[derive(Clone)]
pub struct InfraState {
    pool: PgPool,
    started_at: Instant,
}

impl InfraState {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            started_at: Instant::now(),
        }
    }
}

#[derive(Debug, Serialize)]
struct Page<T> {
    list: Vec<T>,
    total: i64,
}

#[derive(Debug, Deserialize)]
pub struct QueryParams {
    #[serde(default, rename = "pageNo")]
    page_no: Option<i64>,
    #[serde(default, rename = "pageSize")]
    page_size: Option<i64>,
}

pub fn routes(state: InfraState) -> Router {
    Router::new()
        .route("/infra/capabilities", get(capabilities))
        .route("/infra/config/page", get(config_page))
        .route("/infra/config/get", get(config_get))
        .route("/infra/config/get-value-by-key", get(config_value_by_key))
        .route("/infra/config/create", post(config_create))
        .route("/infra/config/update", put(config_update))
        .route("/infra/config/delete", delete(config_delete))
        .route("/infra/config/delete-list", delete(config_delete_list))
        .route("/infra/config/export-excel", get(excel::config_export))
        .route("/infra/data-source-config/list", get(data_source_list))
        .route("/infra/data-source-config/get", get(data_source_get))
        .route("/infra/data-source-config/create", post(data_source_create))
        .route("/infra/data-source-config/update", put(data_source_update))
        .route(
            "/infra/data-source-config/delete",
            delete(data_source_delete),
        )
        .route(
            "/infra/data-source-config/delete-list",
            delete(data_source_delete_list),
        )
        .route("/infra/file-config/page", get(file_config_page))
        .route("/infra/file-config/get", get(file_config_get))
        .route("/infra/file-config/create", post(file_config_create))
        .route("/infra/file-config/update", put(file_config_update))
        .route("/infra/file-config/update-master", put(file_config_master))
        .route("/infra/file-config/delete", delete(file_config_delete))
        .route(
            "/infra/file-config/delete-list",
            delete(file_config_delete_list),
        )
        .route("/infra/file-config/test", get(ok_bool))
        .route("/infra/file/page", get(file_page))
        .route("/infra/file/create", post(file_create))
        .route("/infra/file/upload", post(file_upload))
        .route("/upload/{*path}", get(file_download))
        .route("/infra/file/presigned-url", get(file_presigned_url))
        .route("/infra/file/delete", delete(file_delete))
        .route("/infra/file/delete-list", delete(file_delete_list))
        .route("/infra/job/page", get(job_page))
        .route("/infra/job/get", get(job_get))
        .route("/infra/job/create", post(job_create))
        .route("/infra/job/update", put(job_update))
        .route("/infra/job/update-status", put(job_update_status))
        .route("/infra/job/trigger", put(job_trigger))
        .route("/infra/job/get_next_times", get(job_next_times))
        .route("/infra/job/sync", post(job_sync))
        .route("/infra/job/delete", delete(job_delete))
        .route("/infra/job/delete-list", delete(job_delete_list))
        .route("/infra/job/export-excel", get(excel::job_export))
        .route("/infra/job-log/page", get(job_log_page))
        .route("/infra/job-log/export-excel", get(excel::job_log_export))
        .route("/infra/api-access-log/page", get(api_access_log_page))
        .route(
            "/infra/api-access-log/export-excel",
            get(excel::api_access_log_export),
        )
        .route("/infra/api-error-log/page", get(api_error_log_page))
        .route(
            "/infra/api-error-log/update-status",
            put(api_error_log_update_status),
        )
        .route(
            "/infra/api-error-log/export-excel",
            get(excel::api_error_log_export),
        )
        .route("/infra/redis/get-monitor-info", get(redis_monitor_info))
        .route("/infra/monitor/postgresql", get(monitor::postgresql))
        .route("/infra/monitor/rust", get(monitor::rust_service))
        .route("/infra/monitor/traces", get(monitor::traces))
        .route("/infra/codegen/table/list", get(codegen_table_list))
        .route("/infra/codegen/table/page", get(codegen_table_page))
        .route("/infra/codegen/detail", get(codegen_detail))
        .route("/infra/codegen/update", put(codegen_update))
        .route("/infra/codegen/sync-from-db", put(codegen_sync_from_db))
        .route("/infra/codegen/preview", get(codegen_preview))
        .route("/infra/codegen/download", get(codegen_download))
        .route("/infra/codegen/db/table/list", get(codegen_db_table_list))
        .route("/infra/codegen/create-list", post(codegen_create_list))
        .route("/infra/codegen/delete", delete(codegen_delete))
        .route("/infra/codegen/delete-list", delete(codegen_delete_list))
        .route("/infra/demo01-contact/page", get(demo01_contact_page))
        .route("/infra/demo01-contact/get", get(demo01_contact_get))
        .route("/infra/demo01-contact/create", post(demo01_contact_create))
        .route("/infra/demo01-contact/update", put(demo01_contact_update))
        .route(
            "/infra/demo01-contact/delete",
            delete(demo01_contact_delete),
        )
        .route(
            "/infra/demo01-contact/delete-list",
            delete(demo01_contact_delete_list),
        )
        .route(
            "/infra/demo01-contact/export-excel",
            get(excel::demo01_contact_export),
        )
        .route("/infra/demo02-category/list", get(demo02_category_list))
        .route("/infra/demo02-category/get", get(demo02_category_get))
        .route(
            "/infra/demo02-category/create",
            post(demo02_category_create),
        )
        .route("/infra/demo02-category/update", put(demo02_category_update))
        .route(
            "/infra/demo02-category/delete",
            delete(demo02_category_delete),
        )
        .route(
            "/infra/demo02-category/export-excel",
            get(excel::demo02_category_export),
        )
        .route(
            "/infra/demo03-student-normal/page",
            get(demo03_student_page),
        )
        .route("/infra/demo03-student-normal/get", get(demo03_student_get))
        .route(
            "/infra/demo03-student-normal/create",
            post(demo03_student_create),
        )
        .route(
            "/infra/demo03-student-normal/update",
            put(demo03_student_update),
        )
        .route(
            "/infra/demo03-student-normal/delete",
            delete(demo03_student_delete),
        )
        .route(
            "/infra/demo03-student-normal/delete-list",
            delete(demo03_student_delete_list),
        )
        .route(
            "/infra/demo03-student-normal/export-excel",
            get(excel::demo03_student_export),
        )
        .route(
            "/infra/demo03-student-normal/demo03-course/list-by-student-id",
            get(demo03_course_list_by_student_id),
        )
        .route(
            "/infra/demo03-student-normal/demo03-grade/get-by-student-id",
            get(demo03_grade_get_by_student_id),
        )
        .route("/infra/demo03-student-inner/page", get(demo03_student_page))
        .route("/infra/demo03-student-inner/get", get(demo03_student_get))
        .route(
            "/infra/demo03-student-inner/create",
            post(demo03_student_create),
        )
        .route(
            "/infra/demo03-student-inner/update",
            put(demo03_student_update),
        )
        .route(
            "/infra/demo03-student-inner/delete",
            delete(demo03_student_delete),
        )
        .route(
            "/infra/demo03-student-inner/delete-list",
            delete(demo03_student_delete_list),
        )
        .route(
            "/infra/demo03-student-inner/export-excel",
            get(excel::demo03_student_export),
        )
        .route(
            "/infra/demo03-student-inner/demo03-course/list-by-student-id",
            get(demo03_course_list_by_student_id),
        )
        .route(
            "/infra/demo03-student-inner/demo03-grade/get-by-student-id",
            get(demo03_grade_get_by_student_id),
        )
        .route("/infra/demo03-student-erp/page", get(demo03_student_page))
        .route("/infra/demo03-student-erp/get", get(demo03_student_get))
        .route(
            "/infra/demo03-student-erp/create",
            post(demo03_student_create),
        )
        .route(
            "/infra/demo03-student-erp/update",
            put(demo03_student_update),
        )
        .route(
            "/infra/demo03-student-erp/delete",
            delete(demo03_student_delete),
        )
        .route(
            "/infra/demo03-student-erp/delete-list",
            delete(demo03_student_delete_list),
        )
        .route(
            "/infra/demo03-student-erp/export-excel",
            get(excel::demo03_student_export),
        )
        .route(
            "/infra/demo03-student-erp/demo03-course/page",
            get(demo03_course_page),
        )
        .route(
            "/infra/demo03-student-erp/demo03-course/get",
            get(demo03_course_get),
        )
        .route(
            "/infra/demo03-student-erp/demo03-course/create",
            post(demo03_course_create),
        )
        .route(
            "/infra/demo03-student-erp/demo03-course/update",
            put(demo03_course_update),
        )
        .route(
            "/infra/demo03-student-erp/demo03-course/delete",
            delete(demo03_course_delete),
        )
        .route(
            "/infra/demo03-student-erp/demo03-course/delete-list",
            delete(demo03_course_delete_list),
        )
        .route(
            "/infra/demo03-student-erp/demo03-grade/page",
            get(demo03_grade_page),
        )
        .route(
            "/infra/demo03-student-erp/demo03-grade/get",
            get(demo03_grade_get),
        )
        .route(
            "/infra/demo03-student-erp/demo03-grade/create",
            post(demo03_grade_create),
        )
        .route(
            "/infra/demo03-student-erp/demo03-grade/update",
            put(demo03_grade_update),
        )
        .route(
            "/infra/demo03-student-erp/demo03-grade/delete",
            delete(demo03_grade_delete),
        )
        .route(
            "/infra/demo03-student-erp/demo03-grade/delete-list",
            delete(demo03_grade_delete_list),
        )
        .with_state(state)
}

async fn capabilities() -> Json<ApiResponse<InfraCapability>> {
    Json(ApiResponse::new(InfraCapability::default()))
}

async fn config_page(
    State(state): State<InfraState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<ApiResponse<Page<Value>>>, AppError> {
    page(
        &state.pool,
        "SELECT count(*) FROM infra_config WHERE deleted=0",
        "SELECT jsonb_build_object('id', id, 'category', category, 'type', type, 'name', name, 'key', config_key, 'value', value, 'visible', visible, 'remark', remark, 'createTime', create_time) FROM infra_config WHERE deleted=0 ORDER BY id DESC LIMIT $1 OFFSET $2",
        params,
    )
    .await
}

async fn config_get(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    get_one(&state.pool, "SELECT jsonb_build_object('id', id, 'category', category, 'type', type, 'name', name, 'key', config_key, 'value', value, 'visible', visible, 'remark', remark, 'createTime', create_time) FROM infra_config WHERE id=$1 AND deleted=0", id_param(&params)?).await
}

async fn config_value_by_key(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let key = params
        .get("key")
        .ok_or_else(|| AppError::bad_request("key is required"))?;
    let value = sqlx::query_scalar::<_, String>(
        "SELECT value FROM infra_config WHERE config_key=$1 AND deleted=0",
    )
    .bind(key)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to get config"))?
    .unwrap_or_default();
    Ok(Json(ApiResponse::new(value)))
}

async fn config_create(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let id = sqlx::query_scalar::<_, i64>("INSERT INTO infra_config (id, category, type, name, config_key, value, visible, remark) VALUES (nextval('infra_config_seq'),$1,$2,$3,$4,$5,$6,$7) RETURNING id")
        .bind(str_field(&payload, "category"))
        .bind(i16_field(&payload, "type", 2))
        .bind(str_field(&payload, "name"))
        .bind(str_field(&payload, "key"))
        .bind(str_field(&payload, "value"))
        .bind(bool_field(&payload, "visible", true))
        .bind(opt_str_field(&payload, "remark"))
        .fetch_one(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to create config"))?;
    Ok(Json(ApiResponse::new(id.to_string())))
}

async fn config_update(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    sqlx::query("UPDATE infra_config SET category=$2,type=$3,name=$4,config_key=$5,value=$6,visible=$7,remark=$8,update_time=now() WHERE id=$1 AND deleted=0")
        .bind(i64_field(&payload, "id", 0))
        .bind(str_field(&payload, "category"))
        .bind(i16_field(&payload, "type", 2))
        .bind(str_field(&payload, "name"))
        .bind(str_field(&payload, "key"))
        .bind(str_field(&payload, "value"))
        .bind(bool_field(&payload, "visible", true))
        .bind(opt_str_field(&payload, "remark"))
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to update config"))?;
    Ok(Json(ApiResponse::new(())))
}

async fn config_delete(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete(&state.pool, "infra_config", id_param(&params)?).await
}

async fn config_delete_list(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete_list(&state.pool, "infra_config", ids_param(&params)).await
}

async fn data_source_list(
    State(state): State<InfraState>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>("SELECT jsonb_build_object('id', id, 'name', name, 'url', url, 'username', username, 'password', CASE WHEN password IS NULL OR password = '' THEN '' ELSE '******' END, 'createTime', create_time) FROM infra_data_source_config WHERE deleted=0 ORDER BY id")
        .fetch_all(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to list data sources"))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn data_source_get(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    get_one(&state.pool, "SELECT jsonb_build_object('id', id, 'name', name, 'url', url, 'username', username, 'password', CASE WHEN password IS NULL OR password = '' THEN '' ELSE '******' END, 'createTime', create_time) FROM infra_data_source_config WHERE id=$1 AND deleted=0", id_param(&params)?).await
}

async fn data_source_create(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let password = seal_secret(&str_field(&payload, "password"));
    let id = sqlx::query_scalar::<_, i64>("INSERT INTO infra_data_source_config (id, name, url, username, password) VALUES (nextval('infra_data_source_config_seq'),$1,$2,$3,$4) RETURNING id")
        .bind(str_field(&payload, "name")).bind(str_field(&payload, "url")).bind(str_field(&payload, "username")).bind(password)
        .fetch_one(&state.pool).await.map_err(|_| AppError::internal("failed to create data source"))?;
    Ok(Json(ApiResponse::new(id.to_string())))
}

async fn data_source_update(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let password = opt_str_field(&payload, "password")
        .filter(|value| value != "******")
        .map(|value| seal_secret(&value));
    sqlx::query("UPDATE infra_data_source_config SET name=$2,url=$3,username=$4,password=COALESCE($5,password),update_time=now() WHERE id=$1 AND deleted=0")
        .bind(i64_field(&payload, "id", 0)).bind(str_field(&payload, "name")).bind(str_field(&payload, "url")).bind(str_field(&payload, "username")).bind(password)
        .execute(&state.pool).await.map_err(|_| AppError::internal("failed to update data source"))?;
    Ok(Json(ApiResponse::new(())))
}

async fn data_source_delete(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete(&state.pool, "infra_data_source_config", id_param(&params)?).await
}

async fn data_source_delete_list(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete_list(&state.pool, "infra_data_source_config", ids_param(&params)).await
}

async fn file_config_page(
    State(state): State<InfraState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<ApiResponse<Page<Value>>>, AppError> {
    page(&state.pool, "SELECT count(*) FROM infra_file_config WHERE deleted=0", "SELECT jsonb_build_object('id', id, 'name', name, 'storage', storage, 'master', master, 'visible', true, 'config', config::jsonb, 'remark', remark, 'createTime', create_time) FROM infra_file_config WHERE deleted=0 ORDER BY id DESC LIMIT $1 OFFSET $2", params).await
}

async fn file_config_get(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    get_one(&state.pool, "SELECT jsonb_build_object('id', id, 'name', name, 'storage', storage, 'master', master, 'visible', true, 'config', config::jsonb, 'remark', remark, 'createTime', create_time) FROM infra_file_config WHERE id=$1 AND deleted=0", id_param(&params)?).await
}

async fn file_config_create(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let id = sqlx::query_scalar::<_, i64>("INSERT INTO infra_file_config (id, name, storage, master, config, remark) VALUES (nextval('infra_file_config_seq'),$1,$2,$3,$4,$5) RETURNING id")
        .bind(str_field(&payload, "name")).bind(i16_field(&payload, "storage", 10)).bind(bool_field(&payload, "master", false)).bind(payload.get("config").cloned().unwrap_or_else(|| json!({})).to_string()).bind(opt_str_field(&payload, "remark"))
        .fetch_one(&state.pool).await.map_err(|_| AppError::internal("failed to create file config"))?;
    Ok(Json(ApiResponse::new(id.to_string())))
}

async fn file_config_update(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    sqlx::query("UPDATE infra_file_config SET name=$2,storage=$3,master=$4,config=$5,remark=$6,update_time=now() WHERE id=$1 AND deleted=0")
        .bind(i64_field(&payload, "id", 0)).bind(str_field(&payload, "name")).bind(i16_field(&payload, "storage", 10)).bind(bool_field(&payload, "master", false)).bind(payload.get("config").cloned().unwrap_or_else(|| json!({})).to_string()).bind(opt_str_field(&payload, "remark"))
        .execute(&state.pool).await.map_err(|_| AppError::internal("failed to update file config"))?;
    Ok(Json(ApiResponse::new(())))
}

async fn file_config_master(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let id = id_param(&params)?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to update file config"))?;
    sqlx::query("UPDATE infra_file_config SET master=false, update_time=now() WHERE deleted=0")
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to update file config"))?;
    sqlx::query(
        "UPDATE infra_file_config SET master=true, update_time=now() WHERE id=$1 AND deleted=0",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::internal("failed to update file config"))?;
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed to update file config"))?;
    Ok(Json(ApiResponse::new(())))
}

async fn file_config_delete(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete(&state.pool, "infra_file_config", id_param(&params)?).await
}

async fn file_config_delete_list(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete_list(&state.pool, "infra_file_config", ids_param(&params)).await
}

async fn file_page(
    State(state): State<InfraState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<ApiResponse<Page<Value>>>, AppError> {
    page(&state.pool, "SELECT count(*) FROM infra_file WHERE deleted=0", "SELECT jsonb_build_object('id', id, 'configId', config_id, 'name', name, 'path', path, 'url', url, 'type', type, 'size', size, 'createTime', create_time) FROM infra_file WHERE deleted=0 ORDER BY id DESC LIMIT $1 OFFSET $2", params).await
}

async fn file_create(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let id = sqlx::query_scalar::<_, i64>("INSERT INTO infra_file (id, config_id, name, path, url, type, size) VALUES (nextval('infra_file_seq'),$1,$2,$3,$4,$5,$6) RETURNING id")
        .bind(opt_i64_field(&payload, "configId")).bind(opt_str_field(&payload, "name")).bind(str_field(&payload, "path")).bind(str_field(&payload, "url")).bind(opt_str_field(&payload, "type")).bind(i32_field(&payload, "size", 0))
        .fetch_one(&state.pool).await.map_err(|_| AppError::internal("failed to create file"))?;
    Ok(Json(ApiResponse::new(id.to_string())))
}

async fn file_upload(
    State(state): State<InfraState>,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let mut uploaded: Option<(String, String, Vec<u8>)> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::bad_request("failed to read upload file"))?
    {
        let file_name = field
            .file_name()
            .map(sanitize_file_name)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("upload-{}", Utc::now().timestamp_millis()));
        let content_type = field
            .content_type()
            .map(ToString::to_string)
            .unwrap_or_else(|| "application/octet-stream".to_owned());
        let bytes = field
            .bytes()
            .await
            .map_err(|_| AppError::bad_request("failed to read upload file"))?
            .to_vec();
        uploaded = Some((file_name, content_type, bytes));
        break;
    }
    let Some((name, content_type, bytes)) = uploaded else {
        return Err(AppError::bad_request("file is required"));
    };
    let date = Utc::now().format("%Y%m%d").to_string();
    let object_name = format!("{}_{}", Utc::now().timestamp_millis(), name);
    let relative_path = format!("{date}/{object_name}");
    let storage_path = upload_storage_dir().join(&relative_path);
    if let Some(parent) = storage_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| AppError::internal("failed to prepare upload directory"))?;
    }
    tokio::fs::write(&storage_path, &bytes)
        .await
        .map_err(|_| AppError::internal("failed to save upload file"))?;
    let path = format!("/upload/{relative_path}");
    let size = i32::try_from(bytes.len()).unwrap_or(i32::MAX);
    let id = sqlx::query_scalar::<_, i64>("INSERT INTO infra_file (id, name, path, url, type, size) VALUES (nextval('infra_file_seq'),$1,$2,$3,$4,$5) RETURNING id")
        .bind(&name).bind(&path).bind(&path).bind(&content_type).bind(size)
        .fetch_one(&state.pool).await.map_err(|_| AppError::internal("failed to upload file"))?;
    Ok(Json(ApiResponse::new(
        json!({"id": id, "name": name, "path": path, "url": path, "type": content_type, "size": size}),
    )))
}

async fn file_download(Path(path): Path<String>) -> Result<Response, AppError> {
    if path.split('/').any(|part| part == ".." || part.is_empty()) {
        return Err(AppError::bad_request("invalid file path"));
    }
    let storage_path = upload_storage_dir().join(&path);
    let bytes = tokio::fs::read(&storage_path)
        .await
        .map_err(|_| AppError::not_found("file not found"))?;
    let content_type = infer_content_type(&path);
    Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(bytes))
        .map_err(|_| AppError::internal("failed to read file"))
}

fn upload_storage_dir() -> std::path::PathBuf {
    env::var("INFRA_UPLOAD_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("storage/uploads"))
}

fn sanitize_file_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '.' | '-' | '_' => ch,
            _ => '_',
        })
        .collect()
}

fn infer_content_type(path: &str) -> &'static str {
    match path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "txt" => "text/plain; charset=utf-8",
        "json" => "application/json",
        "pdf" => "application/pdf",
        "html" => "text/html; charset=utf-8",
        _ => "application/octet-stream",
    }
}

async fn file_presigned_url(
    Query(params): Query<HashMap<String, String>>,
) -> Json<ApiResponse<Value>> {
    let name = params.get("name").cloned().unwrap_or_else(|| "file".into());
    let directory = params
        .get("directory")
        .cloned()
        .unwrap_or_else(|| "upload".into());
    let path = format!("/{directory}/{name}");
    Json(ApiResponse::new(
        json!({"configId": 1, "uploadUrl": path, "url": path, "path": path}),
    ))
}

async fn file_delete(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete(&state.pool, "infra_file", id_param(&params)?).await
}

async fn file_delete_list(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete_list(&state.pool, "infra_file", ids_param(&params)).await
}

async fn job_page(
    State(state): State<InfraState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<ApiResponse<Page<Value>>>, AppError> {
    page(&state.pool, "SELECT count(*) FROM infra_job WHERE deleted=0", "SELECT jsonb_build_object('id', id, 'name', name, 'status', status, 'handlerName', handler_name, 'handlerParam', handler_param, 'cronExpression', cron_expression, 'retryCount', retry_count, 'retryInterval', retry_interval, 'monitorTimeout', monitor_timeout, 'createTime', create_time) FROM infra_job WHERE deleted=0 ORDER BY id DESC LIMIT $1 OFFSET $2", params).await
}

async fn job_get(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    get_one(&state.pool, "SELECT jsonb_build_object('id', id, 'name', name, 'status', status, 'handlerName', handler_name, 'handlerParam', handler_param, 'cronExpression', cron_expression, 'retryCount', retry_count, 'retryInterval', retry_interval, 'monitorTimeout', monitor_timeout, 'createTime', create_time) FROM infra_job WHERE id=$1 AND deleted=0", id_param(&params)?).await
}

async fn job_create(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let id = sqlx::query_scalar::<_, i64>("INSERT INTO infra_job (id, name, status, handler_name, handler_param, cron_expression, retry_count, retry_interval, monitor_timeout) VALUES (nextval('infra_job_seq'),$1,$2,$3,$4,$5,$6,$7,$8) RETURNING id")
        .bind(str_field(&payload, "name")).bind(i16_field(&payload, "status", 0)).bind(str_field(&payload, "handlerName")).bind(opt_str_field(&payload, "handlerParam")).bind(str_field(&payload, "cronExpression")).bind(i32_field(&payload, "retryCount", 0)).bind(i32_field(&payload, "retryInterval", 0)).bind(i32_field(&payload, "monitorTimeout", 0))
        .fetch_one(&state.pool).await.map_err(|_| AppError::internal("failed to create job"))?;
    Ok(Json(ApiResponse::new(id.to_string())))
}

async fn job_update(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    sqlx::query("UPDATE infra_job SET name=$2,status=$3,handler_name=$4,handler_param=$5,cron_expression=$6,retry_count=$7,retry_interval=$8,monitor_timeout=$9,update_time=now() WHERE id=$1 AND deleted=0")
        .bind(i64_field(&payload, "id", 0)).bind(str_field(&payload, "name")).bind(i16_field(&payload, "status", 0)).bind(str_field(&payload, "handlerName")).bind(opt_str_field(&payload, "handlerParam")).bind(str_field(&payload, "cronExpression")).bind(i32_field(&payload, "retryCount", 0)).bind(i32_field(&payload, "retryInterval", 0)).bind(i32_field(&payload, "monitorTimeout", 0))
        .execute(&state.pool).await.map_err(|_| AppError::internal("failed to update job"))?;
    Ok(Json(ApiResponse::new(())))
}

async fn job_update_status(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
    body: Bytes,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let payload = parse_optional_json_body(&body)?;
    let id = params
        .get("id")
        .and_then(|value| value.parse::<i64>().ok())
        .or_else(|| {
            payload
                .as_ref()
                .and_then(|value| opt_i64_field(value, "id"))
        })
        .ok_or_else(|| AppError::bad_request("id is required"))?;
    let status = params
        .get("status")
        .and_then(|value| value.parse::<i16>().ok())
        .or_else(|| {
            payload
                .as_ref()
                .and_then(|value| opt_i64_field(value, "status"))
                .and_then(|value| i16::try_from(value).ok())
        })
        .unwrap_or(0);
    sqlx::query("UPDATE infra_job SET status=$2, update_time=now() WHERE id=$1 AND deleted=0")
        .bind(id)
        .bind(status)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to update job status"))?;
    Ok(Json(ApiResponse::new(())))
}

fn parse_optional_json_body(body: &Bytes) -> Result<Option<Value>, AppError> {
    if body.is_empty() {
        return Ok(None);
    }
    serde_json::from_slice(body)
        .map(Some)
        .map_err(|_| AppError::bad_request("invalid json body"))
}

async fn job_trigger(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let job = job_get(State(state.clone()), Query(params)).await?.0.data;
    let now = Utc::now().naive_utc();
    sqlx::query("INSERT INTO infra_job_log (id, job_id, handler_name, handler_param, begin_time, end_time, duration, status, result) VALUES (nextval('infra_job_log_seq'),$1,$2,$3,$4,$4,0,1,'manual trigger')")
        .bind(job["id"].as_i64().unwrap_or_default()).bind(job["handlerName"].as_str().unwrap_or_default()).bind(job["handlerParam"].as_str()).bind(now)
        .execute(&state.pool).await.map_err(|_| AppError::internal("failed to trigger job"))?;
    Ok(Json(ApiResponse::new(())))
}

async fn job_next_times(
    Query(params): Query<HashMap<String, String>>,
) -> Json<ApiResponse<Vec<String>>> {
    let expression = params
        .get("cronExpression")
        .or_else(|| params.get("cron"))
        .map(String::as_str)
        .unwrap_or("0 0/5 * * * ?");
    Json(ApiResponse::new(next_cron_times(expression, 5)))
}

async fn job_sync(State(state): State<InfraState>) -> Result<Json<ApiResponse<i64>>, AppError> {
    let enabled_jobs =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM infra_job WHERE deleted=0 AND status=0")
            .fetch_one(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to sync jobs"))?;
    Ok(Json(ApiResponse::new(enabled_jobs)))
}

async fn job_delete(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete(&state.pool, "infra_job", id_param(&params)?).await
}

async fn job_delete_list(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete_list(&state.pool, "infra_job", ids_param(&params)).await
}

async fn job_log_page(
    State(state): State<InfraState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<ApiResponse<Page<Value>>>, AppError> {
    page(&state.pool, "SELECT count(*) FROM infra_job_log WHERE deleted=0", "SELECT jsonb_build_object('id', id, 'jobId', job_id, 'handlerName', handler_name, 'handlerParam', handler_param, 'executeIndex', execute_index, 'beginTime', begin_time, 'endTime', end_time, 'duration', duration, 'status', status, 'result', result, 'createTime', create_time) FROM infra_job_log WHERE deleted=0 ORDER BY id DESC LIMIT $1 OFFSET $2", params).await
}

async fn api_access_log_page(
    State(state): State<InfraState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<ApiResponse<Page<Value>>>, AppError> {
    page(&state.pool, "SELECT count(*) FROM infra_api_access_log WHERE deleted=0", "SELECT jsonb_build_object('id', id, 'traceId', trace_id, 'userId', user_id, 'userType', user_type, 'applicationName', application_name, 'requestMethod', request_method, 'requestUrl', request_url, 'requestParams', request_params, 'responseBody', response_body, 'userIp', user_ip, 'userAgent', user_agent, 'operateModule', operate_module, 'operateName', operate_name, 'operateType', operate_type, 'beginTime', begin_time, 'endTime', end_time, 'duration', duration, 'resultCode', result_code, 'resultMsg', result_msg, 'createTime', create_time) FROM infra_api_access_log WHERE deleted=0 ORDER BY id DESC LIMIT $1 OFFSET $2", params).await
}

async fn api_error_log_page(
    State(state): State<InfraState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<ApiResponse<Page<Value>>>, AppError> {
    page(&state.pool, "SELECT count(*) FROM infra_api_error_log WHERE deleted=0", "SELECT jsonb_build_object('id', id, 'traceId', trace_id, 'userId', user_id, 'userType', user_type, 'applicationName', application_name, 'requestMethod', request_method, 'requestUrl', request_url, 'requestParams', request_params, 'userIp', user_ip, 'userAgent', user_agent, 'exceptionTime', exception_time, 'exceptionName', exception_name, 'exceptionMessage', exception_message, 'exceptionRootCauseMessage', exception_root_cause_message, 'exceptionStackTrace', exception_stack_trace, 'exceptionClassName', exception_class_name, 'exceptionFileName', exception_file_name, 'exceptionMethodName', exception_method_name, 'exceptionLineNumber', exception_line_number, 'processStatus', process_status, 'processTime', process_time, 'processUserId', process_user_id, 'createTime', create_time) FROM infra_api_error_log WHERE deleted=0 ORDER BY id DESC LIMIT $1 OFFSET $2", params).await
}

async fn api_error_log_update_status(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    sqlx::query("UPDATE infra_api_error_log SET process_status=$2, process_time=now(), update_time=now() WHERE id=$1")
        .bind(id_param(&params)?)
        .bind(params.get("processStatus").and_then(|value| value.parse::<i16>().ok()).unwrap_or(1))
        .execute(&state.pool).await.map_err(|_| AppError::internal("failed to update error log"))?;
    Ok(Json(ApiResponse::new(())))
}

async fn redis_monitor_info() -> Json<ApiResponse<Value>> {
    Json(ApiResponse::new(match redis_monitor_info_value().await {
        Ok(value) => value,
        Err(error) => json!({
            "available": false,
            "error": error,
            "info": {},
            "dbSize": 0,
            "commandStats": []
        }),
    }))
}

async fn redis_monitor_info_value() -> Result<Value, String> {
    let url = env::var("REDIS_URL").map_err(|_| "REDIS_URL is not configured".to_owned())?;
    let client = redis::Client::open(url.as_str()).map_err(|error| error.to_string())?;
    let mut connection = client
        .get_multiplexed_async_connection()
        .await
        .map_err(|error| error.to_string())?;
    let info_text: String = redis::cmd("INFO")
        .query_async(&mut connection)
        .await
        .map_err(|error| error.to_string())?;
    let db_size: i64 = redis::cmd("DBSIZE")
        .query_async(&mut connection)
        .await
        .map_err(|error| error.to_string())?;
    let mut info = serde_json::Map::new();
    let mut command_stats = Vec::new();
    for line in info_text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if let Some(command) = key.strip_prefix("cmdstat_") {
            let calls = value
                .split(',')
                .find_map(|part| part.strip_prefix("calls="))
                .and_then(|calls| calls.parse::<i64>().ok())
                .unwrap_or(0);
            command_stats.push(json!({"command": command, "calls": calls}));
        } else {
            info.insert(key.to_owned(), json!(value));
        }
    }
    Ok(json!({
        "available": true,
        "info": info,
        "dbSize": db_size,
        "commandStats": command_stats
    }))
}

fn next_cron_times(expression: &str, count: usize) -> Vec<String> {
    let fields: Vec<&str> = expression.split_whitespace().collect();
    if fields.len() < 6 {
        return fallback_next_times(count);
    }
    let minutes = parse_cron_number_field(fields[1], 0, 59);
    let hours = parse_cron_number_field(fields[2], 0, 23);
    let days = parse_cron_number_field(fields[3], 1, 31);
    let months = parse_cron_number_field(fields[4], 1, 12);
    let mut cursor = Utc::now() + chrono::Duration::minutes(1);
    cursor = cursor
        .with_second(0)
        .and_then(|value| value.with_nanosecond(0))
        .unwrap_or(cursor);
    let mut result = Vec::with_capacity(count);
    for _ in 0..(366 * 24 * 60) {
        let minute_ok = minutes
            .as_ref()
            .is_none_or(|values| values.contains(&cursor.minute()));
        let hour_ok = hours
            .as_ref()
            .is_none_or(|values| values.contains(&cursor.hour()));
        let day_ok = days
            .as_ref()
            .is_none_or(|values| values.contains(&cursor.day()));
        let month_ok = months
            .as_ref()
            .is_none_or(|values| values.contains(&cursor.month()));
        if minute_ok && hour_ok && day_ok && month_ok {
            result.push(cursor.to_rfc3339());
            if result.len() == count {
                break;
            }
        }
        cursor += chrono::Duration::minutes(1);
    }
    if result.is_empty() {
        fallback_next_times(count)
    } else {
        result
    }
}

fn parse_cron_number_field(field: &str, min: u32, max: u32) -> Option<Vec<u32>> {
    if matches!(field, "*" | "?") {
        return None;
    }
    let mut values = Vec::new();
    for part in field.split(',') {
        if let Some((start, step)) = part.split_once('/') {
            let start = if start == "*" {
                min
            } else {
                start.parse::<u32>().ok()?.clamp(min, max)
            };
            let step = step.parse::<usize>().ok()?.max(1);
            values.extend((start..=max).step_by(step));
        } else if let Some((start, end)) = part.split_once('-') {
            values.extend(start.parse::<u32>().ok()?.max(min)..=end.parse::<u32>().ok()?.min(max));
        } else {
            values.push(part.parse::<u32>().ok()?.clamp(min, max));
        }
    }
    values.sort_unstable();
    values.dedup();
    Some(values)
}

fn fallback_next_times(count: usize) -> Vec<String> {
    let now = Utc::now();
    (1..=count)
        .map(|n| (now + chrono::Duration::minutes((n * 5) as i64)).to_rfc3339())
        .collect()
}

#[derive(Clone, Copy)]
struct TableSpec {
    table: &'static str,
    seq: &'static str,
}

const CODEGEN_TABLE: TableSpec = TableSpec {
    table: "infra_codegen_table",
    seq: "infra_codegen_table_seq",
};
const CODEGEN_COLUMN: TableSpec = TableSpec {
    table: "infra_codegen_column",
    seq: "infra_codegen_column_seq",
};
const DEMO01_CONTACT: TableSpec = TableSpec {
    table: "yudao_demo01_contact",
    seq: "yudao_demo01_contact_seq",
};
const DEMO02_CATEGORY: TableSpec = TableSpec {
    table: "yudao_demo02_category",
    seq: "yudao_demo02_category_seq",
};
const DEMO03_STUDENT: TableSpec = TableSpec {
    table: "yudao_demo03_student",
    seq: "yudao_demo03_student_seq",
};
const DEMO03_COURSE: TableSpec = TableSpec {
    table: "yudao_demo03_course",
    seq: "yudao_demo03_course_seq",
};
const DEMO03_GRADE: TableSpec = TableSpec {
    table: "yudao_demo03_grade",
    seq: "yudao_demo03_grade_seq",
};

async fn codegen_table_list(
    State(state): State<InfraState>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    table_list(&state.pool, CODEGEN_TABLE).await
}

async fn codegen_table_page(
    State(state): State<InfraState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<ApiResponse<Page<Value>>>, AppError> {
    table_page(&state.pool, CODEGEN_TABLE, params).await
}

async fn codegen_detail(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let table_id = params
        .get("tableId")
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(|| AppError::bad_request("tableId is required"))?;
    let table = table_get_value(&state.pool, CODEGEN_TABLE, table_id).await?;
    let columns = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM infra_codegen_column t WHERE table_id=$1 AND deleted=0 ORDER BY ordinal_position, id",
    )
    .bind(table_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to list codegen columns"))?
    .into_iter()
    .map(table_value)
    .collect::<Vec<_>>();
    Ok(Json(ApiResponse::new(
        json!({ "table": table, "columns": columns }),
    )))
}

async fn codegen_update(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    if let Some(table) = payload.get("table").cloned() {
        let _ = table_update(&state.pool, CODEGEN_TABLE, table).await?;
    }
    for column in payload
        .get("columns")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let _ = table_update(&state.pool, CODEGEN_COLUMN, column).await?;
    }
    Ok(Json(ApiResponse::new(())))
}

async fn codegen_preview(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let table_id = table_id_param(&params)?;
    Ok(Json(ApiResponse::new(
        codegen_preview_files(&state.pool, table_id).await?,
    )))
}

async fn codegen_download(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let table_id = table_id_param(&params)?;
    let files = codegen_preview_files(&state.pool, table_id).await?;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for file in files {
        let path = file["filePath"].as_str().unwrap_or("generated.txt");
        let code = file["code"].as_str().unwrap_or_default();
        writer
            .start_file(path, options)
            .map_err(|_| AppError::internal("failed to build codegen archive"))?;
        writer
            .write_all(code.as_bytes())
            .map_err(|_| AppError::internal("failed to build codegen archive"))?;
    }
    let bytes = writer
        .finish()
        .map_err(|_| AppError::internal("failed to build codegen archive"))?
        .into_inner();
    Response::builder()
        .header("content-type", "application/zip")
        .header(
            "content-disposition",
            "attachment; filename=\"codegen.zip\"",
        )
        .body(Body::from(bytes))
        .map_err(|_| AppError::internal("failed to build download"))
}

async fn codegen_sync_from_db(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let table_id = table_id_param(&params)?;
    sync_codegen_columns(&state.pool, table_id).await?;
    Ok(Json(ApiResponse::new(())))
}

async fn codegen_db_table_list(
    State(state): State<InfraState>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object('name', table_name, 'comment', table_name)
         FROM information_schema.tables
         WHERE table_schema='public'
           AND table_type='BASE TABLE'
           AND table_name NOT LIKE 'qrtz_%'
         ORDER BY table_name",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to list database tables"))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn codegen_create_list(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let data_source_config_id = opt_i64_field(&payload, "dataSourceConfigId").unwrap_or(0);
    for table_name in payload
        .get("tableNames")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM infra_codegen_table WHERE table_name=$1 AND deleted=0",
        )
        .bind(table_name)
        .fetch_one(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to check codegen table"))?;
        if exists > 0 {
            continue;
        }
        let business_name = table_name.rsplit('_').next().unwrap_or(table_name);
        let class_name = pascal_case(table_name);
        let table_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO infra_codegen_table (id, data_source_config_id, scene, table_name, table_comment, module_name, business_name, class_name, class_comment, author, template_type, front_type)
             VALUES (nextval('infra_codegen_table_seq'),$1,1,$2,$2,'infra',$3,$4,$2,'admin',1,20) RETURNING id",
        )
        .bind(data_source_config_id)
        .bind(table_name)
        .bind(business_name)
        .bind(class_name)
        .fetch_one(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to create codegen table"))?;

        insert_codegen_columns(&state.pool, table_id, table_name).await?;
    }
    Ok(Json(ApiResponse::new(())))
}

async fn codegen_delete(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let table_id = params
        .get("tableId")
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(|| AppError::bad_request("tableId is required"))?;
    soft_delete(&state.pool, "infra_codegen_table", table_id).await
}

async fn codegen_delete_list(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete_list(
        &state.pool,
        "infra_codegen_table",
        ids_named_param(&params, "tableIds"),
    )
    .await
}

fn table_id_param(params: &HashMap<String, String>) -> Result<i64, AppError> {
    params
        .get("tableId")
        .or_else(|| params.get("id"))
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(|| AppError::bad_request("tableId is required"))
}

async fn codegen_preview_files(pool: &PgPool, table_id: i64) -> Result<Vec<Value>, AppError> {
    let table = table_get_value(pool, CODEGEN_TABLE, table_id).await?;
    let class_name = table["className"].as_str().unwrap_or("Generated");
    Ok(vec![
        json!({"filePath": format!("src/api/{class_name}.ts"), "code": format!("// preview for {class_name}\nexport interface {class_name} {{\n  id: number;\n}}\n")}),
        json!({"filePath": format!("src/views/{class_name}/index.vue"), "code": format!("<template>\n  <div>{class_name}</div>\n</template>\n")}),
    ])
}

async fn sync_codegen_columns(pool: &PgPool, table_id: i64) -> Result<(), AppError> {
    let table = table_get_value(pool, CODEGEN_TABLE, table_id).await?;
    let table_name = table["tableName"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("tableName is required"))?;
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to sync codegen columns"))?;
    sqlx::query("UPDATE infra_codegen_column SET deleted=1, update_time=now() WHERE table_id=$1")
        .bind(table_id)
        .execute(&mut *transaction)
        .await
        .map_err(|_| AppError::internal("failed to sync codegen columns"))?;
    insert_codegen_columns_in_tx(&mut transaction, table_id, table_name).await?;
    transaction
        .commit()
        .await
        .map_err(|_| AppError::internal("failed to sync codegen columns"))?;
    Ok(())
}

async fn insert_codegen_columns(
    pool: &PgPool,
    table_id: i64,
    table_name: &str,
) -> Result<(), AppError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to create codegen column"))?;
    insert_codegen_columns_in_tx(&mut transaction, table_id, table_name).await?;
    transaction
        .commit()
        .await
        .map_err(|_| AppError::internal("failed to create codegen column"))?;
    Ok(())
}

async fn insert_codegen_columns_in_tx(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    table_id: i64,
    table_name: &str,
) -> Result<(), AppError> {
    let columns = sqlx::query_as::<_, (String, String, bool, i32)>(
        "SELECT column_name, data_type, is_nullable='YES', ordinal_position::int
         FROM information_schema.columns
         WHERE table_schema='public' AND table_name=$1
         ORDER BY ordinal_position",
    )
    .bind(table_name)
    .fetch_all(&mut **transaction)
    .await
    .map_err(|_| AppError::internal("failed to inspect database table"))?;
    for (column_name, data_type, nullable, ordinal_position) in columns {
        sqlx::query(
            "INSERT INTO infra_codegen_column (id, table_id, column_name, data_type, column_comment, nullable, primary_key, ordinal_position, java_type, java_field, create_operation, update_operation, list_operation, list_operation_result, html_type)
             VALUES (nextval('infra_codegen_column_seq'),$1,$2,$3,$2,$4,$5,$6,$7,$8,true,true,true,true,$9)",
        )
        .bind(table_id)
        .bind(&column_name)
        .bind(&data_type)
        .bind(nullable)
        .bind(column_name == "id")
        .bind(ordinal_position)
        .bind(java_type(&data_type))
        .bind(snake_to_camel(&column_name))
        .bind(html_type(&data_type))
        .execute(&mut **transaction)
        .await
        .map_err(|_| AppError::internal("failed to create codegen column"))?;
    }
    Ok(())
}

async fn demo01_contact_page(
    State(state): State<InfraState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<ApiResponse<Page<Value>>>, AppError> {
    table_page(&state.pool, DEMO01_CONTACT, params).await
}

async fn demo01_contact_get(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    table_get(&state.pool, DEMO01_CONTACT, id_param(&params)?).await
}

async fn demo01_contact_create(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    table_create(&state.pool, DEMO01_CONTACT, payload).await
}

async fn demo01_contact_update(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    table_update(&state.pool, DEMO01_CONTACT, payload).await
}

async fn demo01_contact_delete(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete(&state.pool, DEMO01_CONTACT.table, id_param(&params)?).await
}

async fn demo01_contact_delete_list(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete_list(&state.pool, DEMO01_CONTACT.table, ids_param(&params)).await
}

async fn demo02_category_list(
    State(state): State<InfraState>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    table_list(&state.pool, DEMO02_CATEGORY).await
}

async fn demo02_category_get(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    table_get(&state.pool, DEMO02_CATEGORY, id_param(&params)?).await
}

async fn demo02_category_create(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    table_create(&state.pool, DEMO02_CATEGORY, payload).await
}

async fn demo02_category_update(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    table_update(&state.pool, DEMO02_CATEGORY, payload).await
}

async fn demo02_category_delete(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete(&state.pool, DEMO02_CATEGORY.table, id_param(&params)?).await
}

async fn demo03_student_page(
    State(state): State<InfraState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<ApiResponse<Page<Value>>>, AppError> {
    table_page(&state.pool, DEMO03_STUDENT, params).await
}

async fn demo03_student_get(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    table_get(&state.pool, DEMO03_STUDENT, id_param(&params)?).await
}

async fn demo03_student_create(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    table_create(&state.pool, DEMO03_STUDENT, payload).await
}

async fn demo03_student_update(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    table_update(&state.pool, DEMO03_STUDENT, payload).await
}

async fn demo03_student_delete(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete(&state.pool, DEMO03_STUDENT.table, id_param(&params)?).await
}

async fn demo03_student_delete_list(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete_list(&state.pool, DEMO03_STUDENT.table, ids_param(&params)).await
}

async fn demo03_course_page(
    State(state): State<InfraState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<ApiResponse<Page<Value>>>, AppError> {
    table_page(&state.pool, DEMO03_COURSE, params).await
}

async fn demo03_course_get(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    table_get(&state.pool, DEMO03_COURSE, id_param(&params)?).await
}

async fn demo03_course_create(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    table_create(&state.pool, DEMO03_COURSE, payload).await
}

async fn demo03_course_update(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    table_update(&state.pool, DEMO03_COURSE, payload).await
}

async fn demo03_course_delete(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete(&state.pool, DEMO03_COURSE.table, id_param(&params)?).await
}

async fn demo03_course_delete_list(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete_list(&state.pool, DEMO03_COURSE.table, ids_param(&params)).await
}

async fn demo03_grade_page(
    State(state): State<InfraState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<ApiResponse<Page<Value>>>, AppError> {
    table_page(&state.pool, DEMO03_GRADE, params).await
}

async fn demo03_grade_get(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    table_get(&state.pool, DEMO03_GRADE, id_param(&params)?).await
}

async fn demo03_grade_create(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    table_create(&state.pool, DEMO03_GRADE, payload).await
}

async fn demo03_grade_update(
    State(state): State<InfraState>,
    Json(payload): Json<Value>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    table_update(&state.pool, DEMO03_GRADE, payload).await
}

async fn demo03_grade_delete(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete(&state.pool, DEMO03_GRADE.table, id_param(&params)?).await
}

async fn demo03_grade_delete_list(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    soft_delete_list(&state.pool, DEMO03_GRADE.table, ids_param(&params)).await
}

async fn demo03_course_list_by_student_id(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    table_list_by_i64(
        &state.pool,
        DEMO03_COURSE,
        "student_id",
        id_named_param(&params, "studentId")?,
    )
    .await
}

async fn demo03_grade_get_by_student_id(
    State(state): State<InfraState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let student_id = id_named_param(&params, "studentId")?;
    let sql = "SELECT to_jsonb(t) FROM yudao_demo03_grade t WHERE student_id=$1 AND deleted=0 ORDER BY id DESC LIMIT 1";
    let value = sqlx::query_scalar::<_, Value>(sql)
        .bind(student_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to get record"))?
        .map(table_value)
        .unwrap_or_else(|| json!({}));
    Ok(Json(ApiResponse::new(value)))
}

async fn table_page(
    pool: &PgPool,
    spec: TableSpec,
    params: QueryParams,
) -> Result<Json<ApiResponse<Page<Value>>>, AppError> {
    let page_no = params.page_no.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(10).clamp(1, 200);
    let offset = (page_no - 1) * page_size;
    let total_sql = format!("SELECT count(*) FROM {} WHERE deleted=0", spec.table);
    let total = sqlx::query_scalar::<_, i64>(&total_sql)
        .fetch_one(pool)
        .await
        .map_err(|_| AppError::internal("failed to count records"))?;
    let list_sql = format!(
        "SELECT to_jsonb(t) FROM {} t WHERE deleted=0 ORDER BY id DESC LIMIT $1 OFFSET $2",
        spec.table
    );
    let list = sqlx::query_scalar::<_, Value>(&list_sql)
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|_| AppError::internal("failed to list records"))?
        .into_iter()
        .map(table_value)
        .collect();
    Ok(Json(ApiResponse::new(Page { list, total })))
}

async fn table_list(
    pool: &PgPool,
    spec: TableSpec,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let sql = format!(
        "SELECT to_jsonb(t) FROM {} t WHERE deleted=0 ORDER BY id",
        spec.table
    );
    let list = sqlx::query_scalar::<_, Value>(&sql)
        .fetch_all(pool)
        .await
        .map_err(|_| AppError::internal("failed to list records"))?
        .into_iter()
        .map(table_value)
        .collect();
    Ok(Json(ApiResponse::new(list)))
}

async fn table_list_by_i64(
    pool: &PgPool,
    spec: TableSpec,
    column: &str,
    value: i64,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let sql = format!(
        "SELECT to_jsonb(t) FROM {} t WHERE {column}=$1 AND deleted=0 ORDER BY id",
        spec.table
    );
    let list = sqlx::query_scalar::<_, Value>(&sql)
        .bind(value)
        .fetch_all(pool)
        .await
        .map_err(|_| AppError::internal("failed to list records"))?
        .into_iter()
        .map(table_value)
        .collect();
    Ok(Json(ApiResponse::new(list)))
}

async fn table_get(
    pool: &PgPool,
    spec: TableSpec,
    id: i64,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    Ok(Json(ApiResponse::new(
        table_get_value(pool, spec, id).await?,
    )))
}

async fn table_get_value(pool: &PgPool, spec: TableSpec, id: i64) -> Result<Value, AppError> {
    let sql = format!(
        "SELECT to_jsonb(t) FROM {} t WHERE id=$1 AND deleted=0",
        spec.table
    );
    let value = sqlx::query_scalar::<_, Value>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|_| AppError::internal("failed to get record"))?
        .ok_or_else(|| AppError::not_found("record not found"))?;
    Ok(table_value(value))
}

async fn table_create(
    pool: &PgPool,
    spec: TableSpec,
    payload: Value,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let db_payload = camel_payload_to_snake(payload);
    let columns = table_writable_columns(pool, spec.table, &db_payload, false).await?;
    if columns.is_empty() {
        return Err(AppError::bad_request("no writable fields"));
    }
    let column_sql = columns.join(", ");
    let record_sql = columns
        .iter()
        .map(|column| format!("r.{column}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "INSERT INTO {} (id, {}) SELECT nextval('{}'), {} FROM jsonb_populate_record(NULL::{}, $1::jsonb) AS r RETURNING id",
        spec.table, column_sql, spec.seq, record_sql, spec.table
    );
    let id = sqlx::query_scalar::<_, i64>(&sql)
        .bind(db_payload)
        .fetch_one(pool)
        .await
        .map_err(|_| AppError::internal("failed to create record"))?;
    Ok(Json(ApiResponse::new(id.to_string())))
}

async fn table_update(
    pool: &PgPool,
    spec: TableSpec,
    payload: Value,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let id = i64_field(&payload, "id", 0);
    if id == 0 {
        return Err(AppError::bad_request("id is required"));
    }
    let db_payload = camel_payload_to_snake(payload);
    let columns = table_writable_columns(pool, spec.table, &db_payload, true).await?;
    if columns.is_empty() {
        return Ok(Json(ApiResponse::new(())));
    }
    let set_sql = columns
        .iter()
        .map(|column| format!("{column}=r.{column}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "UPDATE {} t SET {}, update_time=now() FROM jsonb_populate_record(NULL::{}, $2::jsonb) AS r WHERE t.id=$1 AND t.deleted=0",
        spec.table, set_sql, spec.table
    );
    sqlx::query(&sql)
        .bind(id)
        .bind(db_payload)
        .execute(pool)
        .await
        .map_err(|_| AppError::internal("failed to update record"))?;
    Ok(Json(ApiResponse::new(())))
}

async fn table_writable_columns(
    pool: &PgPool,
    table: &str,
    payload: &Value,
    include_id: bool,
) -> Result<Vec<String>, AppError> {
    let Some(object) = payload.as_object() else {
        return Ok(vec![]);
    };
    let columns = sqlx::query_scalar::<_, String>(
        "SELECT column_name FROM information_schema.columns WHERE table_schema='public' AND table_name=$1",
    )
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|_| AppError::internal("failed to inspect table"))?;
    Ok(object
        .keys()
        .filter(|key| {
            (include_id || key.as_str() != "id")
                && !matches!(
                    key.as_str(),
                    "creator" | "create_time" | "updater" | "update_time" | "deleted" | "tenant_id"
                )
                && columns.iter().any(|column| column == *key)
        })
        .cloned()
        .collect())
}

async fn page(
    pool: &PgPool,
    count_sql: &str,
    list_sql: &str,
    params: QueryParams,
) -> Result<Json<ApiResponse<Page<Value>>>, AppError> {
    let page_no = params.page_no.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(10).clamp(1, 200);
    let offset = (page_no - 1) * page_size;
    let total = sqlx::query_scalar::<_, i64>(count_sql)
        .fetch_one(pool)
        .await
        .map_err(|_| AppError::internal("failed to count records"))?;
    let list = sqlx::query_scalar::<_, Value>(list_sql)
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|_| AppError::internal("failed to list records"))?;
    Ok(Json(ApiResponse::new(Page { list, total })))
}

async fn get_one(pool: &PgPool, sql: &str, id: i64) -> Result<Json<ApiResponse<Value>>, AppError> {
    let value = sqlx::query_scalar::<_, Value>(sql)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|_| AppError::internal("failed to get record"))?
        .ok_or_else(|| AppError::not_found("record not found"))?;
    Ok(Json(ApiResponse::new(value)))
}

async fn soft_delete(
    pool: &PgPool,
    table: &str,
    id: i64,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let sql = format!("UPDATE {table} SET deleted=1, update_time=now() WHERE id=$1");
    sqlx::query(&sql)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|_| AppError::internal("failed to delete record"))?;
    Ok(Json(ApiResponse::new(())))
}

async fn soft_delete_list(
    pool: &PgPool,
    table: &str,
    ids: Vec<i64>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    for id in ids {
        let _ = soft_delete(pool, table, id).await?;
    }
    Ok(Json(ApiResponse::new(())))
}

fn id_param(params: &HashMap<String, String>) -> Result<i64, AppError> {
    params
        .get("id")
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(|| AppError::bad_request("id is required"))
}

fn ids_param(params: &HashMap<String, String>) -> Vec<i64> {
    ids_named_param(params, "ids")
}

fn ids_named_param(params: &HashMap<String, String>, name: &str) -> Vec<i64> {
    params
        .get(name)
        .into_iter()
        .flat_map(|ids| ids.split(','))
        .filter_map(|id| id.parse::<i64>().ok())
        .collect()
}

fn id_named_param(params: &HashMap<String, String>, name: &str) -> Result<i64, AppError> {
    params
        .get(name)
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(|| AppError::bad_request(format!("{name} is required")))
}

fn str_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn opt_str_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn i16_field(value: &Value, key: &str, default: i16) -> i16 {
    value
        .get(key)
        .and_then(Value::as_i64)
        .and_then(|value| i16::try_from(value).ok())
        .unwrap_or(default)
}

fn i32_field(value: &Value, key: &str, default: i32) -> i32 {
    value
        .get(key)
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(default)
}

fn i64_field(value: &Value, key: &str, default: i64) -> i64 {
    value
        .get(key)
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(|value| value.parse::<i64>().ok()))
        })
        .unwrap_or(default)
}

fn opt_i64_field(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_str().and_then(|value| value.parse::<i64>().ok()))
    })
}

fn bool_field(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn seal_secret(value: &str) -> String {
    if value.is_empty() || value.starts_with("enc:v1:") {
        return value.to_owned();
    }
    let key = env::var("SECRET_ENCRYPTION_KEY")
        .or_else(|_| env::var("JWT_SECRET"))
        .unwrap_or_else(|_| "kairos-platform-local-secret".to_owned());
    let key = key.as_bytes();
    let sealed = value
        .as_bytes()
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ key[index % key.len()])
        .collect::<Vec<_>>();
    format!("enc:v1:{}", BASE64.encode(sealed))
}

fn table_value(value: Value) -> Value {
    let Value::Object(object) = value else {
        return value;
    };
    let mut mapped = serde_json::Map::new();
    for (key, value) in object {
        if key == "deleted" {
            continue;
        }
        mapped.insert(snake_to_camel(&key), parse_jsonish_value(value));
    }
    if mapped.contains_key("parentMenuId") {
        mapped.insert("isParentMenuIdValid".into(), Value::Bool(true));
    }
    Value::Object(mapped)
}

fn camel_payload_to_snake(value: Value) -> Value {
    let Value::Object(object) = value else {
        return json!({});
    };
    let mut mapped = serde_json::Map::new();
    for (key, value) in object {
        if matches!(
            key.as_str(),
            "createTime" | "updateTime" | "isParentMenuIdValid"
        ) {
            continue;
        }
        mapped.insert(camel_to_snake(&key), value);
    }
    Value::Object(mapped)
}

fn parse_jsonish_value(value: Value) -> Value {
    let Value::String(text) = &value else {
        return value;
    };
    if !(text.starts_with('{') || text.starts_with('[')) {
        return value;
    }
    serde_json::from_str(text).unwrap_or(value)
}

fn snake_to_camel(value: &str) -> String {
    let mut output = String::new();
    let mut uppercase = false;
    for character in value.chars() {
        if character == '_' {
            uppercase = true;
        } else if uppercase {
            output.push(character.to_ascii_uppercase());
            uppercase = false;
        } else {
            output.push(character);
        }
    }
    output
}

fn camel_to_snake(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars() {
        if character.is_ascii_uppercase() {
            output.push('_');
            output.push(character.to_ascii_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

fn pascal_case(value: &str) -> String {
    let camel = snake_to_camel(value);
    let mut chars = camel.chars();
    match chars.next() {
        Some(first) => format!(
            "{}{}",
            first.to_ascii_uppercase(),
            chars.collect::<String>()
        ),
        None => "Generated".into(),
    }
}

fn java_type(data_type: &str) -> &'static str {
    match data_type {
        "bigint" => "Long",
        "integer" | "smallint" => "Integer",
        "boolean" => "Boolean",
        "timestamp without time zone" | "timestamp with time zone" => "LocalDateTime",
        "date" => "LocalDate",
        _ => "String",
    }
}

fn html_type(data_type: &str) -> &'static str {
    match data_type {
        "boolean" => "radio",
        "timestamp without time zone" | "timestamp with time zone" | "date" => "datetime",
        _ => "input",
    }
}

async fn ok_bool() -> Json<ApiResponse<bool>> {
    Json(ApiResponse::new(true))
}
