//! Unified task system for Toonflow — all async generation operations
//! (event extraction, prompt refinement, image/video/audio generation,
//! video export) go through this module. No domain module should insert
//! or update toonflow.tasks directly.

use kairos_platform_framework_database::PgPool;
use serde_json::Value;

/// Canonical task states. New code must use these constants rather than
/// raw strings.
pub mod state {
    pub const QUEUED: &str = "queued";
    pub const RUNNING: &str = "running";
    pub const SUCCEEDED: &str = "succeeded";
    pub const FAILED: &str = "failed";
    pub const CANCELED: &str = "canceled";
}

pub struct TaskManager {
    pool: PgPool,
}

impl TaskManager {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new task in `queued` state. Returns the task id.
    pub async fn create(
        &self,
        task_class: &str,
        project_id: Option<i64>,
        model: &str,
        description: &str,
        related_objects: &str,
    ) -> anyhow::Result<i64> {
        let id = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO toonflow.tasks(
                 id, project_id, task_class, related_objects, model,
                 description, state, start_time
             ) VALUES($1, $2, $3, $4, $5, $6, $7, $1)",
        )
        .bind(id)
        .bind(project_id)
        .bind(task_class)
        .bind(related_objects)
        .bind(model)
        .bind(description)
        .bind(state::QUEUED)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    /// Transition to running.
    pub async fn set_running(&self, task_id: i64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE toonflow.tasks SET state=$1 WHERE id=$2",
        )
        .bind(state::RUNNING)
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mark task as succeeded with optional result payload and related object info.
    pub async fn set_succeeded(
        &self,
        task_id: i64,
        related_objects: Option<&str>,
        payload: Option<&Value>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE toonflow.tasks
             SET state=$1, finish_time=$2, related_objects=COALESCE($3, related_objects),
                 payload=COALESCE($4, payload), reason=NULL
             WHERE id=$5",
        )
        .bind(state::SUCCEEDED)
        .bind(chrono::Utc::now().timestamp_millis())
        .bind(related_objects)
        .bind(payload)
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mark task as failed with a structured reason.
    pub async fn set_failed(&self, task_id: i64, reason: &str) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE toonflow.tasks
             SET state=$1, reason=$2, finish_time=$3
             WHERE id=$4",
        )
        .bind(state::FAILED)
        .bind(reason)
        .bind(chrono::Utc::now().timestamp_millis())
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Request cooperative cancellation. The task will be marked canceled
    /// by the next check point in the execution loop.
    pub async fn request_cancel(&self, task_id: i64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE toonflow.tasks SET cancel_requested=true WHERE id=$1",
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mark as canceled and record finish time.
    pub async fn set_canceled(&self, task_id: i64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE toonflow.tasks
             SET state=$1, finish_time=$2
             WHERE id=$3",
        )
        .bind(state::CANCELED)
        .bind(chrono::Utc::now().timestamp_millis())
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Check if cancellation was requested. Long-running loops should call
    /// this periodically and abort when true.
    pub async fn is_cancel_requested(&self, task_id: i64) -> anyhow::Result<bool> {
        let flag: Option<bool> = sqlx::query_scalar(
            "SELECT cancel_requested FROM toonflow.tasks WHERE id=$1",
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(flag.unwrap_or(false))
    }

    /// Create a retry task linked to the original. Returns the new task id.
    pub async fn retry(&self, original_task_id: i64) -> anyhow::Result<i64> {
        let row: Option<(String, Option<i64>, String, String, String)> = sqlx::query_as(
            "SELECT task_class, project_id, model, description, related_objects
             FROM toonflow.tasks WHERE id=$1",
        )
        .bind(original_task_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some((task_class, project_id, model, description, related)) = row else {
            anyhow::bail!("task {original_task_id} not found");
        };
        let new_id = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO toonflow.tasks(
                 id, project_id, task_class, related_objects, model,
                 description, state, start_time, retry_of
             ) VALUES($1, $2, $3, $4, $5, $6, $7, $1, $8)",
        )
        .bind(new_id)
        .bind(project_id)
        .bind(&task_class)
        .bind(&related)
        .bind(&model)
        .bind(&description)
        .bind(state::QUEUED)
        .bind(original_task_id)
        .execute(&self.pool)
        .await?;
        Ok(new_id)
    }

    /// Update progress (0-100). Only updates when the value actually changes
    /// to reduce write pressure.
    pub async fn update_progress(&self, task_id: i64, progress: i32) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE toonflow.tasks SET progress=$1 WHERE id=$2 AND progress<>$1",
        )
        .bind(progress.clamp(0, 100))
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
