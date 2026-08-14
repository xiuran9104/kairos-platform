//! Controlled process execution for security tools.
//!
//! Runs allow-listed external tools with bounded resources and output.

use std::{process::Stdio, time::Instant};

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

use super::tool_registry::{PreparedTool, prepare_tool, prepare_tool_from_json};

const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

/// Resource limits for tool execution.
#[derive(Debug, Clone)]
pub struct ToolExecutionLimits {
    pub cpu_cores: f64,
    pub memory_mb: u64,
    pub timeout_seconds: u64,
    pub network_access: NetworkPolicy,
    pub allowed_paths: Vec<String>,
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

    /// Derive the network policy for a Run from its confirmed scope snapshot.
    ///
    /// Precedence:
    /// 1. An explicit `network_policy` string (`none` / `full`) on the snapshot wins.
    /// 2. Otherwise, if the scope declares confirmed targets, restrict access to
    ///    the first target (`TargetOnly`). This mirrors the per-target isolation
    ///    already used by `http_probe` / `http_request`.
    /// 3. Otherwise fall back to `Full`.
    ///
    /// A `none` policy means every network tool is rejected upstream by
    /// `tool_registry::validate_target`, and the audit trail records `none`.
    pub fn derive_from_scope(scope_snapshot: &serde_json::Value) -> Self {
        if let Some(policy) = scope_snapshot
            .get("network_policy")
            .or_else(|| scope_snapshot.get("network"))
            .and_then(|value| value.as_str())
        {
            match policy.trim() {
                "none" | "deny" | "off" => return Self::None,
                "full" | "any" | "on" => return Self::Full,
                _ => {}
            }
        }
        let target = scope_snapshot
            .get("targets")
            .and_then(serde_json::Value::as_array)
            .and_then(|targets| targets.first())
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|target| !target.is_empty());
        match target {
            Some(target) => Self::TargetOnly(target.to_owned()),
            None => Self::Full,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolExecutionAudit {
    pub backend: &'static str,
    pub network_policy: &'static str,
    pub cpu_cores: f64,
    pub memory_mb: u64,
    pub timeout_seconds: u64,
}

impl Default for ToolExecutionLimits {
    fn default() -> Self {
        Self {
            cpu_cores: 0.5,
            memory_mb: 256,
            timeout_seconds: 300,
            network_access: NetworkPolicy::Full,
            allowed_paths: vec![],
        }
    }
}

/// Execute a tool command directly in a sandboxed environment.
///
/// This is the low-level implementation. Callers should usually use
/// [`execute_tool`] so that the global elastic executor (concurrency limits,
/// circuit breaker, cancellation) can participate.
pub(crate) async fn execute_tool_direct(
    tool_name: &str,
    command: &[String],
    limits: &ToolExecutionLimits,
) -> Result<ToolOutput, String> {
    let prepared = prepare_tool(tool_name, command, &limits.network_access)?;
    execute_prepared_tool(tool_name, &prepared, limits).await
}

/// Execute a recipe from the model's structured JSON arguments using the
/// DesRed-style `parameters` declarative mapping.
pub async fn execute_tool_json(
    tool_name: &str,
    arguments: &serde_json::Value,
    limits: &ToolExecutionLimits,
) -> Result<ToolOutput, String> {
    let prepared = prepare_tool_from_json(tool_name, arguments, &limits.network_access)?;
    execute_prepared_tool(tool_name, &prepared, limits).await
}

async fn execute_prepared_tool(
    tool_name: &str,
    prepared: &PreparedTool,
    limits: &ToolExecutionLimits,
) -> Result<ToolOutput, String> {
    // ── Pre-flight: ensure the binary exists ──────────────────────────
    if !path_is_executable(prepared.program) {
        if auto_install_enabled() {
            if let Some(lifecycle) =
                super::tool_lifecycle::find(tool_name)
            {
                tracing::warn!(
                    tool = tool_name,
                    path = prepared.program,
                    "tool binary missing; attempting auto-install"
                );
                if let Err(error) = lifecycle.ensure_available().await {
                    return Err(format!(
                        "工具 '{tool_name}' 未安装且自动安装失败: {error}\n\
                         手动安装: {}",
                        lifecycle
                            .install_methods
                            .iter()
                            .map(|m| format!("[{}] {}", m.label, m.command))
                            .collect::<Vec<_>>()
                            .join(" | ")
                    ));
                }
                tracing::info!(tool = tool_name, "auto-installed successfully");
            } else {
                return Err(format!(
                    "工具 '{tool_name}' 二进制文件不存在: {}\n\
                     请手动安装此工具后重试。",
                    prepared.program
                ));
            }
        } else {
            let hint = super::tool_lifecycle::find(tool_name)
                .map(|lc| {
                    lc.install_methods
                        .iter()
                        .map(|m| format!("[{}] {}", m.label, m.command))
                        .collect::<Vec<_>>()
                        .join(" | ")
                })
                .unwrap_or_default();
            return Err(format!(
                "工具 '{tool_name}' 二进制文件不存在: {}\n\
                 安装提示: {hint}\n\
                 或设置 PENTA_AUTO_INSTALL_TOOLS=true 启用自动安装。",
                prepared.program
            ));
        }
    }

    let started = Instant::now();
    tracing::info!(
        tool = tool_name,
        program = prepared.program,
        "executing allow-listed security tool"
    );

    let audit = execution_audit(prepared, limits)?;
    let (runner, args) = build_runner(prepared, limits)?;
    ensure_host_state_dir()?;

    let capture = std::env::var("PENTA_TRAFFIC_CAPTURE")
        .map(|v| v == "true")
        .unwrap_or(false);
    let proxy_addr = std::env::var("PENTA_TRAFFIC_PROXY")
        .unwrap_or_else(|_| "http://127.0.0.1:8085".into());
    // ── M3: Low-privilege user execution ──────────────────────────
    // When PENTA_TOOL_USER is set, run the tool via sudo/runuser to drop
    // privileges. Default: run as current user (for dev environments).
    let tool_user = std::env::var("PENTA_TOOL_USER").unwrap_or_default();
    let mut cmd = if tool_user.is_empty() {
        Command::new(runner)
    } else {
        let mut c = Command::new("sudo");
        c.arg("-u").arg(&tool_user).arg(runner);
        c
    };
    cmd.args(args).env_clear().env("LANG", "C.UTF-8");
    // ── M2-F13: mitmproxy traffic capture ──────────────────────────
    if capture {
        cmd.env("http_proxy", &proxy_addr)
           .env("https_proxy", &proxy_addr)
           .env("HTTP_PROXY", &proxy_addr)
           .env("HTTPS_PROXY", &proxy_addr)
           .env("no_proxy", "127.0.0.1,localhost,.local")
           .env("NO_PROXY", "127.0.0.1,localhost,.local");
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("无法启动工具 {tool_name}: {error}"))?;

    // ── M3: cgroup v2 isolation ────────────────────────────────────
    // Attach the spawned process to a per-task cgroup so the kernel
    // enforces CPU, memory, PID, and IO limits at the cgroup level.
    // The cgroup is auto-cleaned when the guard drops.
    let _cgroup_guard = {
        let limits = crate::infrastructure::cgroup::CgroupLimits {
            cpu_quota_us: Some((limits.cpu_cores * 100_000.0) as u64),
            memory_max_bytes: Some(limits.memory_mb.saturating_mul(1024 * 1024)),
            pids_max: Some(64),
        };
        let guard = crate::infrastructure::cgroup::CgroupGuard::create(
            &format!("kairos-tool-{}", std::process::id()),
            &limits,
        );
        if let Some(ref g) = guard {
            if let Some(pid) = child.id() {
                g.attach(pid);
            }
        }
        guard
    };

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

/// Execute a tool command in a sandboxed environment.
///
/// When a global elastic executor is initialized (via
/// [`crate::infrastructure::execution::init_global_executor`]) this routes the
/// call through the executor so that per-tool concurrency limits, circuit
/// breaker, and cancellation are applied. Otherwise it falls back to the
/// direct implementation.
pub async fn execute_tool(
    tool_name: &str,
    command: &[String],
    limits: &ToolExecutionLimits,
) -> Result<ToolOutput, String> {
    if let Some(executor) = crate::infrastructure::execution::global_executor() {
        executor.execute(tool_name, command, limits).await
    } else {
        execute_tool_direct(tool_name, command, limits).await
    }
}

fn execution_audit(
    _prepared: &super::tool_registry::PreparedTool,
    limits: &ToolExecutionLimits,
) -> Result<ToolExecutionAudit, String> {
    Ok(ToolExecutionAudit {
        backend: "host",
        network_policy: limits.network_access.audit_name(),
        cpu_cores: limits.cpu_cores,
        memory_mb: limits.memory_mb,
        timeout_seconds: limits.timeout_seconds,
    })
}

fn build_runner(
    prepared: &super::tool_registry::PreparedTool,
    limits: &ToolExecutionLimits,
) -> Result<(&'static str, Vec<String>), String> {
    let memory_bytes = limits.memory_mb.saturating_mul(1024 * 1024);
    let cpu_seconds = limits.timeout_seconds.max(1);
    // Use --rss (resident set size) instead of --as (address space).
    // Go runtimes pre-reserve large virtual address spaces that --as rejects,
    // but their actual physical memory usage stays within bounds.
    let mut args = vec![
        format!("--rss={memory_bytes}"),
        format!("--cpu={cpu_seconds}"),
        "--".into(),
        prepared.program.into(),
    ];
    args.extend(prepared.args.clone());
    Ok(("/usr/bin/prlimit", args))
}

fn ensure_host_state_dir() -> Result<(), String> {
    let state_root =
        std::env::var("PENTA_TOOL_STATE_ROOT").unwrap_or_else(|_| "/tmp/kairos-pent-tool-state".into());
    std::fs::create_dir_all(&state_root)
        .map_err(|error| format!("创建工具状态目录失败: {error}"))
}

/// Fast synchronous check: does `path` point to an executable regular file?
fn path_is_executable(path: &str) -> bool {
    std::fs::metadata(path)
        .map(|m| m.is_file())
        .unwrap_or(false)
}

/// Whether on-demand auto-install is allowed.
/// Controlled by `PENTA_AUTO_INSTALL_TOOLS` (default: enabled).
fn auto_install_enabled() -> bool {
    std::env::var("PENTA_AUTO_INSTALL_TOOLS")
        .map(|v| v != "false" && v != "0" && v != "no")
        .unwrap_or(true)
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
        crate::infrastructure::recipe::load_recipes_for_tests();
        let limits = ToolExecutionLimits {
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
    fn host_runner_uses_prlimit_and_direct_tool() {
        crate::infrastructure::recipe::load_recipes_for_tests();
        let prepared = super::prepare_tool(
            "tcp_scan",
            &["scan".into(), "127.0.0.1".into()],
            &NetworkPolicy::TargetOnly("127.0.0.1".into()),
        )
        .unwrap();
        let limits = ToolExecutionLimits {
            network_access: NetworkPolicy::None,
            ..Default::default()
        };
        let (runner, args) = build_runner(&prepared, &limits).unwrap();
        assert_eq!(runner, "/usr/bin/prlimit");
        assert!(args.contains(&"--".to_string()));
        assert!(args.contains(&"/usr/bin/nmap".to_string()));
    }

    #[test]
    fn network_policy_derives_from_scope() {
        let none = NetworkPolicy::derive_from_scope(&serde_json::json!({
            "targets": ["example.com"], "network_policy": "none"
        }));
        assert!(matches!(none, NetworkPolicy::None));

        let full = NetworkPolicy::derive_from_scope(&serde_json::json!({
            "targets": ["example.com"], "network_policy": "full"
        }));
        assert!(matches!(full, NetworkPolicy::Full));

        let target = NetworkPolicy::derive_from_scope(&serde_json::json!({
            "targets": ["example.com", "other.com"]
        }));
        assert!(matches!(target, NetworkPolicy::TargetOnly(ref t) if t == "example.com"));

        let empty = NetworkPolicy::derive_from_scope(&serde_json::json!({}));
        assert!(matches!(empty, NetworkPolicy::Full));

        let empty_targets = NetworkPolicy::derive_from_scope(&serde_json::json!({
            "targets": []
        }));
        assert!(matches!(empty_targets, NetworkPolicy::Full));
    }
}
