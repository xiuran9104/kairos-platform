//! YAML tool recipes — parsing, validation, template rendering and the
//! runtime recipe store.
//!
//! Recipes are authored as YAML (see `tools/recipes.yaml`), imported into
//! `pent_tool_definitions` by `import_tool_recipes`, and loaded back into an
//! in-memory store at gateway startup. `tool_registry::prepare_tool` consults
//! the store first and falls back to the hardcoded whitelist when a tool has
//! no executable recipe.
//!
//! Safety rails (kept even though recipes may declare program/args):
//! - no shell: program + args are passed as an execve array;
//! - `program` must be an absolute path under an allowed prefix
//!   (`PENTA_RECIPE_PROGRAM_PREFIXES`, default `/usr/bin/,/usr/local/bin/`);
//! - targets still go through `tool_registry::validate_target` (scope policy,
//!   reserved addresses, NUL/newline/`-` injection);
//! - rendered args reject empty strings and `\0`/`\r`/`\n`.

use super::process::NetworkPolicy;
use super::tool_registry::{PreparedTool, validate_target};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// ── YAML schema ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct RecipeFile {
    pub recipes: Vec<Recipe>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Recipe {
    pub tool_id: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_manifest_version")]
    pub manifest_version: i32,
    pub namespace: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default)]
    pub input_schema: serde_json::Value,
    #[serde(default)]
    pub output_schema: serde_json::Value,
    #[serde(default = "default_risk_level")]
    pub risk_level: String,
    #[serde(default = "default_approval_policy")]
    pub approval_policy: String,
    #[serde(default)]
    pub supported_target_types: Vec<String>,
    pub executor: RecipeExecutor,
    #[serde(default)]
    pub resource_profile: RecipeResourceProfile,
    #[serde(default = "default_lifecycle_status")]
    pub lifecycle_status: String,
    #[serde(default)]
    pub embedding_document: String,
    /// DesRed-style declarative parameter mapping. When present, the executor
    /// renders the command from the model's structured JSON arguments using
    /// each parameter's flag/position/format/template instead of the legacy
    /// `{{target}}`/`{{argN}}` template.
    #[serde(default)]
    pub parameters: Vec<ParameterConfig>,
}

/// One named parameter of a tool recipe, mirroring DesRed's `ParameterConfig`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ParameterConfig {
    pub name: String,
    #[serde(rename = "type", default = "default_param_type")]
    pub type_: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub item_type: Option<String>,
    #[serde(default)]
    pub flag: Option<String>,
    #[serde(default)]
    pub position: Option<usize>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub options: Vec<String>,
    /// Marks the parameter that carries the network target (scope-validated).
    #[serde(default)]
    pub target: bool,
}

