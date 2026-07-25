//! Controlled process execution for security tools.
//!
//! Runs allow-listed external tools with bounded resources and output.

use std::{process::Stdio, time::Instant};

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

use super::tool_registry::prepare_tool;

const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

/// Resource limits for tool execution.
#[derive(Debug, Clone)]
pub struct ContainerLimits {
    pub cpu_cores: f64,
    pub memory_mb: u64,
    pub timeout_seconds: u64,
    pub network_access: NetworkPolicy,
    pub network_name: Option<String>,
    pub allowed_paths: Vec<String>,
    pub backend: ExecutionBackend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionBackend {
    Host,
    Docker,
}

impl ExecutionBackend {
    pub fn from_env() -> Self {
        if cfg!(test) && std::env::var("PENTA_TOOL_EXECUTOR").as_deref() == Ok("host") {
            Self::Host
        } else {
            Self::Docker
        }
    }
}

#[derive(Debug, Clone)]
pub enum NetworkPolicy {
    None,
    TargetOnly(String),
    Full,
}

impl NetworkPolicy {
    fn audit_name(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::TargetOnly(_) => "target_only",
            Self::Full => "full",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolExecutionAudit {
    pub backend: &'static str,
    pub container_image: Option<String>,
    pub image_digest_pinned: bool,
    pub network_policy: &'static str,
    pub network_name: Option<String>,
    pub cpu_cores: f64,
    pub memory_mb: u64,
    pub timeout_seconds: u64,
}

impl Default for ContainerLimits {
    fn default() -> Self {
        Self {
            cpu_cores: 0.5,
            memory_mb: 256,
            timeout_seconds: 300,
            network_access: NetworkPolicy::TargetOnly("".into()),
            network_name: None,
            allowed_paths: vec![],
            backend: ExecutionBackend::Docker,
        }
    }
}

/// Execute a tool command in a sandboxed environment.
pub async fn execute_tool(
    tool_name: &str,
    command: &[String],
    limits: &ContainerLimits,
) -> Result<ToolOutput, String> {
    let prepared = prepare_tool(tool_name, command, &limits.network_access)?;
    let started = Instant::now();
    tracing::info!(
        tool = tool_name,
        program = prepared.program,
        "executing allow-listed security tool"
    );

    let audit = execution_audit(&prepared, limits)?;
    let (runner, args) = build_runner(&prepared, limits)?;

    let mut child = Command::new(runner)
        .args(args)
        .env_clear()
        .env("LANG", "C.UTF-8")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("无法启动工具 {tool_name}: {error}"))?;

    let stdout = child.stdout.take().ok_or("无法捕获工具标准输出")?;
    let stderr = child.stderr.take().ok_or("无法捕获工具错误输出")?;
    let stdout_task = tokio::spawn(read_bounded(stdout));
    let stderr_task = tokio::spawn(read_bounded(stderr));
    let status = match tokio::time::timeout(
        std::time::Duration::from_secs(limits.timeout_seconds.max(1)),
        child.wait(),
    )
    .await
    {
        Ok(status) => status.map_err(|error| format!("等待工具退出失败: {error}"))?,
        Err(_) => {
            child
                .kill()
                .await
                .map_err(|error| format!("工具超时且终止失败: {error}"))?;
            return Err(format!("工具 {tool_name} 执行超时"));
        }
    };
    let (stdout, stdout_truncated) = stdout_task
        .await
        .map_err(|error| format!("读取标准输出失败: {error}"))??;
    let (mut stderr, stderr_truncated) = stderr_task
        .await
        .map_err(|error| format!("读取错误输出失败: {error}"))??;
    if stdout_truncated || stderr_truncated {
        stderr.push_str("\n[输出已按平台上限截断]");
    }

    Ok(ToolOutput {
        exit_code: status.code().unwrap_or(-1),
        stdout,
        stderr,
        duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        audit,
    })
}

fn execution_audit(
    prepared: &super::tool_registry::PreparedTool,
    limits: &ContainerLimits,
) -> Result<ToolExecutionAudit, String> {
    let (backend, image, network_name) = match limits.backend {
        ExecutionBackend::Host => ("host", None, None),
        ExecutionBackend::Docker => {
            validate_container_image(&prepared.container_image)?;
            let network = match &limits.network_access {
                NetworkPolicy::None => Some("none".to_string()),
                NetworkPolicy::TargetOnly(_) => Some(limits.network_name.clone().map_or_else(
                    || {
                        std::env::var("PENTA_DOCKER_TARGET_NETWORK").map_err(|_| {
                            "PENTA_DOCKER_TARGET_NETWORK 未配置，拒绝使用普通 bridge 网络"
                                .to_string()
                        })
                    },
                    Ok,
                )?),
                NetworkPolicy::Full => {
                    Some(std::env::var("PENTA_DOCKER_FULL_NETWORK").map_err(|_| {
                        "PENTA_DOCKER_FULL_NETWORK 未配置，拒绝开放容器网络".to_string()
                    })?)
                }
            };
            ("docker", Some(prepared.container_image.clone()), network)
        }
    };
    Ok(ToolExecutionAudit {
        backend,
        image_digest_pinned: image
            .as_deref()
            .is_some_and(|value| value.contains("@sha256:")),
        container_image: image,
        network_policy: limits.network_access.audit_name(),
        network_name,
        cpu_cores: limits.cpu_cores,
        memory_mb: limits.memory_mb,
        timeout_seconds: limits.timeout_seconds,
    })
}

fn validate_container_image(image: &str) -> Result<(), String> {
    if image.is_empty() {
        return Err("工具没有受支持的容器镜像".into());
    }
    if image.ends_with(":latest") {
        return Err("拒绝使用 latest 工具镜像".into());
    }
    let require_digest =
        !cfg!(test) && std::env::var("PENTA_REQUIRE_IMAGE_DIGEST").as_deref() != Ok("false");
    if require_digest && !image.contains("@sha256:") {
        return Err(format!(
            "工具镜像 {image} 未固定 digest；请配置 PENTA_TOOL_IMAGE_* 为 name@sha256:..."
        ));
    }
    Ok(())
}

fn build_runner(
    prepared: &super::tool_registry::PreparedTool,
    limits: &ContainerLimits,
) -> Result<(&'static str, Vec<String>), String> {
    match limits.backend {
        ExecutionBackend::Host => {
            if !cfg!(test) {
                return Err(
                    "生产环境禁止宿主机直连执行；安全工具必须使用 Docker 隔离执行器".into(),
                );
            }
            let memory_bytes = limits.memory_mb.saturating_mul(1024 * 1024);
            let cpu_seconds = limits.timeout_seconds.max(1);
            let mut args = vec![
                format!("--as={memory_bytes}"),
                format!("--cpu={cpu_seconds}"),
                "--".into(),
                prepared.program.into(),
            ];
            args.extend(prepared.args.clone());
            Ok(("/usr/bin/prlimit", args))
        }
        ExecutionBackend::Docker => {
            validate_container_image(&prepared.container_image)?;
            let network = match &limits.network_access {
                NetworkPolicy::None => "none".to_string(),
                NetworkPolicy::TargetOnly(_) => limits
                    .network_name
                    .clone()
                    .or_else(|| std::env::var("PENTA_DOCKER_TARGET_NETWORK").ok())
                    .ok_or_else(|| {
                        "PENTA_DOCKER_TARGET_NETWORK 未配置，拒绝使用普通 bridge 网络".to_string()
                    })?,
                NetworkPolicy::Full
                    if std::env::var("PENTA_ALLOW_FULL_NETWORK").as_deref() == Ok("true") =>
                {
                    std::env::var("PENTA_DOCKER_FULL_NETWORK").map_err(|_| {
                        "PENTA_DOCKER_FULL_NETWORK 未配置，拒绝开放容器网络".to_string()
                    })?
                }
                NetworkPolicy::Full => return Err("当前策略禁止容器访问完整网络".into()),
            };
            let mut args = vec![
                "run".into(),
                "--rm".into(),
                "--pull".into(),
                "never".into(),
                "--read-only".into(),
                "--init".into(),
                "--tmpfs".into(),
                "/tmp:rw,noexec,nosuid,size=64m".into(),
                "--network".into(),
                network,
                "--cap-drop".into(),
                "ALL".into(),
                "--security-opt".into(),
                "no-new-privileges".into(),
                "--pids-limit".into(),
                "64".into(),
                "--memory".into(),
                format!("{}m", limits.memory_mb),
                "--cpus".into(),
                limits.cpu_cores.to_string(),
            ];
            if let Some(state_path) = limits.allowed_paths.first() {
                #[cfg(unix)]
                use std::os::unix::fs::MetadataExt;
                let canonical = std::fs::canonicalize(state_path)
                    .map_err(|error| format!("HTTP 会话状态目录无效: {error}"))?;
                let allowed_root = std::env::var("PENTA_TOOL_STATE_ROOT")
                    .unwrap_or_else(|_| "/tmp/kairos-pent-tool-state".into());
                let allowed_root = std::fs::canonicalize(&allowed_root)
                    .map_err(|error| format!("工具状态根目录无效: {error}"))?;
                if !canonical.starts_with(&allowed_root) {
                    return Err("拒绝挂载工具状态根目录以外的路径".into());
                }
                args.extend([
                    "--user".into(),
                    {
                        #[cfg(unix)]
                        {
                            let metadata = std::fs::metadata(&canonical)
                                .map_err(|error| format!("读取工具状态目录失败: {error}"))?;
                            format!("{}:{}", metadata.uid(), metadata.gid())
                        }
                        #[cfg(not(unix))]
                        {
                            "65534:65534".into()
                        }
                    },
                    "--mount".into(),
                    format!("type=bind,src={},dst=/state", canonical.to_string_lossy()),
                ]);
            }
            args.extend([
                "--label".into(),
                format!(
                    "kairos.pent.network-policy={}",
                    limits.network_access.audit_name()
                ),
                prepared.container_image.clone(),
            ]);
            args.extend(prepared.args.clone());
            Ok(("/usr/bin/docker", args))
        }
    }
}

async fn read_bounded<R>(mut reader: R) -> Result<(String, bool), String>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut truncated = false;
    loop {
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        let remaining = MAX_OUTPUT_BYTES.saturating_sub(output.len());
        output.extend_from_slice(&buffer[..read.min(remaining)]);
        truncated |= read > remaining;
    }
    Ok((String::from_utf8_lossy(&output).into_owned(), truncated))
}

/// Output from a tool execution.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub audit: ToolExecutionAudit,
}

