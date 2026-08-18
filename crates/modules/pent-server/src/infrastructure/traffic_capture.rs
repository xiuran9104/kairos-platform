//! Forced HTTP(S) capture for allow-listed Web tools.
//!
//! Standard proxy variables cover runtimes such as Go and Java.  Some native
//! tools ignore those variables, so Web-only tools additionally receive a
//! ProxyChains `LD_PRELOAD` shim.  The shim routes direct TCP connects through
//! the platform MITM proxy without changing the tool command line.

use std::fs::{OpenOptions, create_dir_all, remove_file, set_permissions};
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

const CONFIG_MOUNT: &str = "/etc/kairos-proxychains.conf";
const LIBRARY_MOUNT: &str = "/etc/kairos-libproxychains.so";

/// Files and environment variables that must remain alive for one sandboxed
/// tool process. The generated config is deleted automatically afterwards.
pub(crate) struct ForcedProxyInjection {
    config_path: PathBuf,
    library_path: PathBuf,
}

impl ForcedProxyInjection {
    pub(crate) fn environment(&self) -> [(String, String); 2] {
        [
            ("LD_PRELOAD".into(), LIBRARY_MOUNT.into()),
            ("PROXYCHAINS_CONF_FILE".into(), CONFIG_MOUNT.into()),
        ]
    }

    pub(crate) fn readonly_paths(&self) -> [(PathBuf, String); 2] {
        [
            (self.config_path.clone(), CONFIG_MOUNT.into()),
            (self.library_path.clone(), LIBRARY_MOUNT.into()),
        ]
    }
}

impl Drop for ForcedProxyInjection {
    fn drop(&mut self) {
        let _ = remove_file(&self.config_path);
    }
}

/// Validate the optional process-level interception dependency at Worker
/// startup. Capture can be disabled explicitly with
/// `PENTA_TRAFFIC_CAPTURE=false`, and forced interception can be relaxed with
/// `PENTA_FORCE_WEB_PROXY=false`.
pub fn verify_runtime() -> Result<(), String> {
    if !super::mitm_sink::traffic_capture_enabled() || !forced_proxy_enabled() {
        return Ok(());
    }
    locate_proxychains_library().map(|_| ()).map_err(|error| {
        format!(
            "Web 流量强制采集不可用: {error}；请安装 proxychains4，或显式设置 PENTA_FORCE_WEB_PROXY=false"
        )
    })
}

/// Prepare transparent connect interception for a Web tool. Curl-backed
/// built-ins already produce structured traffic rows and are intentionally not
/// wrapped to avoid duplicate records.
pub(crate) fn prepare(
    tool_name: &str,
    resolved_program: &str,
    arguments: &[String],
    run_id: Option<&str>,
    proxy_addr: &str,
) -> Result<Option<ForcedProxyInjection>, String> {
    if !forced_proxy_enabled()
        || resolved_program.contains("curl")
        || !should_force_web_proxy(tool_name, arguments)
    {
        return Ok(None);
    }
    let Some(run_id) = run_id.filter(|value| valid_run_id(value)) else {
        return Ok(None);
    };
    let (host, port) = parse_proxy_endpoint(proxy_addr)?;
    let library_path = locate_proxychains_library()?;
    let config_path = write_proxychains_config(&host, port, run_id)?;
    Ok(Some(ForcedProxyInjection {
        config_path,
        library_path,
    }))
}

fn forced_proxy_enabled() -> bool {
    std::env::var("PENTA_FORCE_WEB_PROXY")
        .ok()
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
        .unwrap_or(true)
}

fn should_force_web_proxy(tool_name: &str, arguments: &[String]) -> bool {
    if matches!(
        tool_name,
        "nmap"
            | "nmap_scan"
            | "tcp_scan"
            | "rustscan"
            | "masscan"
            | "sslscan"
            | "sslyze"
            | "testssl"
            | "web_tls_scan"
            | "qsslcaudit"
    ) {
        return false;
    }
    matches!(
        tool_name,
        "x8" | "dir_fuzz"
            | "sqlmap_scan"
            | "web_vuln_scan"
            | "web_wordpress_scan"
            | "web_xss_scan"
            | "arjun"
            | "commix"
            | "davtest"
            | "dirb"
            | "dirsearch"
            | "feroxbuster"
            | "ffuf"
            | "gobuster"
            | "hakrawler"
            | "httrack"
            | "nikto"
            | "skipfish"
            | "sqlmap"
            | "wafw00f"
            | "wapiti"
            | "wfuzz"
            | "whatweb"
            | "wpscan"
            | "xsser"
            | "command_injection_validate"
            | "csrf_validate"
            | "file_inclusion_validate"
            | "file_upload_validate"
            | "sqli_validate"
            | "xss_validate"
            | "afrog"
            | "dalfox"
            | "nuclei"
            | "gau"
            | "httpx"
            | "katana"
            | "dotdotpwn"
            | "paramspider"
            | "waybackurls"
            | "jaeles"
    ) || arguments.iter().any(|argument| {
        let value = argument.to_ascii_lowercase();
        value.starts_with("http://")
            || value.starts_with("https://")
            || value.contains("=http://")
            || value.contains("=https://")
    })
}

