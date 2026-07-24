//! Dashboard summary — ported from rustset dashboard handler.

use axum::{Json, extract::State};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_web::AppError;
use kairos_platform_scan_api::DashboardSummary;

use crate::ScanState;

pub async fn summary(
    State(state): State<ScanState>,
) -> Result<Json<ApiResponse<DashboardSummary>>, AppError> {
    let zones = state.zones.lock().await;
    let assets = state.assets.lock().await;
    let tasks = state.tasks.lock().await;
    let tickets = state.resource_tickets.lock().await;
    let rooms = state.machine_rooms.lock().await;
    let products = state.security_products.lock().await;

    let running_tasks = tasks
        .iter()
        .filter(|t| matches!(t.status, kairos_platform_scan_api::TaskStatus::Running))
        .count() as u64;

    let scanned_assets = assets.iter().filter(|a| a.last_scanned.is_some()).count() as u64;

    let summary = DashboardSummary {
        asset_count: assets.len() as u64,
        scanned_asset_count: scanned_assets,
        task_count: tasks.len() as u64,
        running_task_count: running_tasks,
        risk_count: 0,
        open_risk_count: 0,
        database_connected: true,
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let _ = (&zones, &tickets, &rooms, &products);
    Ok(Json(ApiResponse::new(summary)))
}
