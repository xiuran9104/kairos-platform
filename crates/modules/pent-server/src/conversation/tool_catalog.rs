use kairos_platform_framework_database::PgPool;
use serde_json::{Value, json};

use super::tool_executor::has_conversation_handler;

/// Load tool schemas from the maintained registry instead of duplicating them
/// in the conversation runtime.
pub async fn load_conversation_tool_definitions(pool: &PgPool) -> anyhow::Result<Vec<Value>> {
    let rows = sqlx::query_as::<_, (String, String, Value)>(
        "SELECT tool_id, description, input_schema
         FROM pent_tool_definitions
         WHERE lifecycle_status = 'active'
         ORDER BY namespace, name",
    )
    .fetch_all(pool)
    .await?;

    let mut definitions = rows
        .into_iter()
        .filter(|(name, _, _)| has_conversation_handler(name))
        .map(|(name, description, parameters)| {
            json!({
                "type": "function",
                "function": {
                    "name": name,
                    "description": description,
                    "parameters": parameters
                }
            })
        })
        .collect::<Vec<_>>();

    // This guarded validator is implemented by the conversation runtime but is
    // not yet part of older database snapshots.
    if !definitions.iter().any(|definition| {
        definition.pointer("/function/name").and_then(Value::as_str) == Some("file_upload_validate")
    }) {
        definitions.push(file_upload_validator_definition());
    }

    Ok(definitions)
}

fn file_upload_validator_definition() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "file_upload_validate",
            "description": "在当前会话已确认的授权范围内，使用无害标记文件验证上传端点是否接受并存储文件。禁止 webshell、命令执行和持久化载荷。",
            "parameters": {
                "type": "object",
                "required": ["url"],
                "properties": {
                    "url": {"type": "string", "description": "上传端点完整 URL"},
                    "field_name": {"type": "string", "description": "文件字段名，默认 imgFile"},
                    "filename": {"type": "string", "description": "标记文件名，仅允许 txt/png/svg"},
                    "content_type": {"type": "string", "description": "标记文件 MIME 类型"},
                    "verify_url": {"type": "string", "description": "可选的上传文件访问 URL"}
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::file_upload_validator_definition;
    use crate::conversation::tool_executor::has_conversation_handler;

    #[test]
    fn guarded_upload_schema_uses_session_authorization() {
        let definition = file_upload_validator_definition();
        let required = definition["function"]["parameters"]["required"]
            .as_array()
            .unwrap();
        assert_eq!(required, &vec![serde_json::json!("url")]);
        assert!(
            definition["function"]["parameters"]["properties"]
                .get("confirm_authorized")
                .is_none()
        );
    }

    #[test]
    fn model_catalog_only_advertises_tools_with_real_handlers() {
        assert!(has_conversation_handler("http_request"));
        assert!(has_conversation_handler("js_analyze"));
        assert!(!has_conversation_handler("sqlmap"));
    }
}