/// Multi-worker registration.
pub struct WorkerRegistry {
    worker_id: String,
    capacity: u32,
}

impl WorkerRegistry {
    pub fn new(worker_id: String, capacity: u32) -> Self {
        Self {
            worker_id,
            capacity,
        }
    }

    pub fn id(&self) -> &str {
        &self.worker_id
    }
    pub fn capacity(&self) -> u32 {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn executes_allowlisted_tool_against_authorized_loopback() {
        if !std::path::Path::new("/usr/bin/nmap").exists()
            || !std::path::Path::new("/usr/bin/prlimit").exists()
        {
            return;
        }
        let limits = ContainerLimits {
            memory_mb: 512,
            timeout_seconds: 15,
            network_access: NetworkPolicy::TargetOnly("127.0.0.1".into()),
            backend: ExecutionBackend::Host,
            ..Default::default()
        };
        let output = execute_tool("tcp_scan", &["scan".into(), "127.0.0.1".into()], &limits)
            .await
            .unwrap();
        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.contains("Nmap scan report"));
        assert!(output.duration_ms > 0);
    }

    #[test]
    fn docker_runner_has_hardening_flags() {
        let prepared = super::prepare_tool(
            "tcp_scan",
            &["scan".into(), "127.0.0.1".into()],
            &NetworkPolicy::TargetOnly("127.0.0.1".into()),
        )
        .unwrap();
        let limits = ContainerLimits {
            backend: ExecutionBackend::Docker,
            network_access: NetworkPolicy::None,
            ..Default::default()
        };
        let (runner, args) = build_runner(&prepared, &limits).unwrap();
        assert_eq!(runner, "/usr/bin/docker");
        assert!(args.windows(2).any(|pair| pair == ["--cap-drop", "ALL"]));
        assert!(args.contains(&"--read-only".to_string()));
        assert!(args.contains(&"no-new-privileges".to_string()));
    }
}
