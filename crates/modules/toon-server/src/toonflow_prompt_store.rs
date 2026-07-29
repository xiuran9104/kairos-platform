use sqlx::PgPool;

/// Load a prompt template from the store, falling back to `fallback`.
pub(crate) async fn load(pool: &PgPool, key: &str, fallback: &str) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT data FROM toonflow.prompts WHERE source_key=$1 AND data<>'' ORDER BY id DESC LIMIT 1",
    )
    .bind(key)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| fallback.to_string())
}

/// Load the project's art style prefix. Returns empty string if the
/// project has no style configured.
pub(crate) async fn load_style_prefix(pool: &PgPool, project_id: i64) -> String {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT a.prefix FROM toonflow.projects p
         LEFT JOIN toonflow.art_styles a ON a.name = p.art_style
         WHERE p.id = $1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    row.and_then(|(prefix,)| prefix)
        .filter(|p| !p.is_empty())
        .unwrap_or_default()
}

/// Inject the project's art style prefix into a prompt. The prefix is
/// prepended so it takes precedence in the model's interpretation.
/// This is the single injection point — all image/video generation must
/// go through here.
pub(crate) fn inject_style_prefix(prompt: &str, prefix: &str) -> String {
    if prefix.is_empty() || prompt.contains(prefix) {
        return prompt.to_string();
    }
    format!("{prefix}，{prompt}")
}

/// Resolve the seed for a generation call following the cascade:
/// 1. Explicit seed override (Some)
/// 2. Project global_seed
/// 3. Random
pub(crate) async fn resolve_seed(
    pool: &PgPool,
    project_id: Option<i64>,
    explicit: Option<i64>,
) -> i64 {
    if let Some(seed) = explicit {
        return seed;
    }
    if let Some(pid) = project_id {
        let global: Option<i64> = sqlx::query_scalar(
            "SELECT global_seed FROM toonflow.projects WHERE id = $1",
        )
        .bind(pid)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        if let Some(seed) = global {
            return seed;
        }
    }
    // Random seed from timestamp + simple hash (no rand dependency needed)
    let t = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    (t ^ (t >> 17) ^ (t << 13)).abs() % 9_007_199_254_740_991
}
