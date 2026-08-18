//! Controlled process execution for security tools.
//!
//! Runs allow-listed external tools with bounded resources and output.

use std::{
    fs::{File, OpenOptions},
    io::{Seek, Write},
    path::{Path, PathBuf},
    process::Stdio,
    time::Instant,
};

#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

pub use kairos_platform_pent_tooling::backend::{
    ExecutionAudit as ToolExecutionAudit, ExecutionOutput as ToolOutput, SecretEnvironment,
};

use super::tool_registry::{PreparedTool, prepare_tool, prepare_tool_from_json};

const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
const DEFAULT_SANDBOX_ROOT: &str = "/tmp/kairos-pent-sandbox";
const DEFAULT_BWRAP_PATH: &str = "/usr/bin/bwrap";

/// Resource limits for tool execution.
#[derive(Debug, Clone)]
pub struct ToolExecutionLimits {
    pub cpu_cores: f64,
    pub memory_mb: u64,
    pub timeout_seconds: u64,
    pub network_access: NetworkPolicy,
    pub workspace_dir: Option<String>,
    pub artifact_dir: Option<String>,
    pub allowed_paths: Vec<String>,
    /// Run id used to tag captured traffic. Injected as `x-kairos-run-id` for
    /// curl-based tools so the MITM proxy can associate exchanges with a run.
    pub run_id: Option<String>,
    /// Ephemeral cloud credentials resolved by the worker from an opaque
    /// credential reference. Values are redacted and never serialized.
    pub secret_environment: SecretEnvironment,
}

/// A long-lived interactive process hosted inside the same bubblewrap and
/// cgroup boundary used by one-shot security tools.
pub struct InteractiveSandbox {
    pub child: tokio::process::Child,
    _cgroup_guard: Option<crate::infrastructure::cgroup::CgroupGuard>,
}