fn default_param_type() -> String {
    "string".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecipeExecutor {
    #[serde(rename = "type", default = "default_executor_type")]
    pub kind: String,
    #[serde(default)]
    pub program: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub target_position: TargetPosition,
    #[serde(default)]
    pub normalize_target: NormalizeTarget,
    #[serde(default)]
    pub target_type: TargetType,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetPosition {
    #[default]
    Last,
    First,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizeTarget {
    #[default]
    None,
    /// Trim a trailing slash and add `https://` when no scheme is present.
    UrlHttps,
}

/// What kind of value the recipe's `{{target}}` represents.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
    /// A network endpoint (host/IP/URL) subject to Scope/NetworkPolicy checks.
    #[default]
    Network,
    /// A local file path; only basic safety checks apply, no network policy.
    File,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RecipeResourceProfile {
    #[serde(default = "default_cpu_cores")]
    pub cpu_cores: f64,
    #[serde(default = "default_memory_mb")]
    pub memory_mb: u64,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
}

impl Default for RecipeResourceProfile {
    fn default() -> Self {
        Self {
            cpu_cores: default_cpu_cores(),
            memory_mb: default_memory_mb(),
            timeout_seconds: default_timeout_seconds(),
        }
    }
}

fn default_version() -> String {
    "1.0.0".into()
}
fn default_manifest_version() -> i32 {
    1
}
fn default_source() -> String {
    "builtin".into()
}
fn default_risk_level() -> String {
    "low".into()
}
fn default_approval_policy() -> String {
    "conditional".into()
}
fn default_executor_type() -> String {
    "host".into()
}
fn default_lifecycle_status() -> String {
    "active".into()
}
fn default_cpu_cores() -> f64 {
    0.5
}
fn default_memory_mb() -> u64 {
    256
}
fn default_timeout_seconds() -> u64 {
    300
}

// ── Validation ──────────────────────────────────────────────────────────

const RISK_LEVELS: &[&str] = &["low", "medium", "high", "critical"];
const APPROVAL_POLICIES: &[&str] = &["never", "conditional", "always"];
const SOURCES: &[&str] = &["builtin", "external"];
const EXECUTOR_TYPES: &[&str] = &["host", "builtin"];
const LIFECYCLE_STATUSES: &[&str] = &["draft", "active", "disabled", "deprecated"];

fn allowed_program_prefixes() -> Vec<String> {
    std::env::var("PENTA_RECIPE_PROGRAM_PREFIXES")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|prefix| !prefix.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|prefixes: &Vec<String>| !prefixes.is_empty())
        .unwrap_or_else(|| {
            vec![
                "/usr/bin/".into(),
                "/usr/local/bin/".into(),
                "/usr/sbin/".into(),
            ]
        })
}

fn validate_tool_id(tool_id: &str) -> Result<(), String> {
    let valid = !tool_id.is_empty()
        && tool_id.len() <= 100
        && tool_id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        && tool_id
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    if valid {
        Ok(())
    } else {
        Err(format!("tool_id '{tool_id}' 格式非法"))
    }
}

fn validate_program(program: &str) -> Result<(), String> {
    if !program.starts_with('/') {
        return Err(format!("program '{program}' 必须是绝对路径"));
    }
    if program.chars().any(char::is_whitespace) || program.contains('\0') {
        return Err(format!("program '{program}' 含非法字符"));
    }
    let prefixes = allowed_program_prefixes();
    if prefixes.iter().any(|prefix| program.starts_with(prefix)) {
        Ok(())
    } else {
        Err(format!(
            "program '{program}' 不在允许前缀内（{}）",
            prefixes.join(", ")
        ))
    }
}

/// Scan a template for `{{...}}` placeholders and check each is known.
fn validate_template(template: &str) -> Result<(), String> {
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let end = rest[start..]
            .find("}}")
            .ok_or_else(|| format!("模板占位符未闭合: {template}"))?;
        let key = rest[start + 2..start + end].trim();
        let known = key == "target"
            || key
                .strip_prefix("arg")
                .is_some_and(|idx| !idx.is_empty() && idx.chars().all(|c| c.is_ascii_digit()));
        if !known {
            return Err(format!("未知占位符 {{{{{key}}}}}"));
        }
        rest = &rest[start + end + 2..];
    }
    if rest.contains("}}") {
        return Err(format!("模板包含未匹配的 '}}': {template}"));
    }
    Ok(())
}

/// Validate a single recipe. Returns an error string describing every rule
/// the recipe violates.
pub fn validate_recipe(recipe: &Recipe) -> Result<(), String> {
    validate_tool_id(&recipe.tool_id)?;
    if recipe.name.trim().is_empty() {
        return Err(format!("{}: name 不能为空", recipe.tool_id));
    }
    if recipe.description.trim().is_empty() {
        return Err(format!("{}: description 不能为空", recipe.tool_id));
    }
    if !RISK_LEVELS.contains(&recipe.risk_level.as_str()) {
        return Err(format!("{}: risk_level 非法", recipe.tool_id));
    }
    if !APPROVAL_POLICIES.contains(&recipe.approval_policy.as_str()) {
        return Err(format!("{}: approval_policy 非法", recipe.tool_id));
    }
    if !SOURCES.contains(&recipe.source.as_str()) {
        return Err(format!("{}: source 非法", recipe.tool_id));
    }
    if !LIFECYCLE_STATUSES.contains(&recipe.lifecycle_status.as_str()) {
        return Err(format!("{}: lifecycle_status 非法", recipe.tool_id));
    }
    if !EXECUTOR_TYPES.contains(&recipe.executor.kind.as_str()) {
        return Err(format!("{}: executor.type 非法", recipe.tool_id));
    }
    if recipe.executor.kind == "builtin" {
        return Ok(());
    }
    let program = recipe
        .executor
        .program
        .as_deref()
        .ok_or_else(|| format!("{}: 可执行配方缺少 program", recipe.tool_id))?;
    validate_program(program).map_err(|e| format!("{}: {e}", recipe.tool_id))?;
    if recipe.executor.args.is_empty() && recipe.parameters.is_empty() {
        return Err(format!("{}: 可执行配方缺少 args", recipe.tool_id));
    }
    for arg in &recipe.executor.args {
        validate_template(arg).map_err(|e| format!("{}: {e}", recipe.tool_id))?;
    }
    Ok(())
}

/// Parse and validate a YAML recipe file.
pub fn parse_recipe_file(content: &str) -> Result<Vec<Recipe>, String> {
    let file: RecipeFile =
        serde_yaml::from_str(content).map_err(|error| format!("YAML 解析失败: {error}"))?;
    let mut seen = HashMap::new();
    for recipe in &file.recipes {
        validate_recipe(recipe)?;
        if seen.insert(recipe.tool_id.clone(), recipe.version.clone()).is_some() {
            return Err(format!("重复配方: {}@{}", recipe.tool_id, recipe.version));
        }
    }
    Ok(file.recipes)
}

// ── Runtime executable recipes ──────────────────────────────────────────

/// A recipe resolved to an executable form held in the runtime store.
#[derive(Debug, Clone)]
pub struct ExecutableRecipe {
    pub program: &'static str,
    pub args: Vec<String>,
    pub target_position: TargetPosition,
    pub normalize_target: NormalizeTarget,
    pub target_type: TargetType,
    pub parameters: Vec<ParameterConfig>,
    pub resource_profile: RecipeResourceProfile,
}

impl ExecutableRecipe {
    pub fn from_recipe(recipe: &Recipe) -> Option<Self> {
        if recipe.executor.kind == "builtin" {
            return None;
        }
        let program = recipe.executor.program.as_deref()?;
        if recipe.executor.args.is_empty() && recipe.parameters.is_empty() {
            return None;
        }
        Some(
            Self::new(
                program,
                recipe.executor.args.clone(),
                recipe.executor.target_position,
                recipe.executor.normalize_target,
                recipe.executor.target_type,
                recipe.parameters.clone(),
            )
            .with_resource_profile(recipe.resource_profile.clone()),
        )
    }

    fn new(
        program: &str,
        args: Vec<String>,
        target_position: TargetPosition,
        normalize_target: NormalizeTarget,
        target_type: TargetType,
        parameters: Vec<ParameterConfig>,
    ) -> Self {
        // Program strings come from validated recipes; the set is bounded, so
        // leaking to obtain a &'static str for PreparedTool is acceptable.
        let program: &'static str = Box::leak(program.to_owned().into_boxed_str());
        Self {
            program,
            args,
            target_position,
            normalize_target,
            target_type,
            parameters,
            resource_profile: RecipeResourceProfile::default(),
        }
    }

    pub fn with_resource_profile(mut self, profile: RecipeResourceProfile) -> Self {
        self.resource_profile = profile;
        self
    }

    /// Build an executable recipe from a `pent_tool_definitions.executor` JSON
    /// document produced by the importer.
    pub fn from_executor_json(executor: &serde_json::Value) -> Option<Self> {
        let program = executor.get("command")?.as_str()?;
        let args: Vec<String> = executor
            .get("args")
            .and_then(serde_json::Value::as_array)
            .map(|array| {
                array
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let parameters: Vec<ParameterConfig> = executor
            .get("parameters")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        if args.is_empty() && parameters.is_empty() {
            return None;
        }
        let target_position = match executor.get("target_position").and_then(|v| v.as_str()) {
            Some("first") => TargetPosition::First,
            _ => TargetPosition::Last,
        };
        let normalize_target = match executor.get("normalize_target").and_then(|v| v.as_str()) {
            Some("url_https") => NormalizeTarget::UrlHttps,
            _ => NormalizeTarget::None,
        };
        let target_type = match executor.get("target_type").and_then(|v| v.as_str()) {
            Some("file") => TargetType::File,
            _ => TargetType::Network,
        };
        Some(Self::new(
            program,
            args,
            target_position,
            normalize_target,
            target_type,
            parameters,
        ))
    }
}

// ── Global store ────────────────────────────────────────────────────────

static RECIPE_STORE: RwLock<Option<Arc<HashMap<String, ExecutableRecipe>>>> = RwLock::new(None);

pub fn set_global_recipes(recipes: HashMap<String, ExecutableRecipe>) {
    let mut guard = RECIPE_STORE.write().expect("recipe store poisoned");
    *guard = Some(Arc::new(recipes));
}

/// Look up the executable recipe for a tool, if one was loaded.
pub fn lookup(tool_name: &str) -> Option<ExecutableRecipe> {
    RECIPE_STORE
        .read()
        .expect("recipe store poisoned")
        .as_ref()
        .and_then(|store| store.get(tool_name).cloned())
}

/// Whether an executable recipe exists for this tool.
pub fn has_executable(tool_name: &str) -> bool {
    RECIPE_STORE
        .read()
        .expect("recipe store poisoned")
        .as_ref()
        .is_some_and(|store| store.contains_key(tool_name))
}

/// Load executable recipes from `pent_tool_definitions` into the global store.
/// Returns the number of recipes loaded.
pub async fn load_from_db(
    pool: &kairos_platform_framework_database::PgPool,
) -> anyhow::Result<usize> {
    let rows: Vec<(String, serde_json::Value, serde_json::Value)> = sqlx::query_as(
        "SELECT tool_id, executor, resource_profile FROM pent_tool_definitions
         WHERE lifecycle_status = 'active'
           AND executor ? 'args'
           AND executor ? 'command'",
    )
    .fetch_all(pool)
    .await?;
    let mut recipes = HashMap::new();
    for (tool_id, executor, resource_profile) in rows {
        if let Some(recipe) = ExecutableRecipe::from_executor_json(&executor) {
            let profile = serde_json::from_value(resource_profile).unwrap_or_default();
            let recipe = recipe.with_resource_profile(profile);
            recipes.insert(tool_id, recipe);
        }
    }
    let count = recipes.len();
    set_global_recipes(recipes);
    Ok(count)
}

// ── Template rendering ──────────────────────────────────────────────────

fn render_arg(template: &str, target: &str, request: &[String]) -> Result<String, String> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let end = rest[start..]
            .find("}}")
            .ok_or_else(|| "模板占位符未闭合".to_string())?;
        let key = rest[start + 2..start + end].trim();
        let value = if key == "target" {
            target.to_string()
        } else if let Some(idx) = key.strip_prefix("arg") {
            let index: usize = idx
                .parse()
                .map_err(|_| format!("占位符下标非法: {{{{{key}}}}}"))?;
            request
                .get(index)
                .ok_or_else(|| format!("占位符 {{{{{key}}}}} 超出参数范围"))?
                .clone()
        } else {
            return Err(format!("未知占位符 {{{{{key}}}}}"));
        };
        out.push_str(&value);
        rest = &rest[start + end + 2..];
    }
    out.push_str(rest);
    if out.is_empty() {
        return Err("参数渲染结果为空".into());
    }
    if out.contains(['\0', '\r', '\n']) {
        return Err("参数渲染结果包含非法字符".into());
    }
    Ok(out)
}

fn normalize_url_target(target: &str) -> String {
    let target = target.trim().trim_end_matches('/');
    if target.starts_with("http://") || target.starts_with("https://") {
        target.to_string()
    } else {
        format!("https://{target}")
    }
}

/// Basic safety check for a local file target (no network policy applies).
fn validate_file_target(target: &str) -> Result<(), String> {
    if target.is_empty()
        || target.starts_with('-')
        || target.chars().any(char::is_whitespace)
        || target.contains('\0')
    {
        return Err("文件路径不安全".into());
    }
    Ok(())
}

/// Render an executable recipe into a `PreparedTool`, applying target
/// extraction, normalization, scope validation and template substitution.
pub fn render_recipe(
    recipe: &ExecutableRecipe,
    request: &[String],
    network_policy: &NetworkPolicy,
) -> Result<PreparedTool, String> {
    // Recipes with a `parameters` mapping render from structured arguments.
    // The flattened `request` only carries the target, so reconstruct a minimal
    // JSON object and delegate to the structured renderer.
    if !recipe.parameters.is_empty() {
        let raw_target = match recipe.target_position {
            TargetPosition::First => request.first(),
            TargetPosition::Last => request.last(),
        }
        .map(String::as_str)
        .ok_or_else(|| "工具调用缺少目标".to_string())?;
        let target_name = recipe
            .parameters
            .iter()
            .find(|p| p.target)
            .map(|p| p.name.clone())
            .or_else(|| recipe.parameters.first().map(|p| p.name.clone()))
            .unwrap_or_else(|| "target".to_string());
        let args = serde_json::json!({ target_name: raw_target });
        return render_recipe_from_json(recipe, &args, network_policy);
    }
    let raw_target = match recipe.target_position {
        TargetPosition::First => request.first(),
        TargetPosition::Last => request.last(),
    }
    .map(String::as_str)
    .ok_or_else(|| "工具调用缺少目标".to_string())?;
    let target = match recipe.normalize_target {
        NormalizeTarget::None => raw_target.to_string(),
        NormalizeTarget::UrlHttps => normalize_url_target(raw_target),
    };
    match recipe.target_type {
        TargetType::File => validate_file_target(&target)?,
        TargetType::Network => validate_target(&target, network_policy)?,
    }
    let args = recipe
        .args
        .iter()
        .map(|arg| render_arg(arg, &target, request))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PreparedTool {
        program: recipe.program,
        args,
    })
}

