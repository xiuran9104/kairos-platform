//! Worker-reported tool availability used by model catalog disclosure.

use std::collections::HashSet;

use kairos_platform_framework_database::PgPool;

/// Return tools that have recent health observations but no healthy worker.
/// Unknown tools remain eligible so a fresh deployment can auto-install them
/// on first use; only explicit, recent unhealthy observations hide a tool.
pub(super) async fn recently_unavailable_tools(pool: &PgPool) -> anyhow::Result<HashSet<String>> {
    let ttl_minutes = std::env::var("PENTA_TOOL_HEALTH_TTL_MINUTES")
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(10)
        .clamp(1, 1_440);
    let rows = sqlx::query_as::<_, (String, bool, i64)>(
        "SELECT tool_id,
                bool_or(status='healthy') AS any_healthy,
                count(*) FILTER (WHERE status IN ('healthy','unhealthy')) AS observations
         FROM pent_tool_health
         WHERE checked_at >= now() - make_interval(mins => $1)
           AND status IN ('healthy','unhealthy')
         GROUP BY tool_id",
    )
    .bind(ttl_minutes)
    .fetch_all(pool)
    .await?;
    Ok(unavailable_from_observations(rows))
}

fn unavailable_from_observations(rows: Vec<(String, bool, i64)>) -> HashSet<String> {
    rows.into_iter()
        .filter(|(_, any_healthy, observations)| !*any_healthy && *observations > 0)
        .map(|(tool_id, _, _)| tool_id.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_only_explicitly_unhealthy_tools_without_a_healthy_worker() {
        let unavailable = unavailable_from_observations(vec![
            ("nuclei".into(), false, 2),
            ("sqlmap".into(), true, 3),
            ("unknown".into(), false, 0),
        ]);
        assert!(unavailable.contains("nuclei"));
        assert!(!unavailable.contains("sqlmap"));
        assert!(!unavailable.contains("unknown"));
    }
}
