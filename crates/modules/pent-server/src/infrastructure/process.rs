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
        match std::env::var("PENTA_TOOL_EXECUTOR").as_deref() {
            Ok("docker") => Self::Docker,
            _ => Self::Host,
        }
    }
}

#[derive(Debug, Clone)]
pub enum NetworkPolicy {
    None,
    TargetOnly(String),
    Full,
}

impl Default for ContainerLimits {
    fn default() -> Self {
        Self {
            cpu_cores: 0.5,
            memory_mb: 256,
            timeout_seconds: 300,
            network_access: NetworkPolicy::TargetOnly("".into()),
            allowed_paths: vec![],
            backend: ExecutionBackend::Host,
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

    let (runner, args) = build_runner(&prepared, limits);

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
    })
}

fn build_runner(
    prepared: &super::tool_registry::PreparedTool,
    limits: &ContainerLimits,
) -> (&'static str, Vec<String>) {
    match limits.backend {
        ExecutionBackend::Host => {
            let memory_bytes = limits.memory_mb.saturating_mul(1024 * 1024);
            let cpu_seconds = limits.timeout_seconds.max(1);
            let mut args = vec![
                format!("--as={memory_bytes}"),
                format!("--cpu={cpu_seconds}"),
                "--".into(),
                prepared.program.into(),
            ];
            args.extend(prepared.args.clone());
            ("/usr/bin/prlimit", args)
        }
        ExecutionBackend::Docker => {
            let mut args = vec![
                "run".into(),
                "--rm".into(),
                "--read-only".into(),
                "--network".into(),
                "bridge".into(),
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
                prepared.container_image.into(),
            ];
            args.extend(prepared.args.clone());
            ("/usr/bin/docker", args)
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
            ..Default::default()
        };
        let (runner, args) = build_runner(&prepared, &limits);
        assert_eq!(runner, "/usr/bin/docker");
        assert!(args.windows(2).any(|pair| pair == ["--cap-drop", "ALL"]));
        assert!(args.contains(&"--read-only".to_string()));
        assert!(args.contains(&"no-new-privileges".to_string()));
    }
}
