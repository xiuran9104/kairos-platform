use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_runtime::ports::SkillStore;

#[derive(Clone)]
pub struct PgSkillStore {
    pool: PgPool,
}

impl PgSkillStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl SkillStore for PgSkillStore {
    async fn execute(&self, tool_name: &str, arguments: &str) -> anyhow::Result<serde_json::Value> {
        let arguments = serde_json::from_str::<serde_json::Value>(arguments)
            .map_err(|error| anyhow::anyhow!("内联目录工具参数不是有效 JSON: {error}"))?;
        match tool_name {
            "tool_search" => {
                let query = arguments
                    .get("query")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("tool_search 缺少 query"))?;
                let limit = arguments
                    .get("limit")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(12) as usize;
                crate::conversation::tool_catalog::search_tool_catalog(&self.pool, query, limit)
                    .await
            }
            "skill_list" => {
                let category = arguments
                    .get("category")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let query = arguments
                    .get("query")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let limit = arguments
                    .get("limit")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(20);
                crate::skills::list_published_skills(&self.pool, category, query, limit)
                    .await
                    .map_err(anyhow::Error::msg)
            }
            "skill_load" => {
                let name = arguments
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("skill_load 缺少 name"))?;
                crate::skills::load_published_skill(&self.pool, name)
                    .await
                    .map_err(anyhow::Error::msg)
            }
            _ => anyhow::bail!("未知内联目录工具: {tool_name}"),
        }
    }
}
