use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{PentState, transport::mq::RemoteToolExecutionRequest};

const ACTIVE_VALIDATION_TOOLS: &[&str] = &[
    "nuclei",
    "sqli_validate",
    "xss_validate",
    "csrf_validate",
    "command_injection_validate",
    "file_inclusion_validate",
    "file_upload_validate",
];

#[derive(Debug, Deserialize)]
struct CreateVerificationRequest {
    tool_name: String,
    target: String,
    authorization_statement: String,
}

#[derive(Debug, Deserialize)]
struct ApprovalRequest {
    authorization_confirmed: bool,
}

pub fn routes() -> Router<PentState> {
    Router::new()
        .route("/pent/scan-risk-verifications", get(list_verifications))
        .route(
            "/pent/scan-risks/{risk_id}/verifications",
            post(create_verification),
        )
        .route(
            "/pent/scan-risk-verifications/{id}/approve",
            post(approve_verification),
        )
        .route(
            "/pent/scan-risk-verifications/{id}/reject",
            post(reject_verification),
        )
        .route(
            "/pent/scan-risk-verifications/{id}/execute",
            post(execute_verification),
        )
}

async fn list_verifications(
    State(state): State<PentState>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(v) || jsonb_build_object(
             'risk', jsonb_build_object('asset_ip', r.asset_ip, 'port', r.port,
                                         'status', r.status, 'description', r.description)
         )
         FROM scan_risk_verifications v
         JOIN scan_risks r ON r.id=v.risk_id
         WHERE r.deleted=0
         ORDER BY v.created_at DESC LIMIT 200",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn create_verification(
    State(state): State<PentState>,
    Path(risk_id): Path<String>,
    user: CurrentUser,
    Json(body): Json<CreateVerificationRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tool_name = body.tool_name.trim();
    if !ACTIVE_VALIDATION_TOOLS.contains(&tool_name) {
        return Err(AppError::bad_request("该工具不在主动漏洞验证允许列表中"));
    }
    let statement = body.authorization_statement.trim();
    if statement.chars().count() < 12 {
        return Err(AppError::bad_request("授权声明至少需要 12 个字符"));
    }
    let target = body.target.trim();
    let risk: Option<(String, i32)> = sqlx::query_as(
        "SELECT asset_ip,port FROM scan_risks
         WHERE id=$1 AND deleted=0 AND status IN ('open','pending_review')",
    )
    .bind(&risk_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    let (asset_ip, port) =
        risk.ok_or_else(|| AppError::bad_request("风险不存在、已删除，或当前状态不允许主动验证"))?;
    validate_target(target, &asset_ip, port)?;

    let id = Uuid::new_v4();
    let row = sqlx::query_scalar::<_, Value>(
        "INSERT INTO scan_risk_verifications(
             id,risk_id,tool_name,target,authorization_statement,requested_by
         ) VALUES($1,$2,$3,$4,$5,$6)
         RETURNING to_jsonb(scan_risk_verifications)",
    )
    .bind(id)
    .bind(&risk_id)
    .bind(tool_name)
    .bind(target)
    .bind(statement)
    .bind(&user.user_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::with_message(
        row,
        "主动验证申请已提交，等待人工批准",
    )))
}