/// Render a recipe from the model's *structured* JSON arguments using the
/// recipe's `parameters` declarative mapping (DesRed-style). Each parameter is
/// mapped to the command via its `format` (`flag` / `positional` / `combined` /
/// `template`). The parameter marked `target: true` (or conventionally named
/// `target`/`url`/`host`) is scope-validated.
pub fn render_recipe_from_json(
    recipe: &ExecutableRecipe,
    args: &serde_json::Value,
    network_policy: &NetworkPolicy,
) -> Result<PreparedTool, String> {
    let fixed: Vec<String> = recipe
        .args
        .iter()
        .filter(|arg| !arg.contains("{{"))
        .cloned()
        .collect();
    let mut command: Vec<String> = fixed;
    let mut positional: Vec<(usize, String)> = Vec::new();
    let mut target: Option<String> = None;

    for param in &recipe.parameters {
        let value = match param_value(param, args)? {
            Some(value) => value,
            None => continue,
        };
        if param.target
            || matches!(
                param.name.as_str(),
                "target" | "url" | "host" | "domain" | "ip" | "hostname"
            )
        {
            target = Some(value.clone());
        }
        match param.format.as_deref().unwrap_or("flag") {
            "positional" => {
                let position = param.position.unwrap_or(command.len());
                positional.push((position, value));
            }
            "combined" => {
                let flag = param.flag.as_deref().unwrap_or("");
                command.push(format!("{flag}={value}"));
            }
            "template" => {
                let template = param.template.as_deref().unwrap_or("{value}");
                let rendered = template
                    .replace("{value}", &value)
                    .replace("{flag}", param.flag.as_deref().unwrap_or(""))
                    .replace("{name}", &param.name);
                for part in rendered.split_whitespace() {
                    command.push(part.to_string());
                }
            }
            _ => {
                if let Some(flag) = param.flag.as_deref() {
                    if !flag.is_empty() {
                        command.push(flag.to_string());
                    }
                }
                command.push(value);
            }
        }
    }
    for (position, value) in positional {
        let position = position.min(command.len());
        command.insert(position, value);
    }

    match recipe.target_type {
        TargetType::File => {
            if let Some(target) = &target {
                validate_file_target(target)?;
            }
        }
        TargetType::Network => {
            let target = target
                .as_deref()
                .ok_or_else(|| "参数映射缺少网络目标".to_string())?;
            let normalized = match recipe.normalize_target {
                NormalizeTarget::None => target.to_string(),
                NormalizeTarget::UrlHttps => normalize_url_target(target),
            };
            validate_target(&normalized, network_policy)?;
        }
    }

    Ok(PreparedTool {
        program: recipe.program,
        args: command,
    })
}

