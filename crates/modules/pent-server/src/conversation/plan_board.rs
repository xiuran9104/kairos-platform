//! Persistent Plan → Execute → Replan task board.

use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_runtime::ports::{PlanProgress, PlanStore};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub struct PgPlanStore {
    pool: PgPool,
}

impl PgPlanStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl PlanStore for PgPlanStore {
    async fn exists(&self, run_id: &str) -> anyhow::Result<bool> {
        exists(&self.pool, run_id).await
    }

    async fn has_executable_step(&self, run_id: &str) -> anyhow::Result<bool> {
        has_executable_step(&self.pool, run_id).await
    }

    async fn progress(&self, run_id: &str) -> anyhow::Result<Option<PlanProgress>> {
        progress(&self.pool, run_id).await
    }

    async fn begin_action(
        &self,
        run_id: &str,
        tool_name: &str,
        tool_call_id: &str,
    ) -> anyhow::Result<Option<String>> {
        begin_action(&self.pool, run_id, tool_name, tool_call_id).await
    }

    async fn finish_action(
        &self,
        run_id: &str,
        node_id: &str,
        success: bool,
        evidence_ids: &[String],
        reason: Option<&str>,
    ) -> anyhow::Result<()> {
        finish_action(&self.pool, run_id, node_id, success, evidence_ids, reason).await
    }

    async fn execute(
        &self,
        run_id: &str,
        tool_name: &str,
        arguments: &str,
    ) -> anyhow::Result<Value> {
        execute(&self.pool, run_id, tool_name, arguments).await
    }
}

pub async fn progress(pool: &PgPool, run_id: &str) -> anyhow::Result<Option<PlanProgress>> {
    let graph: Option<Value> = sqlx::query_scalar(
        "SELECT graph FROM pent_plan_versions WHERE run_id=$1 ORDER BY version DESC LIMIT 1",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await?;
    Ok(graph.map(|graph| progress_from_graph(&graph)))
}

fn progress_from_graph(graph: &Value) -> PlanProgress {
    let mut progress = PlanProgress::default();
    for node in graph
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        progress.total += 1;
        match node.get("status").and_then(Value::as_str).unwrap_or("pending") {
            "ready" | "running" => progress.active += 1,
            "completed" | "skipped" | "cancelled" => progress.completed += 1,
            "failed" => progress.failed += 1,
            "blocked" | "stale" => progress.blocked += 1,
            _ => progress.pending += 1,
        }
    }
    progress
}

async fn begin_action(
    pool: &PgPool,
    run_id: &str,
    tool_name: &str,
    tool_call_id: &str,
) -> anyhow::Result<Option<String>> {
    let current = get(pool, run_id).await?;
    let Some(node_id) = current["nodes"]
        .as_array()
        .and_then(|nodes| nodes.iter().find(|node| {
            matches!(node["status"].as_str(), Some("ready" | "running"))
        }))
        .and_then(|node| node["node_id"].as_str())
        .map(str::to_owned)
    else {
        return Ok(None);
    };
    mutate_node(pool, run_id, &node_id, "runtime_begin_action", |node| {
        node["status"] = json!("running");
        node["attempt"] = json!(node["attempt"].as_u64().unwrap_or(0) + 1);
        node["last_action"] = json!({"tool_name":tool_name,"tool_call_id":tool_call_id,"started_at":chrono::Utc::now()});
    })
    .await?;
    Ok(Some(node_id))
}

async fn finish_action(
    pool: &PgPool,
    run_id: &str,
    node_id: &str,
    success: bool,
    evidence_ids: &[String],
    reason: Option<&str>,
) -> anyhow::Result<()> {
    mutate_node(pool, run_id, node_id, "runtime_finish_action", |node| {
        let refs = node["observation_refs"].as_array_mut();
        if let Some(refs) = refs {
            for evidence_id in evidence_ids {
                if !refs.iter().any(|value| value.as_str() == Some(evidence_id)) {
                    refs.push(json!(evidence_id));
                }
            }
        } else {
            node["observation_refs"] = json!(evidence_ids);
        }
        node["last_action"]["completed_at"] = json!(chrono::Utc::now());
        node["last_action"]["success"] = json!(success);
        if !success {
            node["status"] = json!("blocked");
            node["last_error"] = json!(reason.unwrap_or("工具执行失败"));
        }
    })
    .await?;
    Ok(())
}

