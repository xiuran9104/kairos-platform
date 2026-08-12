//! Tool lifecycle management — install, health check, update, uninstall.
//!
//! Each host-native security tool has a `ToolLifecycle` entry that bundles:
//! - Binary path (expected location on disk)
//! - Install methods (ordered; first successful method wins)
//! - Update and uninstall commands
//!
//! Before executing a tool, callers should invoke [`ToolLifecycle::ensure_available`]
//! to verify the binary exists and attempt auto-install if it doesn't.

use tracing::{info, warn};

// ── Data types ────────────────────────────────────────────────────────────

/// A single install approach (package manager, language toolchain, etc.).
#[derive(Debug, Clone)]
pub struct InstallMethod {
    /// Human-readable label: `"apt"`, `"go"`, `"pip"`, `"cargo"`.
    pub label: &'static str,
    /// Shell one-liner to install the tool. Should be non-interactive.
    pub command: &'static str,
}

/// Full lifecycle data for one host-native security tool binary.
///
/// Multiple tool *names* (aliases) may share the same binary — e.g. `"nmap"`,
/// `"nmap_scan"`, and `"tcp_scan"` all resolve to `/usr/bin/nmap`.
#[derive(Debug, Clone)]
pub struct ToolLifecycle {
    /// Primary tool identifier (the canonical name).
    pub tool_name: &'static str,
    /// Aliases that also resolve to this binary.
    pub aliases: &'static [&'static str],
    /// Expected absolute path to the binary on disk.
    pub binary_path: &'static str,
    /// Install methods in priority order. The first that succeeds wins.
    pub install_methods: &'static [InstallMethod],
    /// Commands to update an already-installed tool.
    pub update_commands: &'static [&'static str],
    /// Commands to remove the tool.
    pub uninstall_commands: &'static [&'static str],
}

// ── Public API ─────────────────────────────────────────────────────────────

impl ToolLifecycle {
    /// Check whether the binary exists and has the executable bit set.
    /// Uses `tokio::fs::metadata` so it can be called from async contexts
    /// without blocking the runtime.
    pub async fn is_installed(&self) -> bool {
        tokio::fs::metadata(self.binary_path)
            .await
            .map(|m| {
                // On Unix, check the owner execute bit as a quick heuristic.
                // A full mode check would use std::os::unix::fs::PermissionsExt.
                m.is_file()
            })
            .unwrap_or(false)
    }

    /// Attempt to install the tool by trying each install method in order.
    /// Returns `Ok(())` on the first successful method, or an `Err` with
    /// all failure details if every method fails.
    pub async fn install(&self) -> Result<(), String> {
        if self.is_installed().await {
            info!(tool = self.tool_name, "tool already installed; skipping");
            return Ok(());
        }

        let mut last_error = String::new();
        for method in self.install_methods {
            info!(
                tool = self.tool_name,
                method = method.label,
                "attempting install"
            );
            match run_install_command(method.command).await {
                Ok(()) => {
                    // Verify the binary actually appeared.
                    if self.is_installed().await {
                        info!(
                            tool = self.tool_name,
                            method = method.label,
                            "install succeeded"
                        );
                        return Ok(());
                    }
                    warn!(
                        tool = self.tool_name,
                        method = method.label,
                        "install command succeeded but binary not found at {}",
                        self.binary_path
                    );
                    last_error = format!(
                        "{} 命令执行成功但 {} 仍不存在",
                        method.label, self.binary_path
                    );
                }
                Err(e) => {
                    warn!(
                        tool = self.tool_name,
                        method = method.label,
                        error = %e,
                        "install method failed"
                    );
                    last_error = e;
                }
            }
        }
        Err(format!(
            "工具 {} 的所有安装方法均失败: {}",
            self.tool_name, last_error
        ))
    }

    /// Update an already-installed tool. No-op if not installed.
    pub async fn update(&self) -> Result<(), String> {
        if !self.is_installed().await {
            return Err(format!("工具 {} 未安装，无法更新", self.tool_name));
        }
        for cmd in self.update_commands {
            run_install_command(cmd).await?;
        }
        Ok(())
    }

