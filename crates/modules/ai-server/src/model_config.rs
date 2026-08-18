use kairos_platform_framework_database::PgPool;
use kairos_platform_framework_web::AppError;
use sqlx::Row;

pub(crate) async fn delete(pool: &PgPool, id: i64) -> Result<(), AppError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to start AI model deletion"))?;

    let model = sqlx::query(
        "SELECT key FROM ai.model_configs
         WHERE id=$1 AND deleted=false
         FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| AppError::internal("failed to load AI model"))?
    .ok_or_else(|| AppError::not_found("AI model not found"))?;

    // Runtime configuration is disposable. Historical AI rows keep their model
    // foreign key and can still display the retained tombstone metadata.
    sqlx::query("DELETE FROM pent_model_role_bindings WHERE model_config_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to clear AI model bindings"))?;
    sqlx::query(
        "UPDATE pent_model_role_bindings
         SET fallback_model_config_ids=COALESCE(
               (SELECT jsonb_agg(item.value)
                FROM jsonb_array_elements(fallback_model_config_ids) AS item(value)
                WHERE item.value <> to_jsonb($1::bigint)),
               '[]'::jsonb
             ),
             revision=revision+1,
             updated_at=now()
         WHERE fallback_model_config_ids @> jsonb_build_array($1::bigint)",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::internal("failed to clear AI fallback model bindings"))?;

    sqlx::query(
        "UPDATE pent_runtime_policies
         SET hitl_audit_model_id=NULL, updated_at=now()
         WHERE hitl_audit_model_id=$1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::internal("failed to clear AI audit model binding"))?;

    for path in [
        &[
            "platform_settings",
            "knowledge",
            "embedding_model_config_id",
        ][..],
        &["platform_settings", "knowledge", "rerank_model_config_id"][..],
    ] {
        sqlx::query(
            "UPDATE pent_runtime_policies
             SET agent_config=jsonb_set(agent_config,$2,'0'::jsonb,false), updated_at=now()
             WHERE agent_config #>> $2 = $1::text",
        )
        .bind(id)
        .bind(path)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to clear AI knowledge model binding"))?;
    }

    sqlx::query(
        "UPDATE pent_agent_sessions
         SET constraints=constraints-'model_id', updated_at=now()
         WHERE constraints->>'model_id'=$1::text",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::internal("failed to clear AI session model binding"))?;

    let original_key: String = model.get("key");
    let result = sqlx::query(
        "UPDATE ai.model_configs
         SET key=$2, api_key='', status=1, deleted=true, update_time=$3
         WHERE id=$1 AND deleted=false",
    )
    .bind(id)
    .bind(deleted_key(id))
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::internal("failed to delete AI model"))?;
    debug_assert_eq!(result.rows_affected(), 1, "locked model must still exist");
    tracing::info!(model_id = id, model_key = %original_key, "soft-deleted AI model");

    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed to commit AI model deletion"))?;
    Ok(())
}

fn deleted_key(id: i64) -> String {
    format!("deleted:{id}")
}

#[cfg(test)]
mod tests {
    use super::deleted_key;

    #[test]
    fn tombstone_key_is_stable_and_frees_original_key() {
        assert_eq!(deleted_key(42), "deleted:42");
    }
}