async fn mutate_node(
    pool: &PgPool,
    run_id: &str,
    node_id: &str,
    reason: &str,
    mutate: impl FnOnce(&mut Value),
) -> anyhow::Result<Value> {
    let current = get(pool, run_id).await?;
    let mut next = current.clone();
    let nodes = next["nodes"]
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("计划结构损坏"))?;
    let node = nodes
        .iter_mut()
        .find(|node| node["node_id"] == node_id)
        .ok_or_else(|| anyhow::anyhow!("计划步骤不存在"))?;
    mutate(node);
    let version = current["version"].as_i64().unwrap_or(1) + 1;
    next["version"] = json!(version);
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO pent_plan_versions(plan_id,run_id,version,objective,graph,content_hash,created_by) VALUES($1,$2,$3,$4,$5,$6,'runtime')")
        .bind(next["plan_id"].as_str().unwrap_or_default()).bind(run_id).bind(version)
        .bind(next["objective"].as_str().unwrap_or_default()).bind(&next).bind(hash(&next))
        .execute(&mut *tx).await?;
    sqlx::query("INSERT INTO pent_plan_deltas(id,plan_id,run_id,base_version,result_version,replan_level,reason_codes,operations,proposed_by,status,applied_at) VALUES($1,$2,$3,$4,$5,'step',$6,$7,'runtime','applied',now())")
        .bind(uuid::Uuid::new_v4().to_string()).bind(next["plan_id"].as_str().unwrap_or_default())
        .bind(run_id).bind(version-1).bind(version).bind(json!([reason]))
        .bind(json!([{"operation":"runtime_action","node_id":node_id}])).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(next)
}

pub fn definitions() -> Vec<Value> {
    vec![
        tool(
            "plan_create",
            "创建或替换当前 Run 的执行计划。plan_execute 模式必须先调用。",
            json!({"type":"object","required":["objective","steps"],"properties":{"objective":{"type":"string"},"steps":{"type":"array","minItems":1,"items":{"type":"string"}}}}),
        ),
        tool(
            "plan_get",
            "读取当前 Run 最新计划及步骤状态。",
            json!({"type":"object","properties":{}}),
        ),
        tool(
            "plan_update",
            "更新一个计划步骤的状态，并形成不可变的新版本。失败或环境变化时附带 reason。",
            json!({"type":"object","required":["node_id","status"],"properties":{"node_id":{"type":"string"},"status":{"type":"string","enum":["pending","ready","running","completed","failed","skipped","blocked","stale","cancelled"]},"reason":{"type":"string"}}}),
        ),
    ]
}

fn tool(name: &str, description: &str, parameters: Value) -> Value {
    json!({"type":"function","function":{"name":name,"description":description,"parameters":parameters}})
}

pub async fn execute(
    pool: &PgPool,
    run_id: &str,
    name: &str,
    arguments: &str,
) -> anyhow::Result<Value> {
    let args: Value = serde_json::from_str(arguments)?;
    match name {
        "plan_create" => create(pool, run_id, &args).await,
        "plan_get" => get(pool, run_id).await,
        "plan_update" => update(pool, run_id, &args).await,
        _ => anyhow::bail!("unknown plan board tool"),
    }
}

pub async fn exists(pool: &PgPool, run_id: &str) -> anyhow::Result<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pent_plan_versions WHERE run_id=$1)")
            .bind(run_id)
            .fetch_one(pool)
            .await?,
    )
}

