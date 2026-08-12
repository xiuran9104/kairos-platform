use crate::ScanState;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use kairos_platform_scan_api::ApproveTicketRequest;
use serde_json::{Value, json};

pub struct ApprovalPolicy {
    pub risk_level: &'static str,
    pub roles: &'static [&'static str],
    pub requires_target: bool,
    pub requires_window: bool,
    pub requires_backup: bool,
    pub requires_rollback: bool,
}

pub fn policy(ticket_type: &str) -> ApprovalPolicy {
    match ticket_type {
        "migration" => ApprovalPolicy {
            risk_level: "high",
            roles: &["业务负责人", "资源管理员", "运维负责人"],
            requires_target: true,
            requires_window: true,
            requires_backup: true,
            requires_rollback: true,
        },
        "stop" => ApprovalPolicy {
            risk_level: "high",
            roles: &["业务负责人", "运维负责人", "执行确认人"],
            requires_target: true,
            requires_window: true,
            requires_backup: false,
            requires_rollback: true,
        },
        "start" => ApprovalPolicy {
            risk_level: "medium",
            roles: &["资源管理员"],
            requires_target: true,
            requires_window: false,
            requires_backup: false,
            requires_rollback: false,
        },
        "reclaim" => ApprovalPolicy {
            risk_level: "critical",
            roles: &[
                "业务负责人",
                "部门负责人",
                "资源管理员",
                "运维负责人",
                "最终回收确认人",
            ],
            requires_target: true,
            requires_window: true,
            requires_backup: true,
            requires_rollback: true,
        },
        "resize" | "disk_expand" | "network_change" | "renew" => ApprovalPolicy {
            risk_level: "medium",
            roles: &["部门负责人", "资源管理员"],
            requires_target: true,
            requires_window: false,
            requires_backup: false,
            requires_rollback: false,
        },
        _ => ApprovalPolicy {
            risk_level: "normal",
            roles: &["部门负责人", "资源管理员"],
            requires_target: false,
            requires_window: false,
            requires_backup: false,
            requires_rollback: false,
        },
    }
}

pub async fn approve(
    state: &ScanState,
    user: &CurrentUser,
    ticket_id: i32,
    request: ApproveTicketRequest,
) -> Result<Value, AppError> {
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    let row:(String,String,i32,i32)=sqlx::query_as("SELECT ticket_status,ticket_type,approval_stage,approval_total FROM scan_resource_tickets WHERE id=$1 AND deleted=0 FOR UPDATE")
        .bind(ticket_id).fetch_optional(&mut *tx).await.map_err(|error|AppError::internal(error.to_string()))?.ok_or_else(||AppError::not_found("工单不存在"))?;
    if row.0 != "pending_approval" {
        return Err(AppError::bad_request("只能审批待审批状态的工单"));
    }
    let rules = policy(&row.1);
    let index = (row.2 - 1).max(0) as usize;
    let role = rules.roles.get(index).copied().unwrap_or("审批人");
    require_stage_role(user, role)?;
    sqlx::query("INSERT INTO scan_resource_ticket_approvals(ticket_id,stage_no,role_name,approver,decision,comment) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(ticket_id).bind(row.2).bind(role).bind(&user.username).bind(if request.approved{"approved"}else{"rejected"}).bind(&request.comment).execute(&mut *tx).await.map_err(|error|AppError::internal(error.to_string()))?;
    if !request.approved {
        sqlx::query("UPDATE scan_resource_tickets SET ticket_status='rejected',approver=$2,approve_time=now(),approve_comment=$3,current_approval_role=NULL WHERE id=$1").bind(ticket_id).bind(&user.username).bind(&request.comment).execute(&mut *tx).await.map_err(|error|AppError::internal(error.to_string()))?;
    } else if row.2 >= row.3 {
        sqlx::query("UPDATE scan_resource_tickets SET ticket_status='pending_provision',approver=$2,approve_time=now(),approve_comment=$3,current_approval_role=NULL WHERE id=$1").bind(ticket_id).bind(&user.username).bind(&request.comment).execute(&mut *tx).await.map_err(|error|AppError::internal(error.to_string()))?;
    } else {
        let next_stage = row.2 + 1;
        let next_role = rules
            .roles
            .get((next_stage - 1) as usize)
            .copied()
            .unwrap_or("审批人");
        sqlx::query("UPDATE scan_resource_tickets SET approval_stage=$2,current_approval_role=$3,approve_comment=$4 WHERE id=$1").bind(ticket_id).bind(next_stage).bind(next_role).bind(&request.comment).execute(&mut *tx).await.map_err(|error|AppError::internal(error.to_string()))?;
    }
    tx.commit()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(
        json!({"message":if request.approved&&row.2<row.3{"本级审批完成，已进入下一级审批"}else{"审批完成"},"stage":row.2,"total":row.3}),
    )
}

fn require_stage_role(user: &CurrentUser, role: &str) -> Result<(), AppError> {
    if user.role_codes.iter().any(|code| code == "super_admin") {
        return Ok(());
    }
    let required = match role {
        "业务负责人" => "business_owner",
        "部门负责人" => "department_owner",
        "资源管理员" => "resource_admin",
        "运维负责人" | "执行确认人" | "最终回收确认人" => "operations_owner",
        _ => "resource_approver",
    };
    if user.role_codes.iter().any(|code| code == required) {
        Ok(())
    } else {
        Err(AppError::forbidden(format!("当前审批节点需要角色：{role}")))
    }
}
