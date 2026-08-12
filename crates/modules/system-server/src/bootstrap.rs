use anyhow::{Context, bail};
use tracing::warn;

use crate::SystemState;

pub async fn initialize(state: &SystemState) -> anyhow::Result<()> {
    let administrators = sqlx::query_scalar::<_, i64>(
        "SELECT count(*)
         FROM system_users u
         JOIN system_user_role ur ON ur.user_id = u.id AND ur.deleted = 0
         JOIN system_role r ON r.id = ur.role_id AND r.deleted = 0
         WHERE u.deleted = 0 AND u.status = 0 AND r.status = 0
           AND r.code = 'super_admin'",
    )
    .fetch_one(&state.pool)
    .await
    .context("failed to verify Yudao administrator")?;
    if administrators == 0 {
        bail!("Yudao system_users has no enabled super administrator");
    }
    tracing::info!(administrators, "Yudao administrators are ready");

    initialize_admin_password(state).await?;

    Ok(())
}

/// Set the initial admin password from `BOOTSTRAP_ADMIN_PASSWORD` when the
/// account's password is still empty (e.g. freshly restored from the baseline
/// SQL snapshot).  This only fires once — after the first successful run the
/// password column is no longer empty and the block is skipped.
async fn initialize_admin_password(state: &SystemState) -> anyhow::Result<()> {
    let username = std::env::var("BOOTSTRAP_ADMIN_USERNAME")
        .unwrap_or_else(|_| "admin".into())
        .trim()
        .to_string();
    if username.is_empty() {
        return Ok(());
    }

    let password = std::env::var("BOOTSTRAP_ADMIN_PASSWORD")
        .unwrap_or_default()
        .trim()
        .to_string();
    if password.is_empty() {
        warn!(
            "BOOTSTRAP_ADMIN_PASSWORD is not set; admin password will remain empty \
             and login will fail until manually configured"
        );
        return Ok(());
    }

    // Only update if the password column is still empty — this is idempotent
    // across restarts so we don't accidentally overwrite a changed password.
    let row = sqlx::query_scalar::<_, String>(
        "SELECT password FROM system_users WHERE username = $1 AND deleted = 0 AND status = 0",
    )
    .bind(&username)
    .fetch_optional(&state.pool)
    .await
    .context("failed to look up admin user for password bootstrap")?;

    let Some(current_password) = row else {
        tracing::info!(%username, "BOOTSTRAP_ADMIN_USERNAME not found; skipping password bootstrap");
        return Ok(());
    };

    if !current_password.is_empty() {
        tracing::info!(%username, "admin password is already set; skipping password bootstrap");
        return Ok(());
    }

    let hash = state
        .passwords
        .hash_secret(&password)
        .context("failed to hash bootstrap admin password")?;

    sqlx::query("UPDATE system_users SET password = $1 WHERE username = $2 AND deleted = 0")
        .bind(&hash)
        .bind(&username)
        .execute(&state.pool)
        .await
        .context("failed to persist bootstrap admin password")?;

    tracing::info!(%username, "bootstrap admin password initialized");
    Ok(())
}
