use serde::Deserialize;
use tokio::process::Command;

#[derive(Debug)]
pub struct EgressLease {
    pub network_name: String,
    pub policy_id: String,
    cleanup_command: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PreparedEgress {
    network_name: String,
    policy_id: String,
}

impl EgressLease {
    pub async fn prepare(run_id: &str, target: &str) -> Result<Self, String> {
        let Ok(command) = std::env::var("PENTA_EGRESS_POLICY_COMMAND") else {
            let network_name = std::env::var("PENTA_DOCKER_TARGET_NETWORK").map_err(|_| {
                "必须配置 PENTA_EGRESS_POLICY_COMMAND 或 PENTA_DOCKER_TARGET_NETWORK".to_string()
            })?;
            validate_network_name(&network_name)?;
            return Ok(Self {
                network_name,
                policy_id: "static-target-network".into(),
                cleanup_command: None,
            });
        };
        if !command.starts_with('/') {
            return Err("PENTA_EGRESS_POLICY_COMMAND 必须是绝对路径".into());
        }
        let output = Command::new(&command)
            .args(["prepare", run_id, target])
            .env_clear()
            .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
            .output()
            .await
            .map_err(|error| format!("出口策略控制器启动失败: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "出口策略控制器拒绝目标: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let prepared: PreparedEgress = serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("出口策略控制器响应无效: {error}"))?;
        validate_network_name(&prepared.network_name)?;
        if prepared.policy_id.trim().is_empty() {
            return Err("出口策略控制器未返回 policy_id".into());
        }
        Ok(Self {
            network_name: prepared.network_name,
            policy_id: prepared.policy_id,
            cleanup_command: Some(command),
        })
    }
}

impl Drop for EgressLease {
    fn drop(&mut self) {
        let Some(command) = self.cleanup_command.clone() else {
            return;
        };
        let policy_id = self.policy_id.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = Command::new(command)
                    .args(["release", &policy_id])
                    .env_clear()
                    .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
                    .output()
                    .await;
            });
        }
    }
}

fn validate_network_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 128
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_.-".contains(character))
    {
        return Err("出口策略控制器返回了非法 Docker 网络名".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_network_name;

    #[test]
    fn validates_docker_network_names() {
        assert!(validate_network_name("pent-target_run.1").is_ok());
        assert!(validate_network_name("network;rm -rf").is_err());
        assert!(validate_network_name("").is_err());
    }
}
