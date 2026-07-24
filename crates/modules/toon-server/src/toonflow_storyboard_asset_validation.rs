use kairos_platform_framework_web::AppError;

pub async fn reject_base_role_asset_ids(
    pool: &sqlx::PgPool,
    asset_ids: &[i64],
) -> Result<(), AppError> {
    if asset_ids.is_empty() {
        return Ok(());
    }
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM toonflow.assets WHERE id=ANY($1) AND type='role' AND parent_asset_id IS NULL ORDER BY name",
    )
    .bind(asset_ids)
    .fetch_all(pool)
    .await
    .map_err(|_| AppError::internal("failed to validate storyboard role assets"))?;
    reject_names(names)
}

pub async fn reject_base_roles_for_storyboards(
    pool: &sqlx::PgPool,
    storyboard_ids: &[i64],
) -> Result<(), AppError> {
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT a.name FROM toonflow.assets a JOIN toonflow.assets_storyboards ast ON ast.asset_id=a.id WHERE ast.storyboard_id=ANY($1) AND a.type='role' AND a.parent_asset_id IS NULL ORDER BY a.name",
    )
    .bind(storyboard_ids)
    .fetch_all(pool)
    .await
    .map_err(|_| AppError::internal("failed to validate storyboard role assets"))?;
    reject_names(names)
}

fn reject_names(names: Vec<String>) -> Result<(), AppError> {
    if names.is_empty() {
        Ok(())
    } else {
        Err(AppError::bad_request(format!(
            "分镜禁止直接引用基础人物：{}。请改用对应场次的衍生人物形象后再生成。",
            names.join("、")
        )))
    }
}
