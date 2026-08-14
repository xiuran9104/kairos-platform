//! Import YAML tool recipes into `pent_tool_definitions`.
//!
//! Usage:
//!   DATABASE_URL=postgres://... import_tool_recipes [path-to-recipes.yaml]
//!   import_tool_recipes --check [path-to-recipes.yaml]   # validate only
//!
//! The recipes file defaults to `tools/recipes.yaml` inside the pent-server
//! crate. All recipes are validated before any database write happens; the
//! import runs in a single transaction.

use kairos_platform_pent_server::infrastructure::recipe::{
    ExecutableRecipe, Recipe, parse_recipe_file,
};
use serde_json::{Value, json};

fn default_recipes_path() -> String {
    format!("{}/tools/recipes.yaml", env!("CARGO_MANIFEST_DIR"))
}

fn executor_json(recipe: &Recipe) -> Value {
    if recipe.executor.kind == "builtin" {
        return json!({
            "type": "builtin",
            "handler": recipe.tool_id,
        });
    }
    json!({
        "type": recipe.executor.kind,
        "command": recipe.executor.program,
        "args": recipe.executor.args,
        "target_position": match recipe.executor.target_position {
            kairos_platform_pent_server::infrastructure::recipe::TargetPosition::First => "first",
            kairos_platform_pent_server::infrastructure::recipe::TargetPosition::Last => "last",
        },
        "normalize_target": match recipe.executor.normalize_target {
            kairos_platform_pent_server::infrastructure::recipe::NormalizeTarget::UrlHttps => "url_https",
            kairos_platform_pent_server::infrastructure::recipe::NormalizeTarget::None => "none",
        },
        "target_type": match recipe.executor.target_type {
            kairos_platform_pent_server::infrastructure::recipe::TargetType::File => "file",
            kairos_platform_pent_server::infrastructure::recipe::TargetType::Network => "network",
        },
        "parameters": recipe.parameters,
    })
}

fn resource_profile_json(recipe: &Recipe) -> Value {
    json!({
        "cpu_cores": recipe.resource_profile.cpu_cores,
        "memory_mb": recipe.resource_profile.memory_mb,
        "timeout_seconds": recipe.resource_profile.timeout_seconds,
    })
}

/// Use the explicit `input_schema` when present; otherwise derive it from the
/// recipe's `parameters` so the model knows the structured arguments.
fn input_schema_for(recipe: &Recipe) -> Value {
    if !recipe.input_schema.is_null() && recipe.input_schema != json!({}) {
        return recipe.input_schema.clone();
    }
    kairos_platform_pent_server::infrastructure::recipe::parameters_to_input_schema(
        &recipe.parameters,
    )
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let check_only = args.iter().any(|arg| arg == "--check");
    let path = args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .cloned()
        .unwrap_or_else(default_recipes_path);

    let content = std::fs::read_to_string(&path)
        .map_err(|error| anyhow::anyhow!("读取配方文件 {path} 失败: {error}"))?;
    let recipes = parse_recipe_file(&content).map_err(|error| anyhow::anyhow!(error))?;
    let executable = recipes
        .iter()
        .filter(|recipe| ExecutableRecipe::from_recipe(recipe).is_some())
        .count();
    println!(
        "配方校验通过: {} 条（{} 条可执行，{} 条 builtin 元数据）",
        recipes.len(),
        executable,
        recipes.len() - executable
    );
    if check_only {
        return Ok(());
    }

    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL is required"))?;
    let pool = sqlx::PgPool::connect(&database_url).await?;

    let mut tx = pool.begin().await?;
    for recipe in &recipes {
        sqlx::query(
            "INSERT INTO pent_tool_definitions(
                 tool_id, version, manifest_version, namespace, name, description,
                 capabilities, tags, source, input_schema, output_schema,
                 risk_level, approval_policy, supported_target_types,
                 executor, resource_profile, lifecycle_status, embedding_document
             ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)
             ON CONFLICT (tool_id, version) DO UPDATE SET
                 manifest_version=EXCLUDED.manifest_version,
                 namespace=EXCLUDED.namespace,
                 name=EXCLUDED.name,
                 description=EXCLUDED.description,
                 capabilities=EXCLUDED.capabilities,
                 tags=EXCLUDED.tags,
                 source=EXCLUDED.source,
                 input_schema=EXCLUDED.input_schema,
                 output_schema=EXCLUDED.output_schema,
                 risk_level=EXCLUDED.risk_level,
                 approval_policy=EXCLUDED.approval_policy,
                 supported_target_types=EXCLUDED.supported_target_types,
                 executor=EXCLUDED.executor,
                 resource_profile=EXCLUDED.resource_profile,
                 lifecycle_status=EXCLUDED.lifecycle_status,
                 embedding_document=EXCLUDED.embedding_document,
                 revision=pent_tool_definitions.revision+1,
                 updated_at=now()",
        )
        .bind(&recipe.tool_id)
        .bind(&recipe.version)
        .bind(recipe.manifest_version)
        .bind(&recipe.namespace)
        .bind(&recipe.name)
        .bind(&recipe.description)
        .bind(json!(recipe.capabilities))
        .bind(json!(recipe.tags))
        .bind(&recipe.source)
        .bind(input_schema_for(recipe))
        .bind(&recipe.output_schema)
        .bind(&recipe.risk_level)
        .bind(&recipe.approval_policy)
        .bind(json!(recipe.supported_target_types))
        .bind(executor_json(recipe))
        .bind(resource_profile_json(recipe))
        .bind(&recipe.lifecycle_status)
        .bind(&recipe.embedding_document)
        .execute(&mut *tx)
        .await
        .map_err(|error| anyhow::anyhow!("导入配方 {} 失败: {error}", recipe.tool_id))?;
    }
    tx.commit().await?;
    println!("已导入 {} 条配方到 pent_tool_definitions", recipes.len());
    Ok(())
}
