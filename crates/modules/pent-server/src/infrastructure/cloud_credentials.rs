//! Tenant-bound cloud credential reference resolution for Pent workers.
//!
//! The model and Gateway only pass an opaque `_credential_ref`. The worker
//! derives the tenant from the persisted run/session and resolves encrypted
//! material locally immediately before sandbox execution.

use super::process::SecretEnvironment;
use super::secrets::decrypt_api_key;

pub async fn resolve_for_execution(
    pool: &kairos_platform_framework_database::PgPool,
    credential_ref: &str,
    tool_name: &str,
    expected_provider: Option<&str>,
    run_id: Option<&str>,
    session_id: &str,
) -> Result<SecretEnvironment, String> {
    validate_reference(credential_ref)?;
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT c.provider,c.secret_payload
         FROM pent_cloud_credentials c
         WHERE c.id=$1
           AND c.tenant_id=COALESCE(
               (SELECT tenant_id FROM pent_runs
                WHERE id=$2 AND (session_id IS NULL OR session_id=$3)),
               (SELECT tenant_id FROM pent_agent_sessions
                WHERE id=$3 AND ($2::text IS NULL OR active_run_id=$2))
           )
           AND c.status='active'
           AND (c.expires_at IS NULL OR c.expires_at>now())
           AND c.allowed_tools ? $4",
    )
    .bind(credential_ref)
    .bind(run_id)
    .bind(session_id)
    .bind(tool_name)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("无法解析云凭据引用: {error}"))?;
    let (provider, encrypted_payload) = row.ok_or_else(|| {
        "云凭据引用不存在、已失效、租户不匹配或未授权给当前工具".to_string()
    })?;
    if expected_provider.is_some_and(|expected| expected != provider) {
        return Err("云凭据提供商与工具参数不匹配".to_string());
    }
    if !encrypted_payload.starts_with("env:") {
        return Err("云凭据载荷不是平台密文".to_string());
    }
    let payload = decrypt_api_key(&encrypted_payload)
        .map_err(|_| "云凭据解密失败".to_string())?;
    payload_to_environment(&provider, &payload)
}

fn validate_reference(reference: &str) -> Result<(), String> {
    let valid = !reference.is_empty()
        && reference.len() <= 50
        && reference
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    valid
        .then_some(())
        .ok_or_else(|| "credential_ref 格式无效".to_string())
}

fn payload_to_environment(provider: &str, payload: &str) -> Result<SecretEnvironment, String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|_| "云凭据载荷格式无效".to_string())?;
    let object = value
        .as_object()
        .ok_or_else(|| "云凭据载荷必须是 JSON 对象".to_string())?;
    let mappings: &[(&str, &str, bool)] = match provider {
        "aws" => &[
            ("access_key_id", "AWS_ACCESS_KEY_ID", true),
            ("secret_access_key", "AWS_SECRET_ACCESS_KEY", true),
            ("session_token", "AWS_SESSION_TOKEN", false),
            ("region", "AWS_DEFAULT_REGION", false),
        ],
        "azure" => &[
            ("client_id", "AZURE_CLIENT_ID", true),
            ("client_secret", "AZURE_CLIENT_SECRET", true),
            ("tenant_id", "AZURE_TENANT_ID", true),
        ],
        "aliyun" => &[
            ("access_key_id", "ALIBABA_CLOUD_ACCESS_KEY_ID", true),
            ("access_key_secret", "ALIBABA_CLOUD_ACCESS_KEY_SECRET", true),
        ],
        "tencent" => &[
            ("secret_id", "TENCENTCLOUD_SECRET_ID", true),
            ("secret_key", "TENCENTCLOUD_SECRET_KEY", true),
            ("session_token", "TENCENTCLOUD_SESSION_TOKEN", false),
            ("region", "TENCENTCLOUD_REGION", false),
        ],
        // GCP uses a worker-created, read-only credential file rather than raw
        // JSON in an environment value. File materialization is a later step.
        "gcp" => return Err("GCP 凭据需要只读文件注入，当前尚未启用".to_string()),
        _ => return Err("不支持的云凭据提供商".to_string()),
    };
    let mut environment = Vec::new();
    for (field, variable, required) in mappings {
        match object.get(*field).and_then(serde_json::Value::as_str) {
            Some(value) if !value.is_empty() => {
                environment.push(((*variable).to_string(), value.to_string()));
            }
            _ if *required => return Err(format!("云凭据缺少字段: {field}")),
            _ => {}
        }
    }
    Ok(SecretEnvironment::new(environment))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aws_payload_maps_without_exposing_source_field_names() {
        let environment = payload_to_environment(
            "aws",
            r#"{"access_key_id":"AKIAEXAMPLE","secret_access_key":"secret","region":"us-east-1"}"#,
        )
        .unwrap();
        let names = environment.iter().map(|(name, _)| name).collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "AWS_ACCESS_KEY_ID",
                "AWS_SECRET_ACCESS_KEY",
                "AWS_DEFAULT_REGION"
            ]
        );
        assert!(!format!("{environment:?}").contains("secret"));
    }

    #[test]
    fn rejects_missing_required_fields_and_unsafe_references() {
        assert!(payload_to_environment("aws", r#"{"access_key_id":"id"}"#).is_err());
        assert!(validate_reference("tenant/escape").is_err());
        assert!(validate_reference("cred-123_valid").is_ok());
    }
}