async fn approve_verification(
    State(state): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<ApprovalRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    if !body.authorization_confirmed {
        return Err(AppError::bad_request("必须明确确认已获得目标所有者授权"));
    }
    let row = sqlx::query_scalar::<_, Value>(
        "UPDATE scan_risk_verifications
         SET status='approved',approved_by=$2,approved_at=now(),updated_at=now()
         WHERE id::text=$1 AND status='pending_approval'
         RETURNING to_jsonb(scan_risk_verifications)",
    )
    .bind(&id)
    .bind(&user.user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .ok_or_else(|| AppError::bad_request("验证申请不存在或已处理"))?;
    Ok(Json(ApiResponse::with_message(
        row,
        "授权已确认，可以执行主动验证",
    )))
}

async fn reject_verification(
    State(state): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let row = sqlx::query_scalar::<_, Value>(
        "UPDATE scan_risk_verifications
         SET status='rejected',rejected_by=$2,rejected_at=now(),updated_at=now()
         WHERE id::text=$1 AND status IN ('pending_approval','approved')
         RETURNING to_jsonb(scan_risk_verifications)",
    )
    .bind(&id)
    .bind(&user.user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .ok_or_else(|| AppError::bad_request("验证申请不存在或无法拒绝"))?;
    Ok(Json(ApiResponse::with_message(row, "主动验证申请已拒绝")))
}

async fn execute_verification(
    State(state): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let execution_uuid = Uuid::new_v4();
    let execution_id = execution_uuid.to_string();
    let row = sqlx::query_as::<_, (String, String, String, String, i32, String)>(
        "UPDATE scan_risk_verifications v
         SET status='running',execution_id=$2,executed_by=$3,executed_at=now(),updated_at=now()
         FROM scan_risks r
         WHERE v.id::text=$1 AND v.status='approved' AND r.id=v.risk_id
           AND r.deleted=0 AND r.status IN ('open','pending_review')
         RETURNING v.risk_id,v.tool_name,v.target,r.asset_ip,r.port,
                   COALESCE(v.approved_by,'')",
    )
    .bind(&id)
    .bind(execution_uuid)
    .bind(&user.user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .ok_or_else(|| AppError::bad_request("验证未获批准、正在执行，或风险状态已变化"))?;
    let (risk_id, tool_name, target, asset_ip, port, approved_by) = row;
    if let Err(error) = validate_target(&target, &asset_ip, port) {
        mark_failed(&state, &id, "批准后风险目标发生变化").await?;
        return Err(error);
    }
    let bus = state
        .nats
        .as_ref()
        .ok_or_else(|| AppError::internal("pent-agent 执行通道不可用"))?;
    let request = RemoteToolExecutionRequest {
        execution_id: execution_id.clone(),
        tool_name: tool_name.clone(),
        arguments: json!({"target": target}),
        scope_snapshot: json!({
            "targets": [target],
            "confirmed": true,
            "confirmed_at": chrono::Utc::now(),
            "confirmed_by": approved_by,
            "authorization_kind": "scan_risk_active_verification",
            "verification_id": id
        }),
        session_id: format!("scan-verification-{id}"),
        run_id: None,
        traffic_run_id: Some(id.clone()),
        timeout_seconds: 600,
        secret_environment: Default::default(),
    };
    let response = match bus.request_tool_execution(&request).await {
        Ok(response) => response,
        Err(error) => {
            mark_failed(&state, &id, &error).await?;
            return Err(AppError::internal(error));
        }
    };
    if let Some(error) = response.error {
        mark_failed(&state, &id, &error).await?;
        return Err(AppError::internal(error));
    }
    let output = response
        .output
        .ok_or_else(|| AppError::internal("pent-agent 未返回验证结果"))?;
    let confirmed = output.exit_code == 0 && contains_nuclei_match(&output.stdout);
    let result = json!({
        "confirmed": confirmed,
        "stdout": output.stdout,
        "stderr": output.stderr,
        "exit_code": output.exit_code,
        "duration_ms": output.duration_ms,
        "audit": output.audit
    });
    let mut transaction = state
        .pool
        .begin()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    sqlx::query(
        "UPDATE scan_risk_verifications
         SET status='completed',result=$2,error=NULL,updated_at=now() WHERE id::text=$1",
    )
    .bind(&id)
    .bind(&result)
    .execute(&mut *transaction)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    if confirmed {
        sqlx::query(
            "UPDATE scan_risks
             SET status='verified',updated_at=now(),
                 evidence=evidence || jsonb_build_object(
                    'active_verification_id',$2::text,
                    'active_verification_tool',$3::text,
                    'active_verification_at',now()
                 )
             WHERE id=$1 AND deleted=0",
        )
        .bind(&risk_id)
        .bind(&id)
        .bind(&tool_name)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    }
    transaction
        .commit()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::with_message(
        json!({"id": id, "execution_id": execution_id, "result": result}),
        if confirmed {
            "主动验证命中，风险已标记为已确认"
        } else {
            "主动验证完成但未发现可确认的命中，风险保持待审核"
        },
    )))
}

async fn mark_failed(state: &PentState, id: &str, error: &str) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE scan_risk_verifications SET status='failed',error=$2,updated_at=now() WHERE id::text=$1",
    )
    .bind(id)
    .bind(error)
    .execute(&state.pool)
    .await
    .map_err(|db_error| AppError::internal(db_error.to_string()))?;
    Ok(())
}

fn validate_target(target: &str, asset_ip: &str, risk_port: i32) -> Result<(), AppError> {
    let (host, port) = if target.contains("://") {
        let url = reqwest::Url::parse(target)
            .map_err(|_| AppError::bad_request("主动验证目标不是有效 URL"))?;
        let host = url
            .host_str()
            .ok_or_else(|| AppError::bad_request("主动验证 URL 缺少主机"))?;
        (host.to_string(), url.port_or_known_default().map(i32::from))
    } else if let Some((host, port)) = target.rsplit_once(':') {
        match port.parse::<i32>() {
            Ok(port) => (host.trim_matches(['[', ']']).to_string(), Some(port)),
            Err(_) => (target.to_string(), None),
        }
    } else {
        (target.to_string(), None)
    };
    if !host.eq_ignore_ascii_case(asset_ip) {
        return Err(AppError::bad_request("主动验证目标必须与风险资产完全一致"));
    }
    if port.is_some_and(|port| port != risk_port) {
        return Err(AppError::bad_request("主动验证目标端口必须与风险端口一致"));
    }
    Ok(())
}

fn contains_nuclei_match(stdout: &str) -> bool {
    stdout.lines().any(|line| {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return false;
        };
        value.get("matched-at").and_then(Value::as_str).is_some()
            || value.get("matched_at").and_then(Value::as_str).is_some()
            || (value.get("template-id").is_some() && value.get("host").is_some())
    })
}

#[cfg(test)]
mod tests {
    use super::{contains_nuclei_match, validate_target};

    #[test]
    fn target_must_match_asset_and_port() {
        assert!(validate_target("https://example.test:8443/a", "example.test", 8443).is_ok());
        assert!(validate_target("https://other.test:8443/a", "example.test", 8443).is_err());
        assert!(validate_target("https://example.test:443/a", "example.test", 8443).is_err());
    }

    #[test]
    fn only_structured_nuclei_findings_confirm_a_risk() {
        assert!(contains_nuclei_match(
            r#"{"template-id":"cve-test","host":"https://example.test","matched-at":"https://example.test/a"}"#
        ));
        assert!(!contains_nuclei_match("scan completed without findings"));
    }
}
