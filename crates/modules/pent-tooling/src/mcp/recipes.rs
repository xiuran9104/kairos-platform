//! DesRed-compatible YAML recipes for host tools.
//!
//! This module parses and projects recipes without granting execution authority.
//! Infrastructure adapters must enforce their own lifecycle and sandbox policy.

use std::{collections::HashMap, fs, path::Path};

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::protocol::ToolDef;

#[derive(Debug, Clone, Deserialize)]
pub struct ToolRecipe {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub short_description: String,
    /// Retained for compatibility with DesRed recipes. Execution deliberately
    /// resolves the binary through the lifecycle registry instead.
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub parameters: Vec<RecipeParameter>,
    #[serde(default)]
    pub input_schema: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecipeParameter {
    pub name: String,
    #[serde(default = "default_string_type")]
    pub r#type: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
}

fn default_string_type() -> String {
    "string".into()
}

impl ToolRecipe {
    pub fn definition(&self) -> ToolDef {
        ToolDef {
            name: self.name.clone(),
            description: if self.short_description.is_empty() {
                self.description.clone()
            } else {
                self.short_description.clone()
            },
            input_schema: self.input_schema.clone().unwrap_or_else(|| {
                let mut properties = Map::new();
                let mut required = Vec::new();
                for parameter in &self.parameters {
                    if parameter.name.trim().is_empty() {
                        continue;
                    }
                    properties.insert(
                        parameter.name.clone(),
                        json!({
                            "type": normalize_type(&parameter.r#type),
                            "description": parameter.description,
                        }),
                    );
                    if parameter.required {
                        required.push(Value::String(parameter.name.clone()));
                    }
                }
                let mut schema = Map::from_iter([
                    ("type".into(), Value::String("object".into())),
                    ("properties".into(), Value::Object(properties)),
                ]);
                if !required.is_empty() {
                    schema.insert("required".into(), Value::Array(required));
                }
                Value::Object(schema)
            }),
            annotations: None,
        }
    }

    pub fn arguments(&self, arguments: &Value) -> Result<Vec<String>, String> {
        let values = arguments
            .as_object()
            .ok_or_else(|| "MCP 工具参数必须是 JSON 对象".to_string())?;
        for parameter in self
            .parameters
            .iter()
            .filter(|parameter| parameter.required)
        {
            if !values.contains_key(&parameter.name) {
                return Err(format!("缺少必填参数: {}", parameter.name));
            }
        }
        self.args
            .iter()
            .map(|argument| expand_template(argument, values))
            .collect()
    }
}

fn normalize_type(value: &str) -> &str {
    match value {
        "bool" => "boolean",
        "int" | "integer" | "float" | "double" => "number",
        "array" | "object" => value,
        _ => "string",
    }
}

fn expand_template(template: &str, values: &Map<String, Value>) -> Result<String, String> {
    let mut output = template.to_string();
    for (name, value) in values {
        let token = format!("{{{{{name}}}}}");
        if output.contains(&token) {
            let value = value
                .as_str()
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| value.to_string());
            if value.contains(['\n', '\r']) {
                return Err(format!("参数 {name} 不得包含换行符"));
            }
            output = output.replace(&token, &value);
        }
    }
    if output.contains("{{") || output.contains("}}") {
        return Err(format!("未解析的工具参数模板: {template}"));
    }
    Ok(output)
}

/// Load every `*.yaml` / `*.yml` recipe in the supplied directory.
pub fn load_dir(directory: impl AsRef<Path>) -> Result<HashMap<String, ToolRecipe>, String> {
    let directory = directory.as_ref();
    if !directory.exists() {
        return Ok(HashMap::new());
    }
    let mut recipes = HashMap::new();
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if !matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("yaml" | "yml")
        ) {
            continue;
        }
        let content = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let recipe: ToolRecipe = serde_yaml::from_str(&content)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if recipe.name.trim().is_empty() {
            return Err(format!("{}: 工具 name 不能为空", path.display()));
        }
        if recipes.insert(recipe.name.clone(), recipe).is_some() {
            return Err(format!("重复的工具配方: {}", path.display()));
        }
    }
    Ok(recipes)
}

pub fn configured_dir() -> String {
    std::env::var("PENTA_TOOLS_DIR").unwrap_or_else(|_| "tools".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_openai_compatible_schema_and_expands_arguments() {
        let recipe: ToolRecipe = serde_yaml::from_str(
            r#"
name: nmap
short_description: Scan one host
args: ["-sV", "{{target}}"]
parameters:
  - name: target
    type: string
    required: true
"#,
        )
        .unwrap();
        assert_eq!(recipe.definition().input_schema["required"][0], "target");
        assert_eq!(
            recipe.arguments(&json!({"target":"127.0.0.1"})).unwrap(),
            ["-sV", "127.0.0.1"]
        );
    }
}