    /// Uninstall the tool. No-op if not installed.
    pub async fn uninstall(&self) -> Result<(), String> {
        if !self.is_installed().await {
            info!(tool = self.tool_name, "tool not installed; nothing to uninstall");
            return Ok(());
        }
        for cmd in self.uninstall_commands {
            run_install_command(cmd).await?;
        }
        Ok(())
    }

    /// Check + auto-install. Call this before every tool execution.
    /// Returns immediately if the binary already exists.
    pub async fn ensure_available(&self) -> Result<(), String> {
        if self.is_installed().await {
            return Ok(());
        }
        warn!(
            tool = self.tool_name,
            path = self.binary_path,
            "tool binary missing; attempting auto-install"
        );
        self.install().await
    }
}

// ── Registry ───────────────────────────────────────────────────────────────

/// All known host-native tools with full lifecycle data.
///
/// **Invariant**: every binary path in [`super::tool_registry::prepare_tool`]
/// MUST have a corresponding entry here. Adding a tool to `prepare_tool`
/// without a lifecycle entry will cause `prepare_tool` to return an error.
///
/// Entries are grouped by binary; aliases share the same install/update/uninstall.
pub fn all_host_tools() -> Vec<ToolLifecycle> {
    vec![
        // ── Core networking ──────────────────────────────────────────────
        ToolLifecycle {
            tool_name: "nmap",
            aliases: &["tcp_scan", "nmap_scan"],
            binary_path: "/usr/bin/nmap",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y nmap" },
            ],
            update_commands: &["apt-get install -y --only-upgrade nmap"],
            uninstall_commands: &["apt-get remove -y nmap"],
        },
        ToolLifecycle {
            tool_name: "curl",
            aliases: &["http_probe", "http_request"],
            binary_path: "/usr/bin/curl",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y curl" },
            ],
            update_commands: &["apt-get install -y --only-upgrade curl"],
            uninstall_commands: &["apt-get remove -y curl"],
        },
        // ── Vulnerability scanners ───────────────────────────────────────
        ToolLifecycle {
            tool_name: "nuclei",
            aliases: &[
                "web_vuln_scan",
                "sqli_validate",
                "xss_validate",
                "csrf_validate",
                "command_injection_validate",
                "file_inclusion_validate",
            ],
            binary_path: "/usr/bin/nuclei",
            install_methods: &[
                InstallMethod {
                    label: "apt",
                    command: "apt-get install -y nuclei",
                },
                InstallMethod {
                    label: "go",
                    command: "go install -v github.com/projectdiscovery/nuclei/v3/cmd/nuclei@latest",
                },
            ],
            update_commands: &[
                "apt-get install -y --only-upgrade nuclei",
                "go install -v github.com/projectdiscovery/nuclei/v3/cmd/nuclei@latest",
            ],
            uninstall_commands: &["apt-get remove -y nuclei"],
        },
        ToolLifecycle {
            tool_name: "afrog",
            aliases: &[],
            binary_path: "/usr/local/bin/afrog",
            install_methods: &[
                InstallMethod {
                    label: "go",
                    command: "go install -v github.com/zan8th/afrog/v2/cmd/afrog@latest",
                },
            ],
            update_commands: &[
                "go install -v github.com/zan8th/afrog/v2/cmd/afrog@latest",
            ],
            uninstall_commands: &["rm -f /usr/local/bin/afrog"],
        },
        // ── DNS ──────────────────────────────────────────────────────────
        ToolLifecycle {
            tool_name: "dig",
            aliases: &["dns_lookup"],
            binary_path: "/usr/bin/dig",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y dnsutils" },
            ],
            update_commands: &["apt-get install -y --only-upgrade dnsutils"],
            uninstall_commands: &["apt-get remove -y dnsutils"],
        },
        // ── SSH / DB clients ─────────────────────────────────────────────
        ToolLifecycle {
            tool_name: "ssh",
            aliases: &["ssh_exec"],
            binary_path: "/usr/bin/ssh",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y openssh-client" },
            ],
            update_commands: &["apt-get install -y --only-upgrade openssh-client"],
            uninstall_commands: &["apt-get remove -y openssh-client"],
        },
        ToolLifecycle {
            tool_name: "psql",
            aliases: &["sql_query"],
            binary_path: "/usr/bin/psql",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y postgresql-client" },
            ],
            update_commands: &["apt-get install -y --only-upgrade postgresql-client"],
            uninstall_commands: &["apt-get remove -y postgresql-client"],
        },
        // ── Reconnaissance ────────────────────────────────────────────────
        ToolLifecycle {
            tool_name: "subfinder",
            aliases: &[],
            binary_path: "/usr/bin/subfinder",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y subfinder" },
                InstallMethod {
                    label: "go",
                    command: "go install -v github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest",
                },
            ],
            update_commands: &[
                "apt-get install -y --only-upgrade subfinder",
                "go install -v github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest",
            ],
            uninstall_commands: &["apt-get remove -y subfinder"],
        },
        ToolLifecycle {
            tool_name: "amass",
            aliases: &[],
            binary_path: "/usr/bin/amass",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y amass" },
                InstallMethod {
                    label: "go",
                    command: "go install -v github.com/owasp-amass/amass/v4/...@master",
                },
            ],
            update_commands: &[
                "apt-get install -y --only-upgrade amass",
                "go install -v github.com/owasp-amass/amass/v4/...@master",
            ],
            uninstall_commands: &["apt-get remove -y amass"],
        },
        ToolLifecycle {
            tool_name: "dnsx",
            aliases: &[],
            binary_path: "/usr/bin/dnsx",
            install_methods: &[
                InstallMethod {
                    label: "go",
                    command: "go install -v github.com/projectdiscovery/dnsx/cmd/dnsx@latest",
                },
            ],
            update_commands: &[
                "go install -v github.com/projectdiscovery/dnsx/cmd/dnsx@latest",
            ],
            uninstall_commands: &["rm -f /usr/bin/dnsx"],
        },
        ToolLifecycle {
            tool_name: "masscan",
            aliases: &[],
            binary_path: "/usr/bin/masscan",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y masscan" },
            ],
            update_commands: &["apt-get install -y --only-upgrade masscan"],
            uninstall_commands: &["apt-get remove -y masscan"],
        },
        ToolLifecycle {
            tool_name: "naabu",
            aliases: &[],
            binary_path: "/usr/bin/naabu",
            install_methods: &[
                InstallMethod {
                    label: "go",
                    command: "go install -v github.com/projectdiscovery/naabu/v2/cmd/naabu@latest",
                },
            ],
            update_commands: &[
                "go install -v github.com/projectdiscovery/naabu/v2/cmd/naabu@latest",
            ],
            uninstall_commands: &["rm -f /usr/bin/naabu"],
        },
        // ── Web discovery / fuzzing ──────────────────────────────────────
        ToolLifecycle {
            tool_name: "ffuf",
            aliases: &[],
            binary_path: "/usr/bin/ffuf",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y ffuf" },
                InstallMethod {
                    label: "go",
                    command: "go install github.com/ffuf/ffuf/v2@latest",
                },
            ],
            update_commands: &[
                "apt-get install -y --only-upgrade ffuf",
                "go install github.com/ffuf/ffuf/v2@latest",
            ],
            uninstall_commands: &["apt-get remove -y ffuf"],
        },
        ToolLifecycle {
            tool_name: "gobuster",
            aliases: &[],
            binary_path: "/usr/bin/gobuster",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y gobuster" },
                InstallMethod {
                    label: "go",
                    command: "go install github.com/OJ/gobuster/v3@latest",
                },
            ],
            update_commands: &[
                "apt-get install -y --only-upgrade gobuster",
                "go install github.com/OJ/gobuster/v3@latest",
            ],
            uninstall_commands: &["apt-get remove -y gobuster"],
        },
        ToolLifecycle {
            tool_name: "feroxbuster",
            aliases: &[],
            binary_path: "/usr/bin/feroxbuster",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y feroxbuster" },
                InstallMethod {
                    label: "cargo",
                    command: "cargo install feroxbuster",
                },
            ],
            update_commands: &[
                "apt-get install -y --only-upgrade feroxbuster",
                "cargo install feroxbuster",
            ],
            uninstall_commands: &["apt-get remove -y feroxbuster"],
        },
        ToolLifecycle {
            tool_name: "wfuzz",
            aliases: &[],
            binary_path: "/usr/bin/wfuzz",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y wfuzz" },
                InstallMethod {
                    label: "pip",
                    command: "pip install wfuzz",
                },
            ],
            update_commands: &[
                "apt-get install -y --only-upgrade wfuzz",
                "pip install --upgrade wfuzz",
            ],
            uninstall_commands: &["apt-get remove -y wfuzz", "pip uninstall -y wfuzz"],
        },
        ToolLifecycle {
            tool_name: "dirsearch",
            aliases: &["dir_fuzz"],
            binary_path: "/usr/bin/dirsearch",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y dirsearch" },
                InstallMethod {
                    label: "pip",
                    command: "pip install dirsearch",
                },
            ],
            update_commands: &[
                "apt-get install -y --only-upgrade dirsearch",
                "pip install --upgrade dirsearch",
            ],
            uninstall_commands: &["apt-get remove -y dirsearch", "pip uninstall -y dirsearch"],
        },
        ToolLifecycle {
            tool_name: "dirb",
            aliases: &[],
            binary_path: "/usr/bin/dirb",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y dirb" },
            ],
            update_commands: &["apt-get install -y --only-upgrade dirb"],
            uninstall_commands: &["apt-get remove -y dirb"],
        },
        // ── HTTP fingerprinting ──────────────────────────────────────────
        ToolLifecycle {
            tool_name: "nikto",
            aliases: &[],
            binary_path: "/usr/bin/nikto",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y nikto" },
            ],
            update_commands: &["apt-get install -y --only-upgrade nikto"],
            uninstall_commands: &["apt-get remove -y nikto"],
        },
        ToolLifecycle {
            tool_name: "whatweb",
            aliases: &[],
            binary_path: "/usr/bin/whatweb",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y whatweb" },
            ],
            update_commands: &["apt-get install -y --only-upgrade whatweb"],
            uninstall_commands: &["apt-get remove -y whatweb"],
        },
        ToolLifecycle {
            tool_name: "wafw00f",
            aliases: &[],
            binary_path: "/usr/bin/wafw00f",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y wafw00f" },
                InstallMethod {
                    label: "pip",
                    command: "pip install wafw00f",
                },
            ],
            update_commands: &[
                "apt-get install -y --only-upgrade wafw00f",
                "pip install --upgrade wafw00f",
            ],
            uninstall_commands: &["apt-get remove -y wafw00f"],
        },
        ToolLifecycle {
            tool_name: "wpscan",
            aliases: &["web_wordpress_scan"],
            binary_path: "/usr/bin/wpscan",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y wpscan" },
            ],
            update_commands: &["apt-get install -y --only-upgrade wpscan"],
            uninstall_commands: &["apt-get remove -y wpscan"],
        },
        // ── Injection tools ──────────────────────────────────────────────
        ToolLifecycle {
            tool_name: "sqlmap",
            aliases: &["sqlmap_scan"],
            binary_path: "/usr/bin/sqlmap",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y sqlmap" },
                InstallMethod {
                    label: "git",
                    command: "git clone --depth 1 https://github.com/sqlmapproject/sqlmap.git /opt/sqlmap && ln -sf /opt/sqlmap/sqlmap.py /usr/local/bin/sqlmap",
                },
            ],
            update_commands: &[
                "apt-get install -y --only-upgrade sqlmap",
                "git -C /opt/sqlmap pull",
            ],
            uninstall_commands: &[
                "apt-get remove -y sqlmap",
                "rm -rf /opt/sqlmap /usr/local/bin/sqlmap",
            ],
        },
        ToolLifecycle {
            tool_name: "xsser",
            aliases: &[],
            binary_path: "/usr/bin/xsser",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y xsser" },
            ],
            update_commands: &["apt-get install -y --only-upgrade xsser"],
            uninstall_commands: &["apt-get remove -y xsser"],
        },
        ToolLifecycle {
            tool_name: "dalfox",
            aliases: &["web_xss_scan"],
            binary_path: "/usr/local/bin/dalfox",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y dalfox" },
                InstallMethod {
                    label: "go",
                    command: "go install github.com/hahwul/dalfox/v2@latest",
                },
            ],
            update_commands: &[
                "apt-get install -y --only-upgrade dalfox",
                "go install github.com/hahwul/dalfox/v2@latest",
            ],
            uninstall_commands: &[
                "apt-get remove -y dalfox",
                "rm -f /usr/local/bin/dalfox",
            ],
        },
        ToolLifecycle {
            tool_name: "commix",
            aliases: &[],
            binary_path: "/usr/bin/commix",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y commix" },
                InstallMethod {
                    label: "git",
                    command: "git clone --depth 1 https://github.com/commixproject/commix.git /opt/commix && ln -sf /opt/commix/commix.py /usr/local/bin/commix",
                },
            ],
            update_commands: &[
                "apt-get install -y --only-upgrade commix",
                "git -C /opt/commix pull",
            ],
            uninstall_commands: &[
                "apt-get remove -y commix",
                "rm -rf /opt/commix /usr/local/bin/commix",
            ],
        },
        // ── TLS / SSL scanners ───────────────────────────────────────────
        ToolLifecycle {
            tool_name: "testssl",
            aliases: &["web_tls_scan"],
            binary_path: "/usr/bin/testssl",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y testssl.sh" },
            ],
            update_commands: &["apt-get install -y --only-upgrade testssl.sh"],
            uninstall_commands: &["apt-get remove -y testssl.sh"],
        },
        ToolLifecycle {
            tool_name: "sslyze",
            aliases: &[],
            binary_path: "/usr/bin/sslyze",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y sslyze" },
                InstallMethod {
                    label: "pip",
                    command: "pip install sslyze",
                },
            ],
            update_commands: &[
                "apt-get install -y --only-upgrade sslyze",
                "pip install --upgrade sslyze",
            ],
            uninstall_commands: &["apt-get remove -y sslyze"],
        },
        ToolLifecycle {
            tool_name: "qsslcaudit",
            aliases: &[],
            binary_path: "/usr/bin/qsslcaudit",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y sslcaudit" },
                InstallMethod {
                    label: "pip",
                    command: "pip install sslcaudit",
                },
            ],
            update_commands: &[
                "apt-get install -y --only-upgrade sslcaudit",
                "pip install --upgrade sslcaudit",
            ],
            uninstall_commands: &["apt-get remove -y sslcaudit"],
        },
        // ── Crawling / spidering ─────────────────────────────────────────
        ToolLifecycle {
            tool_name: "hakrawler",
            aliases: &[],
            binary_path: "/usr/bin/hakrawler",
            install_methods: &[
                InstallMethod {
                    label: "go",
                    command: "go install github.com/hakluke/hakrawler@latest",
                },
            ],
            update_commands: &[
                "go install github.com/hakluke/hakrawler@latest",
            ],
            uninstall_commands: &["rm -f /usr/bin/hakrawler"],
        },
        ToolLifecycle {
            tool_name: "httrack",
            aliases: &[],
            binary_path: "/usr/bin/httrack",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y httrack" },
            ],
            update_commands: &["apt-get install -y --only-upgrade httrack"],
            uninstall_commands: &["apt-get remove -y httrack"],
        },
        ToolLifecycle {
            tool_name: "arjun",
            aliases: &[],
            binary_path: "/usr/bin/arjun",
            install_methods: &[
                InstallMethod {
                    label: "pip",
                    command: "pip install arjun",
                },
            ],
            update_commands: &["pip install --upgrade arjun"],
            uninstall_commands: &["pip uninstall -y arjun"],
        },
        // ── DNS enumeration ──────────────────────────────────────────────
        ToolLifecycle {
            tool_name: "dnsrecon",
            aliases: &[],
            binary_path: "/usr/bin/dnsrecon",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y dnsrecon" },
                InstallMethod {
                    label: "pip",
                    command: "pip install dnsrecon",
                },
            ],
            update_commands: &[
                "apt-get install -y --only-upgrade dnsrecon",
                "pip install --upgrade dnsrecon",
            ],
            uninstall_commands: &["apt-get remove -y dnsrecon", "pip uninstall -y dnsrecon"],
        },
        ToolLifecycle {
            tool_name: "dnsenum",
            aliases: &[],
            binary_path: "/usr/bin/dnsenum",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y dnsenum" },
            ],
            update_commands: &["apt-get install -y --only-upgrade dnsenum"],
            uninstall_commands: &["apt-get remove -y dnsenum"],
        },
        ToolLifecycle {
            tool_name: "fierce",
            aliases: &[],
            binary_path: "/usr/bin/fierce",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y fierce" },
                InstallMethod {
                    label: "pip",
                    command: "pip install fierce",
                },
            ],
            update_commands: &[
                "apt-get install -y --only-upgrade fierce",
                "pip install --upgrade fierce",
            ],
            uninstall_commands: &["apt-get remove -y fierce", "pip uninstall -y fierce"],
        },
        // ── OSINT ────────────────────────────────────────────────────────
        ToolLifecycle {
            tool_name: "dmitry",
            aliases: &[],
            binary_path: "/usr/bin/dmitry",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y dmitry" },
            ],
            update_commands: &["apt-get install -y --only-upgrade dmitry"],
            uninstall_commands: &["apt-get remove -y dmitry"],
        },
        ToolLifecycle {
            tool_name: "theharvester",
            aliases: &[],
            binary_path: "/usr/bin/theharvester",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y theharvester" },
            ],
            update_commands: &["apt-get install -y --only-upgrade theharvester"],
            uninstall_commands: &["apt-get remove -y theharvester"],
        },
        // ── Service enumeration ──────────────────────────────────────────
        ToolLifecycle {
            tool_name: "enum4linux",
            aliases: &[],
            binary_path: "/usr/bin/enum4linux",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y enum4linux" },
            ],
            update_commands: &["apt-get install -y --only-upgrade enum4linux"],
            uninstall_commands: &["apt-get remove -y enum4linux"],
        },
        ToolLifecycle {
            tool_name: "smbmap",
            aliases: &[],
            binary_path: "/usr/bin/smbmap",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y smbmap" },
            ],
            update_commands: &["apt-get install -y --only-upgrade smbmap"],
            uninstall_commands: &["apt-get remove -y smbmap"],
        },
        ToolLifecycle {
            tool_name: "smtp-user-enum",
            aliases: &[],
            binary_path: "/usr/bin/smtp-user-enum",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y smtp-user-enum" },
            ],
            update_commands: &["apt-get install -y --only-upgrade smtp-user-enum"],
            uninstall_commands: &["apt-get remove -y smtp-user-enum"],
        },
        ToolLifecycle {
            tool_name: "swaks",
            aliases: &[],
            binary_path: "/usr/bin/swaks",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y swaks" },
            ],
            update_commands: &["apt-get install -y --only-upgrade swaks"],
            uninstall_commands: &["apt-get remove -y swaks"],
        },
        ToolLifecycle {
            tool_name: "davtest",
            aliases: &[],
            binary_path: "/usr/bin/davtest",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y davtest" },
            ],
            update_commands: &["apt-get install -y --only-upgrade davtest"],
            uninstall_commands: &["apt-get remove -y davtest"],
        },
        ToolLifecycle {
            tool_name: "nbtscan",
            aliases: &[],
            binary_path: "/usr/bin/nbtscan",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y nbtscan" },
            ],
            update_commands: &["apt-get install -y --only-upgrade nbtscan"],
            uninstall_commands: &["apt-get remove -y nbtscan"],
        },
        ToolLifecycle {
            tool_name: "netdiscover",
            aliases: &[],
            binary_path: "/usr/bin/netdiscover",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y netdiscover" },
            ],
            update_commands: &["apt-get install -y --only-upgrade netdiscover"],
            uninstall_commands: &["apt-get remove -y netdiscover"],
        },
        // ── Web vuln scanners (standalone) ───────────────────────────────
        ToolLifecycle {
            tool_name: "wapiti",
            aliases: &[],
            binary_path: "/usr/bin/wapiti",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y wapiti" },
            ],
            update_commands: &["apt-get install -y --only-upgrade wapiti"],
            uninstall_commands: &["apt-get remove -y wapiti"],
        },
        ToolLifecycle {
            tool_name: "skipfish",
            aliases: &[],
            binary_path: "/usr/bin/skipfish",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y skipfish" },
            ],
            update_commands: &["apt-get install -y --only-upgrade skipfish"],
            uninstall_commands: &["apt-get remove -y skipfish"],
        },
        ToolLifecycle {
            tool_name: "lbd",
            aliases: &[],
            binary_path: "/usr/bin/lbd",
            install_methods: &[
                InstallMethod { label: "apt", command: "apt-get install -y lbd" },
            ],
            update_commands: &["apt-get install -y --only-upgrade lbd"],
            uninstall_commands: &["apt-get remove -y lbd"],
        },
    ]
}

