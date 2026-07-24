//! HPM-10: RMOS Lesson Pipeline — candidate -> validated -> promoted.
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
        title: &str,
        content: &str,
        source: &str,
    ) -> anyhow::Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO pent_agent_lessons(id,run_id,title,content,source,status,created_at) VALUES($1,$2,$3,$4,$5,'candidate',now())")
            .bind(&id).bind(run_id).bind(title).bind(content).bind(source).execute(&self.pool).await?;
        Ok(id)
    }

    pub async fn validate(&self, lesson_id: &str, validated: bool) -> anyhow::Result<()> {
        let status = if validated { "validated" } else { "rejected" };
        sqlx::query("UPDATE pent_agent_lessons SET status=$2,validated_at=now() WHERE id=$1")
            .bind(lesson_id)
            .bind(status)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn promote(&self, lesson_id: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE pent_agent_lessons SET status='promoted',promoted_at=now() WHERE id=$1 AND status='validated'").bind(lesson_id).execute(&self.pool).await?;
        Ok(())
    }
}
