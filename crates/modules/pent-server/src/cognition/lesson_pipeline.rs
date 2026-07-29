//! HPM-10: RMOS Lesson Pipeline — candidate -> validated -> promoted.
//! Only validated lessons may be promoted into RMOS recall (invariant: RMOS
//! never accepts unverified lessons).
use kairos_platform_framework_database::PgPool;

pub struct LessonPipeline {
    pool: PgPool,
}

impl LessonPipeline {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl LessonPipeline {
    pub async fn create_candidate(
        &self,
        run_id: &str,
        reflection_id: &str,
        namespace: &str,
        capability: &str,
        lesson: &str,
    ) -> anyhow::Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO pent_agent_lessons(
                 id, tenant_id, source_run_id, source_reflection_id, namespace,
                 capability, lesson, confidence, status, created_at, updated_at
             ) VALUES($1,0,$2,$3,$4,$5,$6,0.5,'candidate',now(),now())",
        )
        .bind(&id)
        .bind(run_id)
        .bind(reflection_id)
        .bind(namespace)
        .bind(capability)
        .bind(lesson)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn validate(&self, lesson_id: &str, validated: bool) -> anyhow::Result<()> {
        let status = if validated { "validated" } else { "rejected" };
        sqlx::query(
            "UPDATE pent_agent_lessons
             SET status=$2, validation_count=validation_count+1, updated_at=now()
             WHERE id=$1",
        )
        .bind(lesson_id)
        .bind(status)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn promote(&self, lesson_id: &str) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE pent_agent_lessons SET status='promoted', updated_at=now()
             WHERE id=$1 AND status='validated'",
        )
        .bind(lesson_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
