use crate::ScanState;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_web::AppError;
use kairos_platform_scan_api::{
    Cabinet, CabinetRow, ConfigureMachineRoomCabinetsRequest, CreateCabinetRequest,
    CreateCabinetRowRequest,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
pub struct RowFilter {
    machine_room_id: Option<i32>,
}
#[derive(Deserialize)]
pub struct CabinetFilter {
    row_id: Option<i32>,
}

pub async fn row_list(
    State(s): State<ScanState>,
    Query(filter): Query<RowFilter>,
) -> Result<Json<ApiResponse<Vec<CabinetRow>>>, AppError> {
    let rows=sqlx::query_scalar::<_,Value>("SELECT to_jsonb(r) FROM scan_cabinet_rows r WHERE deleted=0 AND ($1::bigint IS NULL OR machine_room_id=$1) ORDER BY sort,row_code")
        .bind(filter.machine_room_id).fetch_all(&s.pool).await.map_err(|e|AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        rows.into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect(),
    )))
}
pub async fn row_create(
    State(s): State<ScanState>,
    Json(req): Json<CreateCabinetRowRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let id:i32=sqlx::query_scalar("INSERT INTO scan_cabinet_rows(machine_room_id,row_name,row_code,sort,status,remarks) VALUES($1,$2,$3,$4,$5,$6) RETURNING id")
        .bind(req.machine_room_id).bind(&req.row_name).bind(&req.row_code).bind(req.sort.unwrap_or(0)).bind(req.status.as_deref().unwrap_or("active")).bind(&req.remarks)
        .fetch_one(&s.pool).await.map_err(|e|AppError::bad_request(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"id":id,"message":"机柜列创建成功"}),
    )))
}
pub async fn row_update(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
    Json(req): Json<CreateCabinetRowRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_cabinet_rows SET machine_room_id=$2,row_name=$3,row_code=$4,sort=$5,status=$6,remarks=$7,updated_at=now() WHERE id=$1 AND deleted=0")
        .bind(id).bind(req.machine_room_id).bind(&req.row_name).bind(&req.row_code).bind(req.sort.unwrap_or(0)).bind(req.status.as_deref().unwrap_or("active")).bind(&req.remarks)
        .execute(&s.pool).await.map_err(|e|AppError::bad_request(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message":"机柜列更新成功"}))))
}
pub async fn row_delete(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let used: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM scan_cabinets WHERE row_id=$1 AND deleted=0)",
    )
    .bind(id)
    .fetch_one(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    if used {
        return Err(AppError::bad_request("机柜列下仍有机柜，不能删除"));
    }
    sqlx::query("UPDATE scan_cabinet_rows SET deleted=1 WHERE id=$1")
        .bind(id)
        .execute(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message":"机柜列删除成功"}))))
}
pub async fn list(
    State(s): State<ScanState>,
    Query(filter): Query<CabinetFilter>,
) -> Result<Json<ApiResponse<Vec<Cabinet>>>, AppError> {
    let rows=sqlx::query_scalar::<_,Value>("SELECT to_jsonb(c) FROM scan_cabinets c WHERE deleted=0 AND ($1::bigint IS NULL OR row_id=$1) ORDER BY cabinet_code")
        .bind(filter.row_id).fetch_all(&s.pool).await.map_err(|e|AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        rows.into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect(),
    )))
}
pub async fn create(
    State(s): State<ScanState>,
    Json(req): Json<CreateCabinetRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let id:i32=sqlx::query_scalar("INSERT INTO scan_cabinets(row_id,cabinet_name,cabinet_code,total_u,power_capacity_kw,status,remarks) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id")
        .bind(req.row_id).bind(&req.cabinet_name).bind(&req.cabinet_code).bind(req.total_u).bind(req.power_capacity_kw).bind(req.status.as_deref().unwrap_or("active")).bind(&req.remarks)
        .fetch_one(&s.pool).await.map_err(|e|AppError::bad_request(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"id":id,"message":"机柜创建成功"}),
    )))
}
pub async fn update(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
    Json(req): Json<CreateCabinetRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_cabinets SET row_id=$2,cabinet_name=$3,cabinet_code=$4,total_u=$5,power_capacity_kw=$6,status=$7,remarks=$8,updated_at=now() WHERE id=$1 AND deleted=0")
        .bind(id).bind(req.row_id).bind(&req.cabinet_name).bind(&req.cabinet_code).bind(req.total_u).bind(req.power_capacity_kw).bind(req.status.as_deref().unwrap_or("active")).bind(&req.remarks)
        .execute(&s.pool).await.map_err(|e|AppError::bad_request(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message":"机柜更新成功"}))))
}
pub async fn delete_one(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let used: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM scan_business_resources WHERE cabinet_id=$1 AND deleted=0)",
    )
    .bind(id)
    .fetch_one(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    if used {
        return Err(AppError::bad_request("机柜已有服务器占用，不能删除"));
    }
    sqlx::query("UPDATE scan_cabinets SET deleted=1 WHERE id=$1")
        .bind(id)
        .execute(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message":"机柜删除成功"}))))
}

pub async fn configure_room(
    State(s): State<ScanState>,
    Json(req): Json<ConfigureMachineRoomCabinetsRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    if !(1..=26).contains(&req.row_count)
        || !(1..=100).contains(&req.cabinets_per_row)
        || !(1..=60).contains(&req.cabinet_total_u)
    {
        return Err(AppError::bad_request(
            "机柜列须为1-26，每列机柜须为1-100，机柜高度须为1-60U",
        ));
    }
    let occupied: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM scan_business_resources b JOIN scan_cabinets c ON c.id=b.cabinet_id JOIN scan_cabinet_rows r ON r.id=c.row_id WHERE r.machine_room_id=$1 AND b.deleted=0)",
    )
    .bind(req.machine_room_id)
    .fetch_one(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    if occupied {
        return Err(AppError::bad_request(
            "该机房已有服务器占用 U 位，不能整体重新配置",
        ));
    }

    let mut tx = s
        .pool
        .begin()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    sqlx::query("UPDATE scan_cabinets SET deleted=1 WHERE row_id IN (SELECT id FROM scan_cabinet_rows WHERE machine_room_id=$1 AND deleted=0)")
        .bind(req.machine_room_id).execute(&mut *tx).await.map_err(|e|AppError::internal(e.to_string()))?;
    sqlx::query("UPDATE scan_cabinet_rows SET deleted=1 WHERE machine_room_id=$1 AND deleted=0")
        .bind(req.machine_room_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    for row_index in 0..req.row_count {
        let row_code = ((b'A' + row_index as u8) as char).to_string();
        let row_name = format!("{row_code}列");
        let row_id: i64 = sqlx::query_scalar("INSERT INTO scan_cabinet_rows(machine_room_id,row_name,row_code,sort,status) VALUES($1,$2,$3,$4,'active') RETURNING id")
            .bind(req.machine_room_id).bind(row_name).bind(&row_code).bind(row_index)
            .fetch_one(&mut *tx).await.map_err(|e|AppError::bad_request(e.to_string()))?;
        for cabinet_index in 1..=req.cabinets_per_row {
            let cabinet_code = format!("{cabinet_index:02}");
            let cabinet_name = format!("{cabinet_code}号机柜");
            sqlx::query("INSERT INTO scan_cabinets(row_id,cabinet_name,cabinet_code,total_u,status) VALUES($1,$2,$3,$4,'active')")
                .bind(row_id).bind(cabinet_name).bind(cabinet_code).bind(req.cabinet_total_u)
                .execute(&mut *tx).await.map_err(|e|AppError::bad_request(e.to_string()))?;
        }
    }
    tx.commit()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({
        "message":"机房机柜容量配置成功",
        "row_count":req.row_count,
        "cabinet_count":req.row_count*req.cabinets_per_row,
        "total_u":req.row_count*req.cabinets_per_row*req.cabinet_total_u
    }))))
}
