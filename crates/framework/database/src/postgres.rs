use sqlx::{PgPool, postgres::PgPoolOptions};

use crate::DatabaseConfig;

pub async fn connect(config: &DatabaseConfig) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .min_connections(config.min_connections)
        .max_connections(config.max_connections)
        .acquire_timeout(config.acquire_timeout)
        .connect(&config.url)
        .await
}

pub async fn ping(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}

pub async fn ensure_initialized(pool: &PgPool) -> anyhow::Result<()> {
    let initialized: bool =
        sqlx::query_scalar("SELECT to_regclass('public.system_users') IS NOT NULL")
            .fetch_one(pool)
            .await?;
    anyhow::ensure!(
        initialized,
        "database is not initialized; restore sql/postgresql/0001_initial.sql before starting the gateway"
    );
    Ok(())
}
