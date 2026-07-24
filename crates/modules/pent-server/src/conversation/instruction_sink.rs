use async_trait::async_trait;
use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::{AgentTurnRequest, ConversationIntent};

use super::ConversationInstructionSink;

pub struct InstructionSink {
    pool: PgPool,
}

impl InstructionSink {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ConversationInstructionSink for InstructionSink {
    async fn submit(
        &self,
        run_id: &str,
        request: &AgentTurnRequest,
        intent: &ConversationIntent,
    ) -> anyhow::Result<String> {
        let instruction_type = serde_json::to_value(&intent.kind)?
            .as_str()
            .unwrap_or("steer")
            .to_owned();
        if intent.requires_confirmation {
            anyhow::bail!("该运行指令需要显式确认");
        }
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO pent_run_instructions(
                 id, session_id, run_id, instruction_type, content, payload,
                 status, created_by
             ) VALUES($1,$2,$3,$4,$5,$6,'pending','admin')",
        )
        .bind(&id)
        .bind(&request.session_id)
        .bind(run_id)
        .bind(instruction_type)
        .bind(&request.content)
        .bind(serde_json::json!({"client_request_id": request.client_request_id}))
        .execute(&self.pool)
        .await?;
        Ok(id)
    }
}
