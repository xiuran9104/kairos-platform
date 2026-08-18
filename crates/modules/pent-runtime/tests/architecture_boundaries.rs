use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("pent-runtime must live under crates/modules")
        .to_path_buf()
}

fn read(relative: &str) -> String {
    std::fs::read_to_string(workspace_root().join(relative))
        .unwrap_or_else(|error| panic!("failed to read {relative}: {error}"))
}

fn rust_sources(relative: &str) -> String {
    fn visit(path: &Path, output: &mut String) {
        for entry in std::fs::read_dir(path).expect("source directory must be readable") {
            let path = entry.expect("directory entry must be readable").path();
            if path.is_dir() {
                visit(&path, output);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                output.push_str(&std::fs::read_to_string(path).expect("source must be readable"));
            }
        }
    }
    let mut output = String::new();
    visit(&workspace_root().join(relative), &mut output);
    output
}

fn assert_absent(content: &str, owner: &str, forbidden: &[&str]) {
    for dependency in forbidden {
        assert!(
            !content.contains(dependency),
            "{owner} must not depend on or initialize {dependency}"
        );
    }
}

#[test]
fn pent_crate_dependency_direction_is_enforced() {
    let api = read("crates/modules/pent-api/Cargo.toml");
    assert_absent(
        &api,
        "pent-api",
        &[
            "pent-core",
            "pent-runtime",
            "pent-tooling",
            "pent-server",
            "sqlx",
            "axum",
            "tokio",
        ],
    );

    let core = read("crates/modules/pent-core/Cargo.toml");
    assert_absent(
        &core,
        "pent-core",
        &[
            "pent-runtime",
            "pent-tooling",
            "pent-server",
            "sqlx",
            "axum",
            "tokio",
        ],
    );

    let runtime = read("crates/modules/pent-runtime/Cargo.toml");
    assert_absent(
        &runtime,
        "pent-runtime",
        &["pent-core", "pent-tooling", "pent-server", "sqlx", "axum"],
    );

    let tooling = read("crates/modules/pent-tooling/Cargo.toml");
    assert_absent(
        &tooling,
        "pent-tooling",
        &["pent-core", "pent-runtime", "pent-server", "sqlx", "axum"],
    );

    let core_sources = rust_sources("crates/modules/pent-core/src");
    assert_absent(
        &core_sources,
        "pent-core source",
        &["std::fs", "tokio::fs", "sqlx::", "axum::"],
    );
}

#[test]
fn production_process_roles_do_not_overlap() {
    let gateway = read("services/gateway/src/main.rs");
    assert_absent(
        &gateway,
        "gateway",
        &[
            "start_worker",
            "init_global_executor",
            "PgMcpServerConfigSource",
            "spawn_conversation_tool_responder",
            "spawn_terminal_responder",
        ],
    );
    for required in [
        "start_run_coordinator",
        "mcp::registry::shared",
        "spawn_worker_heartbeat_persistence",
        "spawn_traffic_persistence",
    ] {
        assert!(
            gateway.contains(required),
            "gateway must initialize {required}"
        );
    }

    let agent = read("services/pent-agent/src/main.rs");
    for required in [
        "init_global_executor",
        "verify_bubblewrap",
        "spawn_conversation_tool_responder",
        "spawn_terminal_responder",
        "spawn_worker_heartbeat",
        "request_agent_config",
        "spawn_mitm_proxy_nats",
    ] {
        assert!(
            agent.contains(required),
            "pent-agent must initialize {required}"
        );
    }
    assert_absent(
        &agent,
        "pent-agent",
        &[
            "PentState",
            "routes(state)",
            "authenticate_pent",
            "DatabaseConfig",
            "DATABASE_URL",
            "framework_database",
            "sqlx::",
            "start_run_coordinator",
            "spawn_mcp_reconciler",
        ],
    );
    let agent_manifest = read("services/pent-agent/Cargo.toml");
    assert_absent(
        &agent_manifest,
        "pent-agent manifest",
        &["framework-database", "sqlx"],
    );
}

#[test]
fn gateway_reachable_local_tools_never_spawn_host_processes() {
    for source in [
        "crates/modules/pent-server/src/terminal.rs",
        "crates/modules/pent-server/src/tooling/builtin/local_tools.rs",
    ] {
        let content = read(source);
        assert_absent(
            &content,
            source,
            &["Command::new", "process::Command", "tokio::process"],
        );
    }

    let executor = read("crates/modules/pent-server/src/conversation/tool_executor.rs");
    assert_absent(
        &executor,
        "conversation tool executor",
        &[
            "execute_bash",
            "\"bash\" => execute_builtin_local",
            "infrastructure::process::execute_tool",
        ],
    );
    assert!(executor.contains("request_tool_execution"));
    let terminal = read("crates/modules/pent-server/src/terminal.rs");
    assert!(terminal.contains("system_operate_log"));
    assert_absent(
        &terminal,
        "gateway terminal ownership",
        &["SESSION_OWNERS", "OnceLock", "HashMap"],
    );
    let workflows = read("crates/modules/pent-server/src/workflows.rs");
    assert_absent(
        &workflows,
        "gateway workflow execution",
        &["infrastructure::process::execute_tool"],
    );
    assert!(workflows.contains("request_tool_execution"));
}

#[test]
fn migrated_server_implementations_do_not_regress() {
    let root = workspace_root();
    for removed in [
        "crates/modules/pent-server/src/conversation/tool_execution_coordinator.rs",
        "crates/modules/pent-server/src/tooling/mcp/client.rs",
        "crates/modules/pent-server/src/tooling/mcp/transport.rs",
        "crates/modules/pent-server/src/tooling/mcp/registry.rs",
        "crates/modules/pent-server/src/conversation/tool_bus.rs",
        "crates/modules/pent-server/src/conversation/checkpoint.rs",
        "crates/modules/pent-server/src/cognition",
        "crates/modules/pent-server/src/tooling/dispatcher.rs",
        "crates/modules/pent-server/src/tooling/executors/mcp.rs",
        "crates/modules/pent-server/src/tooling/mcp/mod.rs",
    ] {
        assert!(
            !root.join(removed).exists(),
            "legacy implementation returned: {removed}"
        );
    }

    let api_contracts = read("crates/modules/pent-api/src/conversation.rs");
    for contract in [
        "pub struct AgentSpawnRequest",
        "pub struct AgentChildBudget",
        "pub enum RuntimeAgentStatus",
        "pub struct RuntimeSubAgent",
    ] {
        assert!(
            api_contracts.contains(contract),
            "missing canonical API contract: {contract}"
        );
    }
    let server_sources = rust_sources("crates/modules/pent-server/src");
    assert_absent(
        &server_sources,
        "pent-server source",
        &[
            "struct ApiResponse",
            "struct AgentSpawnRequest",
            "ConversationToolDefinition",
            "ContainerSandboxBackend",
            "HostSandboxBackend",
        ],
    );
    let sandbox = read("crates/modules/pent-server/src/infrastructure/process.rs");
    for required in [
        "--unshare-all",
        "--share-net",
        "--cap-drop",
        "--seccomp",
        "/workspace",
        "/artifacts",
    ] {
        assert!(
            sandbox.contains(required),
            "bubblewrap sandbox must enforce {required}"
        );
    }
}