pub async fn has_executable_step(pool: &PgPool, run_id: &str) -> anyhow::Result<bool> {
    let graph: Option<Value> = sqlx::query_scalar(
        "SELECT graph FROM pent_plan_versions WHERE run_id=$1 ORDER BY version DESC LIMIT 1",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await?;
    Ok(graph
        .and_then(|value| value.get("nodes").and_then(Value::as_array).cloned())
        .is_some_and(|nodes| {
            nodes.iter().any(|node| {
                matches!(
                    node.get("status").and_then(Value::as_str),
                    Some("ready" | "running")
                )
            })
        }))
}

async fn create(pool: &PgPool, run_id: &str, args: &Value) -> anyhow::Result<Value> {
    let objective = args["objective"]
        .as_str()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("objective 不能为空"))?;
    let steps = args["steps"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("steps 必须是数组"))?;
    if steps.is_empty() || steps.len() > 100 {
        anyhow::bail!("steps 数量必须在 1..=100")
    }
    let plan_id = format!("plan-{}", &uuid::Uuid::new_v4().simple().to_string()[..24]);
    let nodes = steps.iter().enumerate().map(|(index, step)| {
        let text = step.as_str().unwrap_or("未命名步骤");
        json!({
            "node_id": format!("step-{}", index + 1), "node_type":"task", "parent_id":null,
            "namespace":"plan_execute", "objective":text, "capability":null, "target":null,
            "status": if index == 0 {"ready"} else {"pending"}, "priority": (steps.len()-index) as i32,
            "success_criteria":{}, "budget":{"max_actions":20,"max_tool_calls":50,"max_duration_seconds":3600,"max_tokens":100000,"max_cost":0.0,"max_concurrency":1},
            "attempt":0,"max_attempts":3,"observation_refs":[],"valid_until":null
        })
    }).collect::<Vec<_>>();
    let graph = json!({"plan_id":plan_id,"run_id":run_id,"version":1,"objective":objective,"nodes":nodes,"edges":[],"global_budget":{"max_actions":100,"max_tool_calls":500,"max_duration_seconds":21600,"max_tokens":1000000,"max_cost":0.0,"max_concurrency":1},"memory_refs":[],"created_at":chrono::Utc::now()});
    let hash = hash(&graph);
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM pent_plan_deltas WHERE run_id=$1")
        .bind(run_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM pent_plan_versions WHERE run_id=$1")
        .bind(run_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO pent_plan_versions(plan_id,run_id,version,objective,graph,content_hash,created_by) VALUES($1,$2,1,$3,$4,$5,'conversation-agent')")
        .bind(&plan_id).bind(run_id).bind(objective).bind(&graph).bind(hash).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(graph)
}

async fn get(pool: &PgPool, run_id: &str) -> anyhow::Result<Value> {
    sqlx::query_scalar(
        "SELECT graph FROM pent_plan_versions WHERE run_id=$1 ORDER BY version DESC LIMIT 1",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| anyhow::anyhow!("当前 Run 尚无计划"))
}

async fn update(pool: &PgPool, run_id: &str, args: &Value) -> anyhow::Result<Value> {
    let node_id = args["node_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("node_id 不能为空"))?;
    let status = args["status"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("status 不能为空"))?;
    if !matches!(
        status,
        "pending"
            | "ready"
            | "running"
            | "completed"
            | "failed"
            | "skipped"
            | "blocked"
            | "stale"
            | "cancelled"
    ) {
        anyhow::bail!("非法步骤状态")
    }
    let current = get(pool, run_id).await?;
    let mut next = current.clone();
    let nodes = next["nodes"]
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("计划结构损坏"))?;
    let index = nodes
        .iter()
        .position(|n| n["node_id"] == node_id)
        .ok_or_else(|| anyhow::anyhow!("计划步骤不存在"))?;
    nodes[index]["status"] = json!(status);
    if matches!(status, "completed" | "skipped")
        && index + 1 < nodes.len()
        && nodes[index + 1]["status"] == "pending"
    {
        nodes[index + 1]["status"] = json!("ready");
    }
    let version = current["version"].as_i64().unwrap_or(1) + 1;
    next["version"] = json!(version);
    let plan_id = next["plan_id"].as_str().unwrap_or("").to_string();
    let objective = next["objective"].as_str().unwrap_or("").to_string();
    let delta_id = uuid::Uuid::new_v4().to_string();
    let operation = json!([{"operation":"update_node","node_id":node_id,"patch":{"status":status,"reason":args["reason"]}}]);
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO pent_plan_versions(plan_id,run_id,version,objective,graph,content_hash,created_by) VALUES($1,$2,$3,$4,$5,$6,'conversation-agent')")
        .bind(&plan_id).bind(run_id).bind(version).bind(objective).bind(&next).bind(hash(&next)).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO pent_plan_deltas(id,plan_id,run_id,base_version,result_version,replan_level,reason_codes,operations,proposed_by,status,applied_at) VALUES($1,$2,$3,$4,$5,'stage',$6,$7,'conversation-agent','applied',now())")
        .bind(delta_id).bind(&plan_id).bind(run_id).bind(version-1).bind(version).bind(json!([args["reason"]])).bind(operation).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(next)
}

fn hash(value: &Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_plan_lifecycle_for_completion_gate() {
        let progress = progress_from_graph(&json!({"nodes":[
            {"status":"completed"}, {"status":"skipped"}, {"status":"failed"}
        ]}));
        assert_eq!(progress.total, 3);
        assert_eq!(progress.completed, 2);
        assert_eq!(progress.failed, 1);
        assert!(progress.is_complete());

        let active = progress_from_graph(&json!({"nodes":[{"status":"ready"}]}));
        assert!(!active.is_complete());
    }

    #[tokio::test]
    async fn persists_plan_versions_and_replan_delta() {
        let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
            return;
        };
        let pool = PgPool::connect(&url).await.unwrap();
        let run_id = format!("plan-test-{}", uuid::Uuid::new_v4());
        sqlx::query("INSERT INTO pent_runs(id,task_id,tenant_id,user_id) VALUES($1,'plan-test-task',0,'test')")
            .bind(&run_id).execute(&pool).await.unwrap();
        let created = create(
            &pool,
            &run_id,
            &json!({"objective":"test","steps":["recon","validate"]}),
        )
        .await
        .unwrap();
        assert_eq!(created["version"], 1);
        assert!(has_executable_step(&pool, &run_id).await.unwrap());
        let updated = update(
            &pool,
            &run_id,
            &json!({"node_id":"step-1","status":"completed","reason":"evidence collected"}),
        )
        .await
        .unwrap();
        assert_eq!(updated["version"], 2);
        assert_eq!(updated["nodes"][1]["status"], "ready");
        let deltas: i64 =
            sqlx::query_scalar("SELECT count(*) FROM pent_plan_deltas WHERE run_id=$1")
                .bind(&run_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(deltas, 1);
        sqlx::query("DELETE FROM pent_plan_deltas WHERE run_id=$1")
            .bind(&run_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM pent_plan_versions WHERE run_id=$1")
            .bind(&run_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM pent_runs WHERE id=$1")
            .bind(&run_id)
            .execute(&pool)
            .await
            .unwrap();
    }
}