fn param_value(
    param: &ParameterConfig,
    args: &serde_json::Value,
) -> Result<Option<String>, String> {
    let value = args.get(&param.name);
    let rendered = match value {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        Some(serde_json::Value::Bool(b)) => Some(b.to_string()),
        Some(serde_json::Value::Array(items)) => Some(
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
                .join(","),
        ),
        Some(other) => Some(other.to_string()),
        None | Some(serde_json::Value::Null) => match &param.default {
            Some(default) => Some(value_to_string(default)),
            None if param.required => {
                return Err(format!("缺少必需参数: {}", param.name));
            }
            None => None,
        },
    };
    Ok(rendered.filter(|s| !s.is_empty()))
}

fn value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Derive an OpenAI-style JSON Schema from the recipe's `parameters`, so the
/// model knows the structured arguments to pass.
pub fn parameters_to_input_schema(parameters: &[ParameterConfig]) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    let mut required: Vec<serde_json::Value> = Vec::new();
    for param in parameters {
        let mut schema = serde_json::Map::new();
        schema.insert(
            "type".into(),
            serde_json::json!(json_param_type(&param.type_)),
        );
        if !param.description.is_empty() {
            schema.insert("description".into(), serde_json::json!(param.description));
        }
        if !param.options.is_empty() {
            schema.insert("enum".into(), serde_json::json!(param.options));
        }
        if let Some(default) = &param.default {
            schema.insert("default".into(), default.clone());
        }
        properties.insert(param.name.clone(), serde_json::Value::Object(schema));
        if param.required {
            required.push(serde_json::json!(param.name));
        }
    }
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