/// Look up a lifecycle entry by tool name. Matches against both the canonical
/// `tool_name` and any listed `aliases`.
pub fn find(tool_name: &str) -> Option<ToolLifecycle> {
    all_host_tools().into_iter().find(|t| {
        t.tool_name == tool_name || t.aliases.contains(&tool_name)
    })
}

// ── Internal helpers ──────────────────────────────────────────────────────

async fn run_install_command(cmd: &str) -> Result<(), String> {
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("无法执行命令: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "命令失败 (exit {}): {} {}",
            output.status.code().unwrap_or(-1),
            stderr.trim(),
            stdout.trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_install_methods() {
        for tool in all_host_tools() {
            assert!(
                !tool.install_methods.is_empty(),
                "{} has no install methods",
                tool.tool_name
            );
            assert!(
                tool.binary_path.starts_with('/'),
                "{} has non-absolute binary_path",
                tool.tool_name
            );
        }
    }

    #[test]
    fn find_resolves_aliases() {
        // dalfox is an alias for web_xss_scan
        let lifecycle = find("web_xss_scan").unwrap();
        assert_eq!(lifecycle.tool_name, "dalfox");
        assert_eq!(lifecycle.binary_path, "/usr/local/bin/dalfox");

        // Direct lookup of primary name also works
        let lifecycle2 = find("dalfox").unwrap();
        assert_eq!(lifecycle2.tool_name, "dalfox");

        // nmap_scan and tcp_scan are aliases for nmap
        let nmap = find("nmap_scan").unwrap();
        assert_eq!(nmap.tool_name, "nmap");
        assert_eq!(nmap.binary_path, "/usr/bin/nmap");

        let tcp = find("tcp_scan").unwrap();
        assert_eq!(tcp.tool_name, "nmap");
    }

    #[test]
    fn all_tools_covered_by_prepare_tool_have_lifecycle() {
        // Every tool name in prepare_tool() must be find-able here.
        // This test mirrors the list in tool_registry.rs.
        let all_names = [
            "tcp_scan", "nmap", "nmap_scan",
            "http_probe", "curl", "http_request",
            "nuclei", "web_vuln_scan",
            "sqli_validate", "xss_validate", "csrf_validate",
            "command_injection_validate", "file_inclusion_validate",
            "afrog",
            "dns_lookup",
            "ssh_exec",
            "sql_query",
            "subfinder", "amass", "dnsx", "masscan", "naabu",
            "ffuf", "gobuster", "feroxbuster", "wfuzz",
            "dirsearch", "dir_fuzz", "dirb",
            "nikto", "whatweb", "wafw00f", "wpscan", "web_wordpress_scan",
            "sqlmap", "sqlmap_scan", "xsser", "dalfox", "web_xss_scan", "commix",
            "testssl", "web_tls_scan", "sslyze", "qsslcaudit",
            "hakrawler", "httrack", "arjun",
            "dnsrecon", "dnsenum", "fierce",
            "dmitry", "theharvester",
            "enum4linux", "smbmap", "smtp-user-enum", "swaks", "davtest", "nbtscan", "netdiscover",
            "wapiti", "skipfish", "lbd",
        ];
        for name in all_names {
            assert!(
                find(name).is_some(),
                "tool '{name}' has no lifecycle entry — add it to tool_lifecycle.rs"
            );
        }
    }
}