fn parse_proxy_endpoint(proxy_addr: &str) -> Result<(String, u16), String> {
    let authority = proxy_addr
        .trim()
        .strip_prefix("http://")
        .unwrap_or(proxy_addr.trim());
    if authority.contains(['/', '@']) {
        return Err(format!(
            "PENTA_TRAFFIC_PROXY 不是受支持的本地 HTTP 代理地址: {proxy_addr}"
        ));
    }
    let (host, port) = authority
        .rsplit_once(':')
        .ok_or_else(|| format!("PENTA_TRAFFIC_PROXY 缺少端口: {proxy_addr}"))?;
    let host = host
        .parse::<Ipv4Addr>()
        .map_err(|_| format!("PENTA_TRAFFIC_PROXY 必须使用 IPv4 地址: {proxy_addr}"))?;
    let port = port
        .parse::<u16>()
        .map_err(|_| format!("PENTA_TRAFFIC_PROXY 端口无效: {proxy_addr}"))?;
    Ok((host.to_string(), port))
}

fn locate_proxychains_library() -> Result<PathBuf, String> {
    if let Ok(configured) = std::env::var("PENTA_PROXYCHAINS_LIBRARY") {
        return canonical_file(Path::new(&configured));
    }
    let multiarch = match std::env::consts::ARCH {
        "x86_64" => "x86_64-linux-gnu",
        "aarch64" => "aarch64-linux-gnu",
        architecture => {
            return Err(format!(
                "当前架构没有默认 ProxyChains 库路径: {architecture}"
            ));
        }
    };
    for candidate in [
        PathBuf::from(format!("/usr/lib/{multiarch}/libproxychains.so.4")),
        PathBuf::from("/usr/lib/libproxychains.so.4"),
    ] {
        if candidate.is_file() {
            return canonical_file(&candidate);
        }
    }
    Err("未找到 libproxychains.so.4".into())
}

fn canonical_file(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() || !path.is_file() {
        return Err(format!("文件不存在或不是绝对路径: {}", path.display()));
    }
    path.canonicalize()
        .map_err(|error| format!("无法解析文件 {}: {error}", path.display()))
}

fn write_proxychains_config(host: &str, port: u16, run_id: &str) -> Result<PathBuf, String> {
    let root = std::env::var("PENTA_SANDBOX_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp/kairos-pent-sandbox"))
        .join("capture-configs");
    create_dir_all(&root)
        .map_err(|error| format!("创建强制代理配置目录 {} 失败: {error}", root.display()))?;
    #[cfg(unix)]
    set_permissions(&root, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("设置强制代理配置目录权限失败: {error}"))?;

    let path = root.join(format!("{}.conf", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(&path)
        .map_err(|error| format!("创建强制代理配置 {} 失败: {error}", path.display()))?;
    let config = format!(
        "strict_chain\nquiet_mode\nproxy_dns\nremote_dns_subnet 224\ntcp_read_time_out 15000\ntcp_connect_time_out 8000\nlocalnet {host}:{port}/255.255.255.255\n[ProxyList]\nhttp {host} {port} {run_id} kairos\n"
    );
    if let Err(error) = file.write_all(config.as_bytes()) {
        let _ = remove_file(&path);
        return Err(format!("写入强制代理配置失败: {error}"));
    }
    Ok(path)
}

fn valid_run_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

#[cfg(test)]
mod tests {
    use super::{parse_proxy_endpoint, should_force_web_proxy, valid_run_id};

    #[test]
    fn only_http_capable_tools_receive_forced_connect_interception() {
        assert!(should_force_web_proxy("sqlmap", &[]));
        assert!(should_force_web_proxy("nuclei", &[]));
        assert!(should_force_web_proxy(
            "new_web_tool",
            &["--target=https://example.test".into()]
        ));
        assert!(!should_force_web_proxy(
            "nmap",
            &["http://example.test".into()]
        ));
        assert!(!should_force_web_proxy("sslscan", &[]));
    }

    #[test]
    fn proxy_endpoint_requires_an_ipv4_http_listener() {
        assert_eq!(
            parse_proxy_endpoint("http://127.0.0.1:8085").unwrap(),
            ("127.0.0.1".into(), 8085)
        );
        assert!(parse_proxy_endpoint("https://proxy.example:8085").is_err());
        assert!(parse_proxy_endpoint("http://127.0.0.1").is_err());
    }

    #[test]
    fn run_id_is_safe_for_proxychains_basic_auth() {
        assert!(valid_run_id("run-123_abc"));
        assert!(!valid_run_id("../escape"));
        assert!(!valid_run_id("contains space"));
    }
}