pub async fn spawn_interactive_shell(
    session_id: &str,
    timeout_seconds: u64,
) -> Result<InteractiveSandbox, String> {
    let limits = ToolExecutionLimits {
        cpu_cores: 1.0,
        memory_mb: 512,
        timeout_seconds: timeout_seconds.clamp(60, 3_600),
        network_access: NetworkPolicy::None,
        run_id: Some(session_id.to_owned()),
        ..Default::default()
    };
    let prepared = PreparedTool {
        program: "/bin/bash",
        args: vec!["--noprofile".into(), "--norc".into()],
    };
    let sandbox = build_bubblewrap_command(&prepared, prepared.program, &limits, &[], &[])?;
    let mut command = Command::new(&sandbox.program);
    command
        .args(&sandbox.args)
        .env_clear()
        .env("LANG", "C.UTF-8")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(target_os = "linux")]
    {
        let seccomp_fd = sandbox.seccomp_file.as_raw_fd();
        unsafe {
            command.pre_exec(move || {
                let flags = libc::fcntl(seccomp_fd, libc::F_GETFD);
                if flags < 0
                    || libc::fcntl(seccomp_fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let child_result = command.spawn();
    drop(sandbox.seccomp_file);
    let _ = std::fs::remove_file(&sandbox.seccomp_path);
    let child = child_result.map_err(|error| format!("无法启动受治理终端: {error}"))?;
    let cgroup_limits = crate::infrastructure::cgroup::CgroupLimits {
        cpu_quota_us: Some(100_000),
        memory_max_bytes: Some(512 * 1024 * 1024),
        pids_max: Some(64),
    };
    let cgroup = crate::infrastructure::cgroup::CgroupGuard::create(
        &format!("kairos-terminal-{session_id}"),
        &cgroup_limits,
    );
    if let (Some(guard), Some(pid)) = (&cgroup, child.id()) {
        guard.attach(pid);
    }
    Ok(InteractiveSandbox {
        child,
        _cgroup_guard: cgroup,
    })
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

impl Default for ToolExecutionLimits {
    fn default() -> Self {
        Self {
            cpu_cores: 0.5,
            memory_mb: 256,
            timeout_seconds: 300,
            network_access: NetworkPolicy::Full,
            workspace_dir: None,
            artifact_dir: None,
            allowed_paths: vec![],
            run_id: None,
            secret_environment: SecretEnvironment::default(),
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
    let mut prepared = prepare_tool(tool_name, command, &limits.network_access)?;
    inject_run_id_header(&mut prepared, limits.run_id.as_deref());
    execute_prepared_tool(tool_name, &prepared, limits).await
}

/// Fail fast when the mandatory local sandbox launcher is not installed.
pub fn verify_bubblewrap() -> Result<(), String> {
    #[cfg(not(target_os = "linux"))]
    return Err("bubblewrap sandbox 仅支持 Linux".into());

    #[cfg(target_os = "linux")]
    {
        let path =
            std::env::var("PENTA_BWRAP_PATH").unwrap_or_else(|_| DEFAULT_BWRAP_PATH.to_string());
        if !Path::new(&path).is_absolute() || !path_is_executable(&path) {
            return Err(format!(
                "bubblewrap 不可用: {path}；请在 pent-agent 主机安装 bubblewrap"
            ));
        }
        let status = std::process::Command::new(&path)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| format!("无法检查 bubblewrap: {error}"))?;
        if !status.success() {
            return Err(format!("bubblewrap 自检失败: {path}"));
        }
        Ok(())
    }
}

/// Tag curl-based tools with the run id so the MITM proxy can associate
/// captured traffic with the run.
pub(crate) fn inject_run_id_header(
    prepared: &mut super::tool_registry::PreparedTool,
    run_id: Option<&str>,
) {
    if let Some(run_id) = run_id
        && prepared.program.contains("curl")
    {
        prepared
            .args
            .extend(["--header".into(), format!("x-kairos-run-id: {run_id}")]);
    }
}

/// Execute a recipe from the model's structured JSON arguments using the
/// DesRed-style `parameters` declarative mapping.
pub(crate) async fn execute_tool_json_direct(
    tool_name: &str,
    arguments: &serde_json::Value,
    limits: &ToolExecutionLimits,
) -> Result<ToolOutput, String> {
    let mut prepared = prepare_tool_from_json(tool_name, arguments, &limits.network_access)?;
    inject_run_id_header(&mut prepared, limits.run_id.as_deref());
    execute_prepared_tool(tool_name, &prepared, limits).await
}

/// Execute a structured recipe call through the global elastic executor when
/// available, matching the scheduling path used by Run-mode tools.
pub async fn execute_tool_json(
    tool_name: &str,
    arguments: &serde_json::Value,
    limits: &ToolExecutionLimits,
) -> Result<ToolOutput, String> {
    let configured_limits = limits_with_recipe_profile(tool_name, limits);
    if let Some(executor) = crate::infrastructure::execution::global_executor() {
        executor
            .execute_json(tool_name, arguments, &configured_limits)
            .await
    } else {
        execute_tool_json_direct(tool_name, arguments, &configured_limits).await
    }
}

async fn execute_prepared_tool(
    tool_name: &str,
    prepared: &PreparedTool,
    limits: &ToolExecutionLimits,
) -> Result<ToolOutput, String> {
    let mut resolved_program = resolve_tool_program(tool_name, prepared.program);
    // ── Pre-flight: ensure the binary exists ──────────────────────────
    if !path_is_executable(&resolved_program) {
        if auto_install_enabled() {
            if let Some(lifecycle) = super::tool_lifecycle::find(tool_name) {
                tracing::warn!(
                    tool = tool_name,
                    path = resolved_program,
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
                    resolved_program
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
                resolved_program
            ));
        }
    }
    resolved_program = resolve_tool_program(tool_name, &resolved_program);
    if !path_is_executable(&resolved_program) {
        return Err(format!(
            "工具 '{tool_name}' 安装后仍无法解析可执行文件: {}",
            resolved_program
        ));
    }

    let started = Instant::now();
    tracing::info!(
        tool = tool_name,
        program = resolved_program,
        "executing allow-listed security tool"
    );

    let capture = super::mitm_sink::traffic_capture_enabled();
    let proxy_addr =
        std::env::var("PENTA_TRAFFIC_PROXY").unwrap_or_else(|_| "http://127.0.0.1:8085".into());
    let mut sandbox_env = vec![("LANG".to_string(), "C.UTF-8".to_string())];
    for (name, value) in limits.secret_environment.iter() {
        validate_secret_environment(name, value)?;
        sandbox_env.push((name.to_string(), value.to_string()));
    }
    let mut readonly_paths = Vec::new();
    let mut forced_proxy = None;
    // Curl-backed tools already emit a structured exchange that is persisted
    // by the conversation traffic sink. Remaining Web tools receive both
    // conventional proxy variables and transparent connect interception so a
    // client cannot silently bypass capture by ignoring HTTP_PROXY.
    if capture && !resolved_program.contains("curl") {
        let attributed_proxy = proxy_url_for_run(&proxy_addr, limits.run_id.as_deref());
        for name in ["http_proxy", "https_proxy", "HTTP_PROXY", "HTTPS_PROXY"] {
            sandbox_env.push((name.to_string(), attributed_proxy.clone()));
        }
        for name in ["no_proxy", "NO_PROXY"] {
            // Authorized localhost/private-range targets are common in local
            // labs and must pass through the capture proxy as well.
            sandbox_env.push((name.to_string(), String::new()));
        }
        // Inject the platform CA so tools trust the proxy's impersonated certs.
        let ca_path = kairos_platform_mitm_proxy::ca::ca_cert_path(
            &kairos_platform_mitm_proxy::ca::default_ca_dir(),
        );
        if ca_path.exists() {
            let ca_path = canonical_file(&ca_path)?;
            let ca_value = "/etc/kairos-mitm-ca.pem".to_string();
            readonly_paths.push((ca_path, ca_value.clone()));
            for name in [
                "SSL_CERT_FILE",
                "CURL_CA_BUNDLE",
                "REQUESTS_CA_BUNDLE",
                "NODE_EXTRA_CA_CERTS",
            ] {
                sandbox_env.push((name.to_string(), ca_value.clone()));
            }
        }
        forced_proxy = super::traffic_capture::prepare(
            tool_name,
            &resolved_program,
            &prepared.args,
            limits.run_id.as_deref(),
            &proxy_addr,
        )?;
        if let Some(injection) = forced_proxy.as_ref() {
            sandbox_env.extend(injection.environment());
            readonly_paths.extend(injection.readonly_paths());
        }
    }
    let sandbox = build_bubblewrap_command(
        prepared,
        &resolved_program,
        limits,
        &sandbox_env,
        &readonly_paths,
    )?;
    let audit = execution_audit(limits, &sandbox.layout);
    let mut cmd = Command::new(&sandbox.program);
    cmd.args(&sandbox.args).env_clear().env("LANG", "C.UTF-8");
    #[cfg(target_os = "linux")]
    {
        let seccomp_fd = sandbox.seccomp_file.as_raw_fd();
        unsafe {
            cmd.pre_exec(move || {
                let flags = libc::fcntl(seccomp_fd, libc::F_GETFD);
                if flags < 0
                    || libc::fcntl(seccomp_fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let child_result = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn();
    drop(sandbox.seccomp_file);
    let _ = std::fs::remove_file(&sandbox.seccomp_path);
    let mut child = child_result.map_err(|error| format!("无法启动工具 {tool_name}: {error}"))?;

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
    drop(forced_proxy);
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

fn proxy_url_for_run(proxy_addr: &str, run_id: Option<&str>) -> String {
    let Some(run_id) = run_id.filter(|value| {
        !value.is_empty()
            && value.len() <= 96
            && value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    }) else {
        return proxy_addr.to_string();
    };
    for scheme in ["http://", "https://"] {
        if let Some(authority) = proxy_addr.strip_prefix(scheme) {
            return format!("{scheme}{run_id}:@{authority}");
        }
    }
    proxy_addr.to_string()
}

const CLOUD_SECRET_ENV_ALLOWLIST: &[&str] = &[
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "AWS_DEFAULT_REGION",
    "AWS_REGION",
    "AZURE_CLIENT_ID",
    "AZURE_CLIENT_SECRET",
    "AZURE_TENANT_ID",
    "GOOGLE_APPLICATION_CREDENTIALS",
    "CLOUDSDK_CORE_PROJECT",
    "ALIBABA_CLOUD_ACCESS_KEY_ID",
    "ALIBABA_CLOUD_ACCESS_KEY_SECRET",
    "TENCENTCLOUD_SECRET_ID",
    "TENCENTCLOUD_SECRET_KEY",
    "TENCENTCLOUD_SESSION_TOKEN",
    "TENCENTCLOUD_REGION",
];

fn validate_secret_environment(name: &str, value: &str) -> Result<(), String> {
    if !CLOUD_SECRET_ENV_ALLOWLIST.contains(&name) {
        return Err(format!("不允许向工具沙箱注入环境变量: {name}"));
    }
    if value.is_empty() || value.contains(['\0', '\r', '\n']) {
        return Err(format!("工具沙箱环境变量 {name} 的值无效"));
    }
    Ok(())
}

fn resolve_tool_program(tool_name: &str, configured_program: &str) -> String {
    if path_is_executable(configured_program) {
        return configured_program.to_owned();
    }
    if let Some(path) = super::tool_lifecycle::find(tool_name)
        .and_then(|lifecycle| lifecycle.resolved_binary_path())
    {
        return path.to_string_lossy().into_owned();
    }
    let mut names = vec![tool_name.to_string()];
    if let Some(file) = Path::new(configured_program)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
    {
        names.push(file);
    }
    super::tool_lifecycle::find_executable(&names)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| configured_program.to_owned())
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
    let configured_limits = limits_with_recipe_profile(tool_name, limits);
    if let Some(executor) = crate::infrastructure::execution::global_executor() {
        executor
            .execute(tool_name, command, &configured_limits)
            .await
    } else {
        execute_tool_direct(tool_name, command, &configured_limits).await
    }
}

fn limits_with_recipe_profile(
    tool_name: &str,
    limits: &ToolExecutionLimits,
) -> ToolExecutionLimits {
    let mut configured = limits.clone();
    if let Some(recipe) = super::recipe::lookup(tool_name) {
        configured.cpu_cores = recipe.resource_profile.cpu_cores.clamp(0.1, 16.0);
        configured.memory_mb = recipe.resource_profile.memory_mb.clamp(32, 32_768);
        configured.timeout_seconds = recipe.resource_profile.timeout_seconds.clamp(1, 3_600);
    }
    configured
}

fn execution_audit(limits: &ToolExecutionLimits, layout: &SandboxLayout) -> ToolExecutionAudit {
    ToolExecutionAudit {
        backend: "bubblewrap".into(),
        network_policy: limits.network_access.audit_name().into(),
        cpu_cores: limits.cpu_cores,
        memory_mb: limits.memory_mb,
        timeout_seconds: limits.timeout_seconds,
        workspace_dir: Some(layout.workspace.to_string_lossy().into_owned()),
        artifact_dir: Some(layout.artifacts.to_string_lossy().into_owned()),
        mount_namespace: true,
        pid_namespace: true,
        user_namespace: true,
        capabilities_dropped: true,
        seccomp: true,
    }
}

#[derive(Debug)]
struct SandboxLayout {
    workspace: PathBuf,
    artifacts: PathBuf,
    allowed_paths: Vec<PathBuf>,
}

#[derive(Debug)]
struct BubblewrapCommand {
    program: String,
    args: Vec<String>,
    layout: SandboxLayout,
    seccomp_file: File,
    seccomp_path: PathBuf,
}

fn build_bubblewrap_command(
    prepared: &PreparedTool,
    resolved_program: &str,
    limits: &ToolExecutionLimits,
    environment: &[(String, String)],
    readonly_paths: &[(PathBuf, String)],
) -> Result<BubblewrapCommand, String> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (
            prepared,
            resolved_program,
            limits,
            environment,
            readonly_paths,
        );
        return Err("bubblewrap sandbox 仅支持 Linux".into());
    }

    #[cfg(target_os = "linux")]
    {
        let program =
            std::env::var("PENTA_BWRAP_PATH").unwrap_or_else(|_| DEFAULT_BWRAP_PATH.to_string());
        if !Path::new(&program).is_absolute() || !path_is_executable(&program) {
            return Err(format!(
                "bubblewrap 不可用: {program}；请安装 bubblewrap 或设置 PENTA_BWRAP_PATH"
            ));
        }
        let layout = prepare_sandbox_layout(limits)?;
        let (seccomp_file, seccomp_path) = create_seccomp_filter(&layout.workspace)?;
        let seccomp_fd = seccomp_file.as_raw_fd().to_string();
        let mut args = vec![
            "--unshare-all".into(),
            "--unshare-user".into(),
            "--disable-userns".into(),
            "--new-session".into(),
            "--die-with-parent".into(),
            "--cap-drop".into(),
            "ALL".into(),
            "--seccomp".into(),
            seccomp_fd,
            "--clearenv".into(),
            "--tmpfs".into(),
            "/".into(),
            "--proc".into(),
            "/proc".into(),
            "--dev".into(),
            "/dev".into(),
            "--tmpfs".into(),
            "/tmp".into(),
            "--tmpfs".into(),
            "/run".into(),
            "--ro-bind".into(),
            "/usr".into(),
            "/usr".into(),
            "--dir".into(),
            "/opt".into(),
            "--ro-bind-try".into(),
            "/opt".into(),
            "/opt".into(),
        ];
        // Arbitrary interactive/local execution never receives the host
        // network. Allow-listed target tools are validated before reaching
        // this builder and may share the network namespace.
        if !matches!(limits.network_access, NetworkPolicy::None) {
            args.insert(3, "--share-net".into());
        }
        // Controlled tool directory: consolidated user-writable symlinks,
        // mounted read-only and prepended to PATH so tools resolve uniformly
        // without depending on root-owned system paths.
        if let Some(tool_dir) = super::tool_lifecycle::controlled_tool_dir() {
            if tool_dir.is_dir() {
                bind_ro_dir(&mut args, &tool_dir);
            }
        }
        args.extend([
            "--symlink".into(),
            "usr/bin".into(),
            "/bin".into(),
            "--symlink".into(),
            "usr/sbin".into(),
            "/sbin".into(),
            "--symlink".into(),
            "usr/lib".into(),
            "/lib".into(),
            "--symlink".into(),
            "usr/lib64".into(),
            "/lib64".into(),
            "--dir".into(),
            "/etc".into(),
        ]);
        for path in [
            "/etc/alternatives",
            "/etc/hosts",
            "/etc/ld.so.cache",
            "/etc/ld.so.conf",
            "/etc/ld.so.conf.d",
            "/etc/localtime",
            "/etc/nsswitch.conf",
            "/etc/passwd",
            "/etc/group",
            "/etc/protocols",
            "/etc/resolv.conf",
            "/etc/services",
            "/etc/ssl",
        ] {
            args.extend(["--ro-bind-try".into(), path.into(), path.into()]);
        }
        bind_external_tool_runtime(&mut args, resolved_program)?;
        args.extend([
            "--dir".into(),
            "/workspace".into(),
            "--bind".into(),
            layout.workspace.to_string_lossy().into_owned(),
            "/workspace".into(),
            "--dir".into(),
            "/artifacts".into(),
            "--bind".into(),
            layout.artifacts.to_string_lossy().into_owned(),
            "/artifacts".into(),
        ]);
        for (index, path) in layout.allowed_paths.iter().enumerate() {
            let target = path.to_string_lossy().into_owned();
            args.extend(["--dir".into(), target.clone()]);
            args.extend(["--bind".into(), target.clone(), target.clone()]);
            if index == 0 {
                args.extend(["--dir".into(), "/state".into()]);
                args.extend(["--bind".into(), target, "/state".into()]);
            }
        }
        for (source, target) in readonly_paths {
            args.extend([
                "--ro-bind".into(),
                source.to_string_lossy().into_owned(),
                target.clone(),
            ]);
        }
        for (name, value) in environment {
            args.extend(["--setenv".into(), name.clone(), value.clone()]);
        }
        let sandbox_path = super::tool_lifecycle::controlled_tool_dir()
            .filter(|dir| dir.is_dir())
            .map(|dir| {
                format!(
                    "{}:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                    dir.display()
                )
            })
            .unwrap_or_else(|| {
                "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string()
            });
        args.extend([
            "--setenv".into(),
            "HOME".into(),
            "/workspace".into(),
            "--setenv".into(),
            "PATH".into(),
            sandbox_path,
            "--setenv".into(),
            "TMPDIR".into(),
            "/tmp".into(),
            "--setenv".into(),
            "KAIROS_ARTIFACT_DIR".into(),
            "/artifacts".into(),
            "--chdir".into(),
            "/workspace".into(),
        ]);

        let memory_bytes = limits.memory_mb.saturating_mul(1024 * 1024);
        let cpu_seconds = limits.timeout_seconds.max(1);
        args.extend([
            "--".into(),
            "/usr/bin/prlimit".into(),
            format!("--rss={memory_bytes}"),
            format!("--cpu={cpu_seconds}"),
            "--".into(),
            resolved_program.into(),
        ]);
        args.extend(prepared.args.clone());
        Ok(BubblewrapCommand {
            program,
            args,
            layout,
            seccomp_file,
            seccomp_path,
        })
    }
}

#[cfg(target_os = "linux")]
fn bind_ro_dir(args: &mut Vec<String>, dir: &Path) {
    let mut parents: Vec<PathBuf> = dir
        .parent()
        .into_iter()
        .flat_map(Path::ancestors)
        .filter(|path| *path != Path::new("/"))
        .map(Path::to_path_buf)
        .collect();
    parents.reverse();
    for parent in parents {
        args.extend(["--dir".into(), parent.to_string_lossy().into_owned()]);
    }
    let target = dir.to_string_lossy().into_owned();
    args.extend(["--ro-bind-try".into(), target.clone(), target]);
}

#[cfg(target_os = "linux")]
fn bind_external_tool_runtime(args: &mut Vec<String>, program: &str) -> Result<(), String> {
    let program = Path::new(program);
    if program.starts_with("/usr") || program.starts_with("/opt") {
        return Ok(());
    }
    let program = program
        .canonicalize()
        .map_err(|error| format!("无法解析工具路径 {}: {error}", program.display()))?;
    let prefix = std::fs::read(&program)
        .ok()
        .filter(|bytes| bytes.starts_with(b"#!"))
        .and_then(|_| {
            program
                .ancestors()
                .find(|path| path.file_name().is_some_and(|name| name == "bin"))
                .and_then(Path::parent)
                .map(Path::to_path_buf)
        });
    let source = prefix.as_deref().unwrap_or(&program);
    let target = source.to_string_lossy().into_owned();
    let mut parents: Vec<PathBuf> = source
        .parent()
        .into_iter()
        .flat_map(Path::ancestors)
        .filter(|path| *path != Path::new("/"))
        .map(Path::to_path_buf)
        .collect();
    parents.reverse();
    for parent in parents {
        args.extend(["--dir".into(), parent.to_string_lossy().into_owned()]);
    }
    args.extend(["--ro-bind".into(), target.clone(), target]);
    Ok(())
}

fn prepare_sandbox_layout(limits: &ToolExecutionLimits) -> Result<SandboxLayout, String> {
    let root = std::env::var("PENTA_SANDBOX_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_SANDBOX_ROOT));
    create_private_dir(&root)?;
    let key = safe_directory_key(limits.run_id.as_deref());
    let workspace = match limits.workspace_dir.as_deref() {
        Some(path) => validated_mount_directory(Path::new(path), &root)?,
        None => {
            let path = root.join("workspaces").join(&key);
            create_private_dir(&path)?;
            canonical_directory(&path)?
        }
    };
    let artifacts = match limits.artifact_dir.as_deref() {
        Some(path) => validated_mount_directory(Path::new(path), &root)?,
        None => {
            let path = root.join("artifacts").join(&key);
            create_private_dir(&path)?;
            canonical_directory(&path)?
        }
    };
    let allowed_paths = limits
        .allowed_paths
        .iter()
        .map(|path| validated_mount_directory(Path::new(path), &root))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SandboxLayout {
        workspace,
        artifacts,
        allowed_paths,
    })
}

fn safe_directory_key(run_id: Option<&str>) -> String {
    let key = run_id
        .unwrap_or("")
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .take(96)
        .collect::<String>();
    if key.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        key
    }
}

fn create_private_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path)
        .map_err(|error| format!("创建 sandbox 目录 {} 失败: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("设置 sandbox 目录权限 {} 失败: {error}", path.display()))?;
    }
    Ok(())
}

fn canonical_directory(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!(
            "sandbox 挂载路径必须是绝对路径: {}",
            path.display()
        ));
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("sandbox 挂载路径无效 {}: {error}", path.display()))?;
    if !canonical.is_dir() {
        return Err(format!("sandbox 挂载路径不是目录: {}", path.display()));
    }
    Ok(canonical)
}

fn validated_mount_directory(path: &Path, sandbox_root: &Path) -> Result<PathBuf, String> {
    let canonical = canonical_directory(path)?;
    for protected in [
        "/", "/bin", "/boot", "/dev", "/etc", "/lib", "/lib64", "/proc", "/root", "/run", "/sbin",
        "/sys", "/usr",
    ] {
        let protected = Path::new(protected);
        if canonical == protected
            || (protected != Path::new("/") && canonical.starts_with(protected))
        {
            return Err(format!(
                "拒绝将受保护的宿主机目录读写挂载到 sandbox: {}",
                canonical.display()
            ));
        }
    }
    let mut roots = vec![canonical_directory(sandbox_root)?];
    let state_root = std::env::var("PENTA_TOOL_STATE_ROOT")
        .unwrap_or_else(|_| "/tmp/kairos-pent-tool-state".into());
    if let Ok(root) = canonical_directory(Path::new(&state_root)) {
        roots.push(root);
    }
    if let Ok(configured) = std::env::var("PENTA_SANDBOX_ALLOWED_ROOTS") {
        for root in configured
            .split(',')
            .map(str::trim)
            .filter(|root| !root.is_empty())
        {
            roots.push(canonical_directory(Path::new(root))?);
        }
    }
    if roots.iter().any(|root| canonical.starts_with(root)) {
        Ok(canonical)
    } else {
        Err(format!(
            "sandbox 挂载路径 {} 不在允许根目录内；使用 PENTA_SANDBOX_ALLOWED_ROOTS 显式授权",
            canonical.display()
        ))
    }
}

fn canonical_file(path: &Path) -> Result<PathBuf, String> {
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("sandbox 只读文件无效 {}: {error}", path.display()))?;
    if !canonical.is_file() {
        return Err(format!("sandbox 只读挂载不是文件: {}", path.display()));
    }
    Ok(canonical)
}

#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Clone, Copy)]
struct SockFilter {
    code: u16,
    jt: u8,
    jf: u8,
    k: u32,
}

#[cfg(target_os = "linux")]
fn create_seccomp_filter(directory: &Path) -> Result<(File, PathBuf), String> {
    const BPF_LD_W_ABS: u16 = 0x20;
    const BPF_JMP_JEQ_K: u16 = 0x15;
    const BPF_RET_K: u16 = 0x06;
    const SECCOMP_RET_KILL_PROCESS: u32 = 0x8000_0000;
    const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
    const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
    #[cfg(target_arch = "x86_64")]
    const AUDIT_ARCH: u32 = 0xc000_003e;
    #[cfg(target_arch = "aarch64")]
    const AUDIT_ARCH: u32 = 0xc000_00b7;
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    compile_error!("bubblewrap seccomp profile currently supports x86_64 and aarch64");

    let mut filters = vec![
        SockFilter {
            code: BPF_LD_W_ABS,
            jt: 0,
            jf: 0,
            k: 4,
        },
        SockFilter {
            code: BPF_JMP_JEQ_K,
            jt: 1,
            jf: 0,
            k: AUDIT_ARCH,
        },
        SockFilter {
            code: BPF_RET_K,
            jt: 0,
            jf: 0,
            k: SECCOMP_RET_KILL_PROCESS,
        },
        SockFilter {
            code: BPF_LD_W_ABS,
            jt: 0,
            jf: 0,
            k: 0,
        },
    ];
    let denied_syscalls = [
        libc::SYS_add_key,
        libc::SYS_bpf,
        libc::SYS_delete_module,
        libc::SYS_finit_module,
        libc::SYS_init_module,
        libc::SYS_kexec_load,
        libc::SYS_keyctl,
        libc::SYS_mount,
        libc::SYS_open_by_handle_at,
        libc::SYS_perf_event_open,
        libc::SYS_pivot_root,
        libc::SYS_ptrace,
        libc::SYS_reboot,
        libc::SYS_request_key,
        libc::SYS_setns,
        libc::SYS_swapoff,
        libc::SYS_swapon,
        libc::SYS_umount2,
        libc::SYS_unshare,
        libc::SYS_userfaultfd,
    ];
    for syscall in denied_syscalls {
        filters.push(SockFilter {
            code: BPF_JMP_JEQ_K,
            jt: 0,
            jf: 1,
            k: syscall as u32,
        });
        filters.push(SockFilter {
            code: BPF_RET_K,
            jt: 0,
            jf: 0,
            k: SECCOMP_RET_ERRNO | libc::EPERM as u32,
        });
    }
    filters.push(SockFilter {
        code: BPF_RET_K,
        jt: 0,
        jf: 0,
        k: SECCOMP_RET_ALLOW,
    });

    let path = directory.join(format!(".seccomp-{}.bpf", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|error| format!("创建 seccomp 过滤器失败: {error}"))?;
    let bytes = unsafe {
        std::slice::from_raw_parts(
            filters.as_ptr().cast::<u8>(),
            filters.len() * std::mem::size_of::<SockFilter>(),
        )
    };
    file.write_all(bytes)
        .map_err(|error| format!("写入 seccomp 过滤器失败: {error}"))?;
    file.flush()
        .map_err(|error| format!("刷新 seccomp 过滤器失败: {error}"))?;
    file.rewind()
        .map_err(|error| format!("重置 seccomp 过滤器读取位置失败: {error}"))?;
    Ok((file, path))
}

#[cfg(not(target_os = "linux"))]
fn create_seccomp_filter(_directory: &Path) -> Result<(File, PathBuf), String> {
    Err("seccomp 仅支持 Linux".into())
}

/// Fast synchronous check: does `path` point to an executable regular file?
fn path_is_executable(path: &str) -> bool {
    std::fs::metadata(path)
        .map(|m| m.is_file())
        .unwrap_or(false)
}

/// Whether on-demand auto-install is allowed.
/// Controlled by `PENTA_AUTO_INSTALL_TOOLS` (default: enabled).
pub(crate) fn auto_install_enabled() -> bool {
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

    #[test]
    fn run_id_header_is_injected_for_curl_recipes_only() {
        let mut curl = PreparedTool {
            program: "/usr/bin/curl",
            args: vec!["https://example.test".into()],
        };
        inject_run_id_header(&mut curl, Some("run-123"));
        assert!(
            curl.args
                .windows(2)
                .any(|pair| pair == ["--header", "x-kairos-run-id: run-123"])
        );

        let mut nmap = PreparedTool {
            program: "/usr/bin/nmap",
            args: vec!["example.test".into()],
        };
        inject_run_id_header(&mut nmap, Some("run-123"));
        assert!(!nmap.args.iter().any(|arg| arg.contains("x-kairos-run-id")));
    }

    #[test]
    fn proxy_url_carries_a_safe_run_id_as_basic_username() {
        assert_eq!(
            proxy_url_for_run("http://127.0.0.1:8085", Some("run-123")),
            "http://run-123:@127.0.0.1:8085"
        );
        assert_eq!(
            proxy_url_for_run("http://127.0.0.1:8085", Some("../escape")),
            "http://127.0.0.1:8085"
        );
        assert_eq!(
            proxy_url_for_run("127.0.0.1:8085", Some("run-123")),
            "127.0.0.1:8085"
        );
    }

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
        assert_eq!(output.exit_code, 0, "sandbox stderr: {}", output.stderr);
        assert!(output.stdout.contains("Nmap scan report"));
        assert!(output.duration_ms > 0);
    }

    #[tokio::test]
    async fn bubblewrap_enforces_filesystem_capabilities_and_seccomp() {
        if !Path::new(DEFAULT_BWRAP_PATH).exists() || !Path::new("/usr/bin/unshare").exists() {
            return;
        }
        let key = uuid::Uuid::new_v4().to_string();
        let root = PathBuf::from(DEFAULT_SANDBOX_ROOT)
            .join("integration-tests")
            .join(&key);
        let workspace = root.join("workspace");
        let artifacts = root.join("artifacts");
        let state = root.join("state");
        create_private_dir(&workspace).unwrap();
        create_private_dir(&artifacts).unwrap();
        create_private_dir(&state).unwrap();
        let host_secret = root.join("host-secret");
        std::fs::write(&host_secret, "must-not-be-visible").unwrap();
        let script = format!(
            "set -eu; \
             test -w /workspace; test -w /artifacts; \
             test ! -e '{}'; \
             test \"$(awk '/CapEff/ {{print $2}}' /proc/self/status)\" = 0000000000000000; \
             if unshare --mount true 2>/dev/null; then exit 42; fi; \
             printf workspace-ok > /workspace/result; \
             printf artifact-ok > /artifacts/result; \
             printf state-ok > /state/result; \
             printf isolated",
            host_secret.display()
        );
        let prepared = PreparedTool {
            program: "/usr/bin/sh",
            args: vec!["-c".into(), script],
        };
        let limits = ToolExecutionLimits {
            network_access: NetworkPolicy::Full,
            workspace_dir: Some(workspace.to_string_lossy().into_owned()),
            artifact_dir: Some(artifacts.to_string_lossy().into_owned()),
            allowed_paths: vec![state.to_string_lossy().into_owned()],
            run_id: Some(key),
            ..Default::default()
        };
        let output = execute_prepared_tool("sandbox_integration", &prepared, &limits)
            .await
            .unwrap();
        assert_eq!(output.exit_code, 0, "sandbox stderr: {}", output.stderr);
        assert_eq!(output.stdout, "isolated");
        assert_eq!(
            std::fs::read_to_string(workspace.join("result")).unwrap(),
            "workspace-ok"
        );
        assert_eq!(
            std::fs::read_to_string(artifacts.join("result")).unwrap(),
            "artifact-ok"
        );
        assert_eq!(
            std::fs::read_to_string(state.join("result")).unwrap(),
            "state-ok"
        );
        assert_eq!(output.audit.backend, "bubblewrap");
        assert!(output.audit.mount_namespace);
        assert!(output.audit.capabilities_dropped);
        assert!(output.audit.seccomp);
    }

    #[test]
    fn bubblewrap_runner_enforces_local_isolation_layers() {
        let bubblewrap =
            std::env::var("PENTA_BWRAP_PATH").unwrap_or_else(|_| DEFAULT_BWRAP_PATH.to_string());
        if !std::path::Path::new(&bubblewrap).is_file() {
            eprintln!("skipping bubblewrap command test: {bubblewrap} is unavailable");
            return;
        }
        crate::infrastructure::recipe::load_recipes_for_tests();
        let prepared = super::prepare_tool(
            "tcp_scan",
            &["scan".into(), "127.0.0.1".into()],
            &NetworkPolicy::TargetOnly("127.0.0.1".into()),
        )
        .unwrap();
        let limits = ToolExecutionLimits {
            network_access: NetworkPolicy::None,
            run_id: Some(format!("sandbox-test-{}", uuid::Uuid::new_v4())),
            ..Default::default()
        };
        let command =
            build_bubblewrap_command(&prepared, prepared.program, &limits, &[], &[]).unwrap();
        assert_eq!(command.program, "/usr/bin/bwrap");
        assert!(
            command
                .args
                .windows(2)
                .any(|pair| pair == ["--cap-drop", "ALL"])
        );
        assert!(command.args.contains(&"--seccomp".to_string()));
        assert!(command.args.contains(&"--unshare-all".to_string()));
        assert!(!command.args.contains(&"--share-net".to_string()));
        assert!(command.args.contains(&"/workspace".to_string()));
        assert!(command.args.contains(&"/artifacts".to_string()));
        assert!(command.args.contains(&"/usr/bin/prlimit".to_string()));
        assert!(command.args.contains(&"/usr/bin/nmap".to_string()));
        drop(command.seccomp_file);
        let _ = std::fs::remove_file(command.seccomp_path);
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

    #[test]
    fn cloud_secret_environment_uses_a_strict_allowlist() {
        assert!(validate_secret_environment("AWS_ACCESS_KEY_ID", "AKIAEXAMPLE").is_ok());
        assert!(validate_secret_environment("AWS_SECRET_ACCESS_KEY", "secret").is_ok());
        assert!(validate_secret_environment("PATH", "/attacker/bin").is_err());
        assert!(validate_secret_environment("LD_PRELOAD", "/tmp/inject.so").is_err());
        assert!(validate_secret_environment("AWS_SESSION_TOKEN", "line\nbreak").is_err());
        assert!(validate_secret_environment("AWS_SESSION_TOKEN", "").is_err());
    }
}
