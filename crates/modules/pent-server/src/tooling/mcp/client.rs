//! MCP JSON-RPC client — manages one MCP server child process.
//!
//! Communication pattern:
//!   spawn → initialize → tools/list (cache) → tools/call (on demand)
//!
//! Framing: Content-Length: N\r\n\r\n{json}

use super::protocol::*;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex;
use tracing::{debug, warn};

const CALL_TIMEOUT: Duration = Duration::from_secs(120);
const INIT_TIMEOUT: Duration = Duration::from_secs(120);

pub struct McpClient {
    pub server_id: String,
    #[allow(dead_code)]
    command_str: String,
    #[allow(dead_code)]
    args: Vec<String>,
    child: Mutex<Option<Child>>,
    stdin: Mutex<Option<ChildStdin>>,
    stdout: Mutex<Option<BufReader<tokio::process::ChildStdout>>>,
    next_id: AtomicU64,
    #[allow(dead_code)]
    initialized: std::sync::atomic::AtomicBool,
}

impl McpClient {
    /// Create a new client. The child process is lazily spawned on first use.
    pub fn new(server_id: String, command: String, args: Vec<String>) -> Self {
        Self {
            server_id,
            command_str: command,
            args,
            child: Mutex::new(None),
            stdin: Mutex::new(None),
            stdout: Mutex::new(None),
            next_id: AtomicU64::new(1),
            initialized: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Spawn the MCP server process and complete the initialize handshake.
    pub async fn start(&self) -> anyhow::Result<InitializeResult> {
        let config = super::registry::server_config(&self.server_id)?;

        debug!(server_id = %self.server_id, command = %config.command, "spawning MCP server");
        let mut child = Command::new(&config.command)
            .args(&config.args)
            .envs(&config.env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let stdin = child.stdin.take().ok_or_else(|| anyhow::anyhow!("child stdin missing"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("child stdout missing"))?;
        let stderr = child.stderr.take();

        // Stderr logging background task
        if let Some(stderr) = stderr {
            let sid = self.server_id.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    warn!(server_id = %sid, stderr = %line, "MCP server stderr");
                }
            });
        }

        let mut stdin_guard = self.stdin.lock().await;
        *stdin_guard = Some(stdin);
        drop(stdin_guard);

        let mut stdout_guard = self.stdout.lock().await;
        *stdout_guard = Some(BufReader::new(stdout));
        drop(stdout_guard);

        let mut child_guard = self.child.lock().await;
        *child_guard = Some(child);
        drop(child_guard);

        // Send initialize
        let init_result: InitializeResult = self
            .call_raw(
                "initialize",
                Some(serde_json::to_value(InitializeRequest {
                    protocol_version: PROTOCOL_VERSION.into(),
                    capabilities: ClientCapabilities::default(),
                    client_info: Implementation {
                        name: "kairos-pent-server".into(),
                        version: env!("CARGO_PKG_VERSION").into(),
                    },
                })?),
                INIT_TIMEOUT,
            )
            .await?;

        debug!(
            server_id = %self.server_id,
            server_name = %init_result.server_info.name,
            protocol = %init_result.protocol_version,
            "MCP server initialized"
        );

        // Send initialized notification (no response expected)
        self.send_notification("notifications/initialized", None)
            .await?;

        self.initialized
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(init_result)
    }

    /// Discover tools from the MCP server.
    pub async fn list_tools(&self) -> anyhow::Result<Vec<ToolDef>> {
        let result: ListToolsResult = self
            .call_raw("tools/list", None::<Value>, CALL_TIMEOUT)
            .await?;
        Ok(result.tools)
    }

    /// Call a tool on the MCP server.
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Value,
    ) -> anyhow::Result<CallToolResult> {
        let params = CallToolParams {
            name: tool_name.into(),
            arguments,
        };
        let result: CallToolResult = self
            .call_raw(
                "tools/call",
                Some(serde_json::to_value(params)?),
                CALL_TIMEOUT,
            )
            .await?;
        Ok(result)
    }

    // ── Internals ───────────────────────────────────────────────────────

    fn next_request_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Send a JSON-RPC request and wait for the matching response.
    async fn call_raw<R: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: Option<Value>,
        timeout: Duration,
    ) -> anyhow::Result<R> {
        let id = self.next_request_id();
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        };
        let body = serde_json::to_string(&request)?;

        // Write to stdin
        let mut stdin_guard = self.stdin.lock().await;
        let stdin = stdin_guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("MCP client not started"))?;
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        stdin.write_all(frame.as_bytes()).await?;
        stdin.flush().await?;
        drop(stdin_guard);

        // Read response — we need to read lines while holding the stdout reader.
        // For simplicity, re-acquire a new reader each time. In production, use
        // a dedicated read task with a response channel.
        match tokio::time::timeout(timeout, self.read_response(id)).await {
            Ok(result) => {
                let response: JsonRpcResponse = result?;
                if let Some(err) = response.error {
                    anyhow::bail!("MCP error {} {}: {}", err.code, err.message, id);
                }
                let value = response
                    .result
                    .ok_or_else(|| anyhow::anyhow!("MCP response missing result"))?;
                Ok(serde_json::from_value(value)?)
            }
            Err(_elapsed) => {
                anyhow::bail!("MCP call {} timed out after {:?}", method, timeout)
            }
        }
    }

    /// Send a notification (no response expected).
    async fn send_notification(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> anyhow::Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or(Value::Null),
        });
        let body = serde_json::to_string(&notification)?;
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);

        let mut stdin_guard = self.stdin.lock().await;
        let stdin = stdin_guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("MCP client not started"))?;
        stdin.write_all(frame.as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    /// Read a single JSON-RPC response matching the given request id.
    async fn read_response(&self, expected_id: u64) -> anyhow::Result<JsonRpcResponse> {
        let mut stdout_guard = self.stdout.lock().await;
        let reader = stdout_guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("MCP stdout not available"))?;

        let mut content_length = String::new();

        loop {
            content_length.clear();
            // Read Content-Length header line
            let n = reader.read_line(&mut content_length).await?;
            if n == 0 {
                anyhow::bail!("MCP server stdout closed unexpectedly");
            }
            let header = content_length.trim();
            if header.is_empty() {
                continue; // skip blank lines before header
            }
            if let Some(len_str) = header.strip_prefix("Content-Length:") {
                let len: usize = len_str
                    .trim()
                    .parse()
                    .map_err(|_| anyhow::anyhow!("invalid Content-Length: {len_str}"))?;

                // Read the blank separator line
                let mut blank = String::new();
                reader.read_line(&mut blank).await?;

                // Read exactly `len` bytes of JSON
                let mut json_bytes = vec![0u8; len];
                use tokio::io::AsyncReadExt;
                reader.read_exact(&mut json_bytes).await?;

                let response: JsonRpcResponse = serde_json::from_slice(&json_bytes)?;
                if response.id == expected_id {
                    return Ok(response);
                }
                // Response for a different request — log and continue
                warn!(
                    expected = expected_id,
                    received = response.id,
                    "MCP response id mismatch; skipping"
                );
            } else {
                // Not a Content-Length header; skip
                debug!(header = %header, "unexpected MCP header line");
            }
        }
    }

    /// Check if the child process is still alive.
    pub async fn is_alive(&self) -> bool {
        let mut guard = self.child.lock().await;
        if let Some(ref mut child) = *guard {
            match child.try_wait() {
                Ok(None) => true,  // still running
                _ => false,
            }
        } else {
            false
        }
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // kill_on_drop on the Child handles process cleanup
    }
}