fn json_param_type(type_: &str) -> &str {
    match type_ {
        "int" | "integer" | "number" | "float" => "number",
        "bool" | "boolean" => "boolean",
        "array" | "list" => "array",
        _ => "string",
    }
}

/// Load the shipped recipes into the global store. Test-only: unit tests that
/// exercise `prepare_tool` rely on the recipe store instead of the (removed)
/// hardcoded fallback branches.
#[cfg(test)]
pub(crate) fn load_recipes_for_tests() {
    let content = include_str!("../../tools/recipes.yaml");
    let recipes = parse_recipe_file(content).expect("recipes.yaml is valid");
    let map: std::collections::HashMap<String, ExecutableRecipe> = recipes
        .iter()
        .filter_map(|recipe| {
            ExecutableRecipe::from_recipe(recipe).map(|executable| (recipe.tool_id.clone(), executable))
        })
        .collect();
    set_global_recipes(map);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_recipe() -> Recipe {
        Recipe {
            tool_id: "nmap_scan".into(),
            version: "1.0.0".into(),
            manifest_version: 1,
            namespace: "scanner".into(),
            name: "Nmap 端口扫描".into(),
            description: "基于 Nmap 执行端口扫描".into(),
            capabilities: vec!["network".into()],
            tags: vec!["nmap".into()],
            source: "builtin".into(),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: serde_json::json!({}),
            risk_level: "medium".into(),
            approval_policy: "conditional".into(),
            supported_target_types: vec!["hostname".into(), "ipv4".into()],
            executor: RecipeExecutor {
                kind: "host".into(),
                program: Some("/usr/bin/nmap".into()),
                args: vec![
                    "-sT".into(),
                    "-Pn".into(),
                    "--reason".into(),
                    "--".into(),
                    "{{target}}".into(),
                ],
                target_position: TargetPosition::Last,
                normalize_target: NormalizeTarget::None,
                target_type: TargetType::Network,
            },
            resource_profile: RecipeResourceProfile::default(),
            lifecycle_status: "active".into(),
            embedding_document: String::new(),
            parameters: vec![],
        }
    }

    #[test]
    fn executable_recipe_keeps_resource_profile() {
        let mut recipe = sample_recipe();
        recipe.resource_profile = RecipeResourceProfile {
            cpu_cores: 2.0,
            memory_mb: 768,
            timeout_seconds: 900,
        };
        let executable = ExecutableRecipe::from_recipe(&recipe).unwrap();
        assert_eq!(executable.resource_profile.cpu_cores, 2.0);
        assert_eq!(executable.resource_profile.memory_mb, 768);
        assert_eq!(executable.resource_profile.timeout_seconds, 900);
    }

    #[test]
    fn parses_and_validates_recipe_yaml() {
        let yaml = r#"
recipes:
  - tool_id: nmap_scan
    namespace: scanner
    name: Nmap 端口扫描
    description: 基于 Nmap 执行端口扫描
    executor:
      type: host
      program: /usr/bin/nmap
      args: ["-sT", "--", "{{target}}"]
"#;
        let recipes = parse_recipe_file(yaml).unwrap();
        assert_eq!(recipes.len(), 1);
        assert_eq!(recipes[0].tool_id, "nmap_scan");
    }

    #[test]
    fn rejects_unknown_placeholder() {
        let mut recipe = sample_recipe();
        recipe.executor.args.push("{{evil}}".into());
        assert!(validate_recipe(&recipe).is_err());
    }

    #[test]
    fn rejects_program_outside_allowed_prefixes() {
        let mut recipe = sample_recipe();
        recipe.executor.program = Some("/tmp/evil".into());
        assert!(validate_recipe(&recipe).is_err());
    }

    #[test]
    fn rejects_duplicate_tool_ids() {
        let yaml = r#"
recipes:
  - tool_id: nmap_scan
    namespace: scanner
    name: a
    description: a
    executor: {type: host, program: /usr/bin/nmap, args: ["{{target}}"]}
  - tool_id: nmap_scan
    namespace: scanner
    name: b
    description: b
    executor: {type: host, program: /usr/bin/nmap, args: ["{{target}}"]}
"#;
        assert!(parse_recipe_file(yaml).is_err());
    }

    #[test]
    fn renders_target_placeholder_with_scope_validation() {
        let recipe = ExecutableRecipe::from_recipe(&sample_recipe()).unwrap();
        let policy = NetworkPolicy::TargetOnly("example.test".into());
        let prepared = render_recipe(&recipe, &["scan".into(), "example.test".into()], &policy).unwrap();
        assert_eq!(prepared.program, "/usr/bin/nmap");
        assert_eq!(prepared.args.last().unwrap(), "example.test");
        assert!(render_recipe(&recipe, &["scan".into(), "other.test".into()], &policy).is_err());
    }

    #[test]
    fn renders_url_normalization() {
        let mut recipe = sample_recipe();
        recipe.executor.normalize_target = NormalizeTarget::UrlHttps;
        let recipe = ExecutableRecipe::from_recipe(&recipe).unwrap();
        let policy = NetworkPolicy::TargetOnly("example.test".into());
        let prepared = render_recipe(&recipe, &["example.test/".into()], &policy).unwrap();
        assert_eq!(prepared.args.last().unwrap(), "https://example.test");
    }

    #[test]
    fn renders_arg_index_placeholders() {
        let args = vec!["{{arg0}}".into(), "x-{{arg1}}".into()];
        let recipe = ExecutableRecipe::new(
            "/usr/bin/nmap",
            args,
            TargetPosition::Last,
            NormalizeTarget::None,
            TargetType::Network,
            vec![],
        );
        let policy = NetworkPolicy::Full;
        let prepared = render_recipe(&recipe, &["scan".into(), "t".into()], &policy).unwrap();
        assert_eq!(prepared.args, vec!["scan".to_string(), "x-t".to_string()]);
    }

    #[test]
    fn rejects_out_of_range_arg_placeholder() {
        let recipe = ExecutableRecipe::new(
            "/usr/bin/nmap",
            vec!["{{arg5}}".into()],
            TargetPosition::Last,
            NormalizeTarget::None,
            TargetType::Network,
            vec![],
        );
        let policy = NetworkPolicy::Full;
        assert!(render_recipe(&recipe, &["t".into()], &policy).is_err());
    }

    #[test]
    fn builtin_recipes_are_not_executable() {
        let mut recipe = sample_recipe();
        recipe.executor.kind = "builtin".into();
        recipe.executor.program = None;
        recipe.executor.args = vec![];
        assert!(validate_recipe(&recipe).is_ok());
        assert!(ExecutableRecipe::from_recipe(&recipe).is_none());
    }

    #[test]
    fn file_target_skips_network_policy() {
        let recipe = ExecutableRecipe::new(
            "/usr/bin/binwalk",
            vec!["{{target}}".into()],
            TargetPosition::First,
            NormalizeTarget::None,
            TargetType::File,
            vec![],
        );
        // A file path is accepted even under a restrictive target-only policy.
        let policy = NetworkPolicy::TargetOnly("example.com".into());
        let prepared = render_recipe(&recipe, &["/tmp/firmware.bin".into()], &policy).unwrap();
        assert_eq!(prepared.args, vec!["/tmp/firmware.bin".to_string()]);

        // Unsafe file paths are still rejected.
        assert!(render_recipe(&recipe, &["-bad".into()], &policy).is_err());
    }

    fn param_recipe(parameters: Vec<ParameterConfig>) -> ExecutableRecipe {
        ExecutableRecipe::new(
            "/usr/bin/nmap",
            vec!["-sT".into()],
            TargetPosition::Last,
            NormalizeTarget::None,
            TargetType::Network,
            parameters,
        )
    }

    fn p(name: &str, format: &str) -> ParameterConfig {
        ParameterConfig {
            name: name.into(),
            type_: "string".into(),
            description: String::new(),
            required: false,
            default: None,
            item_type: None,
            flag: None,
            position: None,
            format: Some(format.into()),
            template: None,
            options: vec![],
            target: false,
        }
    }

    #[test]
    fn json_rendering_maps_flag_positional_combined_template() {
        let mut target = p("target", "positional");
        target.position = Some(1);
        target.target = true;
        let mut ports = p("ports", "flag");
        ports.flag = Some("-p".into());
        let mut timing = p("timing", "template");
        timing.template = Some("-T{value}".into());
        let mut proto = p("proto", "combined");
        proto.flag = Some("--proto".into());
        let recipe = param_recipe(vec![target, ports, timing, proto]);

        let args = serde_json::json!({
            "target": "example.com", "ports": "80", "timing": "4", "proto": "tcp"
        });
        let prepared = render_recipe_from_json(&recipe, &args, &NetworkPolicy::Full).unwrap();
        assert_eq!(
            prepared.args,
            vec![
                "-sT".to_string(),
                "example.com".to_string(),
                "-p".to_string(),
                "80".to_string(),
                "-T4".to_string(),
                "--proto=tcp".to_string(),
            ]
        );
    }

    #[test]
    fn json_rendering_enforces_scope_on_target() {
        let mut target = p("target", "positional");
        target.position = Some(0);
        target.target = true;
        let recipe = param_recipe(vec![target]);
        let policy = NetworkPolicy::TargetOnly("example.com".into());
        let ok = render_recipe_from_json(
            &recipe,
            &serde_json::json!({"target": "example.com"}),
            &policy,
        );
        assert!(ok.is_ok());
        let bad = render_recipe_from_json(
            &recipe,
            &serde_json::json!({"target": "other.com"}),
            &policy,
        );
        assert!(bad.is_err());
    }

    #[test]
    fn json_rendering_rejects_missing_required() {
        let mut target = p("target", "positional");
        target.required = true;
        let recipe = param_recipe(vec![target]);
        assert!(render_recipe_from_json(&recipe, &serde_json::json!({}), &NetworkPolicy::Full)
            .is_err());
    }

    #[test]
    fn parameters_generate_input_schema() {
        let mut target = p("target", "positional");
        target.required = true;
        target.description = "目标".into();
        let schema = parameters_to_input_schema(&[target]);
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["target"]["type"], "string");
        assert!(schema["required"].as_array().unwrap().iter().any(|v| v == "target"));
    }

    #[test]
    fn json_template_with_flag_splits_into_arguments() {
        let mut target = p("target", "template");
        target.template = Some("-u {value}/FUZZ".into());
        target.target = true;
        let recipe = param_recipe(vec![target]);
        let prepared = render_recipe_from_json(
            &recipe,
            &serde_json::json!({"target": "http://example.com"}),
            &NetworkPolicy::Full,
        )
        .unwrap();
        assert_eq!(
            prepared.args,
            vec!["-sT".to_string(), "-u".to_string(), "http://example.com/FUZZ".to_string()]
        );
    }

    #[test]
    fn store_lookup_and_has_executable() {
        let mut map = HashMap::new();
        map.insert(
            "nmap_scan".to_string(),
            ExecutableRecipe::from_recipe(&sample_recipe()).unwrap(),
        );
        set_global_recipes(map);
        assert!(has_executable("nmap_scan"));
        assert!(!has_executable("nope"));
        assert!(lookup("nmap_scan").is_some());
        // Reset so other tests fall back to the hardcoded registry.
        set_global_recipes(HashMap::new());
        assert!(!has_executable("nmap_scan"));
    }

    #[test]
    fn repository_recipes_file_is_valid_and_covers_registry_tools() {
        let content = include_str!("../../tools/recipes.yaml");
        let recipes = parse_recipe_file(content).unwrap();
        let ids: std::collections::HashSet<&str> =
            recipes.iter().map(|r| r.tool_id.as_str()).collect();
        // Every tool name handled by the hardcoded prepare_tool whitelist must
        // have a recipe (executable, or builtin metadata for http_request).
        for tool in [
            "tcp_scan", "nmap", "nmap_scan", "http_probe", "http_request",
            "nuclei", "web_vuln_scan",
            "sqli_validate", "xss_validate", "csrf_validate",
            "command_injection_validate", "file_inclusion_validate", "file_upload_validate",
            "afrog", "dns_lookup", "ssh_exec", "sql_query",
            "subfinder", "amass", "dnsx", "masscan", "naabu",
            "ffuf", "gobuster", "nikto", "whatweb", "wafw00f", "httpx",
            "wpscan", "web_wordpress_scan",
            "sqlmap", "sqlmap_scan", "xsser", "commix",
            "testssl", "web_tls_scan", "sslscan", "sslyze", "qsslcaudit",
            "dir_fuzz", "dirsearch", "dirb", "feroxbuster", "wfuzz",
            "hakrawler", "katana", "gau", "httrack", "arjun",
            "dnsrecon", "dnsenum", "fierce",
            "dmitry", "theharvester",
            "enum4linux", "smbmap", "smtp-user-enum", "swaks", "davtest",
            "nbtscan", "netdiscover",
            "web_vuln_scan", "web_wordpress_scan", "wapiti", "skipfish", "lbd",
            "dalfox", "web_xss_scan",
        ] {
            assert!(ids.contains(tool), "recipes.yaml 缺少工具 {tool}");
        }
    }

    #[test]
    fn executor_json_round_trip() {
        let executor = serde_json::json!({
            "type": "host",
            "command": "/usr/bin/nmap",
            "args": ["--", "{{target}}"],
            "target_position": "last",
            "normalize_target": "url_https"
        });
        let recipe = ExecutableRecipe::from_executor_json(&executor).unwrap();
        assert_eq!(recipe.program, "/usr/bin/nmap");
        assert_eq!(recipe.normalize_target, NormalizeTarget::UrlHttps);
    }
}
