use crate::ScanState;
use axum::{
    Json,
    extract::{Path, State},
};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_web::AppError;
use kairos_platform_scan_api::{CreateMachineRoomRequest, MachineRoom, UpdateMachineRoomRequest};
use serde_json::{Value, json};

pub async fn list(
    State(s): State<ScanState>,
) -> Result<Json<ApiResponse<Vec<MachineRoom>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_machine_rooms t WHERE deleted=0 ORDER BY id",
    )
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    let list: Vec<MachineRoom> = rows
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    Ok(Json(ApiResponse::new(list)))
}

pub async fn get_one(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<MachineRoom>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_machine_rooms t WHERE id=$1 AND deleted=0",
    )
    .bind(id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(
        serde_json::from_value(v).map_err(|e| AppError::internal(e.to_string()))?,
    )))
}

pub async fn create(
    State(s): State<ScanState>,
    Json(req): Json<CreateMachineRoomRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO scan_machine_rooms(room_name,room_code,facility_type,address,platform_id,room_type,contact_person,contact_phone,floor,cabinet_count,area_size,remarks,status) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) RETURNING id"
    ).bind(&req.room_name).bind(&req.room_code).bind(&req.facility_type).bind(&req.address)
     .bind(req.platform_id).bind(&req.room_type).bind(&req.contact_person).bind(&req.contact_phone)
     .bind(&req.floor).bind(req.cabinet_count).bind(&req.area_size).bind(&req.remarks)
     .bind(req.status.as_deref().unwrap_or("active"))
     .fetch_one(&s.pool).await.map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message": "机房创建成功", "id": id}),
    )))
}

pub async fn update(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
    Json(req): Json<UpdateMachineRoomRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let rows = sqlx::query("UPDATE scan_machine_rooms SET room_name=COALESCE($2,room_name),room_code=COALESCE($3,room_code),facility_type=COALESCE($4,facility_type),address=COALESCE($5,address),platform_id=COALESCE($6,platform_id),room_type=COALESCE($7,room_type),contact_person=COALESCE($8,contact_person),contact_phone=COALESCE($9,contact_phone),floor=COALESCE($10,floor),cabinet_count=COALESCE($11,cabinet_count),area_size=COALESCE($12,area_size),remarks=COALESCE($13,remarks),status=COALESCE($14,status),updated_at=now() WHERE id=$1 AND deleted=0")
        .bind(id).bind(&req.room_name).bind(&req.room_code).bind(&req.facility_type).bind(&req.address)
        .bind(req.platform_id).bind(&req.room_type).bind(&req.contact_person).bind(&req.contact_phone)
        .bind(&req.floor).bind(req.cabinet_count).bind(&req.area_size).bind(&req.remarks).bind(&req.status)
        .execute(&s.pool).await.map_err(|e| AppError::bad_request(e.to_string()))?;
    if rows.rows_affected() == 0 {
        return Err(AppError::not_found("not found"));
    }
    Ok(Json(ApiResponse::new(json!({"message": "机房更新成功"}))))
}

pub async fn delete_one(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query(
        "UPDATE scan_machine_rooms SET deleted=1,updated_at=now() WHERE id=$1 AND deleted=0",
    )
    .bind(id)
    .execute(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message": "机房删除成功"}))))
}
