//! MCP transports — stdio child process and HTTP/SSE streaming.

use super::protocol::*;
use futures_util::StreamExt;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, oneshot};
use tracing::{debug, warn};

const INIT_TIMEOUT: Duration = Duration::from_secs(120);
const SSE_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Transport-level MCP operations.
#[async_trait::async_trait]
pub trait McpTransport: Send + Sync {
    /// Complete the initialize handshake and return server capabilities.
    async fn initialize(
        self: Arc<Self>,
        client_info: Implementation,
    ) -> anyhow::Result<InitializeResult>;

    /// Send a JSON-RPC request and await the matching response.
    async fn call(
        &self,
        request: JsonRpcRequest,
        timeout: Duration,
    ) -> anyhow::Result<JsonRpcResponse>;

    /// Send a notification (no response expected).
    async fn notify(&self, method: &str, params: Option<Value>) -> anyhow::Result<()>;

    /// Whether the transport is still connected.
    async fn is_alive(&self) -> bool;
}

fn next_id(next: &AtomicU64) -> u64 {
    next.fetch_add(1, Ordering::SeqCst)
}

// ── Stdio transport ──────────────────────────────────────────────────────

pub struct StdioTransport {
    server_id: String,
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    child: Mutex<Option<Child>>,
    stdin: Mutex<Option<ChildStdin>>,
    stdout: Mutex<Option<BufReader<tokio::process::ChildStdout>>>,
    next_id: AtomicU64,
    alive: AtomicBool,
}

impl StdioTransport {
    pub fn new(
        server_id: String,
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
    ) -> Self {
        Self {
            server_id,
            command,
            args,
            env,
            child: Mutex::new(None),
            stdin: Mutex::new(None),
            stdout: Mutex::new(None),
            next_id: AtomicU64::new(1),
            alive: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl McpTransport for StdioTransport {
    async fn initialize(
        self: Arc<Self>,
        client_info: Implementation,
    ) -> anyhow::Result<InitializeResult> {
        debug!(server_id = %self.server_id, command = %self.command, "spawning MCP server");
        let mut child = Command::new(&self.command)
            .args(&self.args)
            .envs(&self.env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("child stdin missing"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("child stdout missing"))?;
        let stderr = child.stderr.take();

        if let Some(stderr) = stderr {
            let sid = self.server_id.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    warn!(server_id = %sid, stderr = %line, "MCP server stderr");
                }
            });
        }

        *self.stdin.lock().await = Some(stdin);
        *self.stdout.lock().await = Some(BufReader::new(stdout));
        *self.child.lock().await = Some(child);
        self.alive.store(true, Ordering::SeqCst);

        let init_request = InitializeRequest {
            protocol_version: PROTOCOL_VERSION.into(),
            capabilities: ClientCapabilities::default(),
            client_info,
        };
        let init_response = self
            .call(
                JsonRpcRequest {
                    jsonrpc: "2.0",
                    id: next_id(&self.next_id),
                    method: "initialize".into(),
                    params: Some(serde_json::to_value(init_request)?),
                },
                INIT_TIMEOUT,
            )
            .await?;

        if let Some(err) = init_response.error {
            anyhow::bail!("MCP initialize error {}: {}", err.code, err.message);
        }
        let result = init_response
            .result
            .ok_or_else(|| anyhow::anyhow!("MCP initialize response missing result"))?;
        let init_result: InitializeResult = serde_json::from_value(result)?;

        debug!(
            server_id = %self.server_id,
            server_name = %init_result.server_info.name,
            protocol = %init_result.protocol_version,
            "MCP server initialized"
        );

        self.notify("notifications/initialized", None).await?;
        Ok(init_result)
    }

    async fn call(
        &self,
        request: JsonRpcRequest,
        timeout: Duration,
    ) -> anyhow::Result<JsonRpcResponse> {
        let body = serde_json::to_string(&request)?;
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);

        let mut stdin_guard = self.stdin.lock().await;
        let stdin = stdin_guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("MCP client not started"))?;
        stdin.write_all(frame.as_bytes()).await?;
        stdin.flush().await?;
        drop(stdin_guard);

        match tokio::time::timeout(timeout, self.read_response(request.id)).await {
            Ok(result) => result,
            Err(_elapsed) => {
                anyhow::bail!("MCP call {} timed out after {:?}", request.method, timeout)
            }
        }
    }

    async fn notify(&self, method: &str, params: Option<Value>) -> anyhow::Result<()> {
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

    async fn is_alive(&self) -> bool {
        if !self.alive.load(Ordering::SeqCst) {
            return false;
        }
        let mut guard = self.child.lock().await;
        if let Some(ref mut child) = *guard {
            match child.try_wait() {
                Ok(None) => true,
                _ => {
                    self.alive.store(false, Ordering::SeqCst);
                    false
                }
            }
        } else {
            false
        }
    }
}

impl StdioTransport {
    async fn read_response(&self, expected_id: u64) -> anyhow::Result<JsonRpcResponse> {
        let mut stdout_guard = self.stdout.lock().await;
        let reader = stdout_guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("MCP stdout not available"))?;

        let mut content_length = String::new();
        loop {
            content_length.clear();
            let n = reader.read_line(&mut content_length).await?;
            if n == 0 {
                self.alive.store(false, Ordering::SeqCst);
                anyhow::bail!("MCP server stdout closed unexpectedly");
            }
            let header = content_length.trim();
            if header.is_empty() {
                continue;
            }
            if let Some(len_str) = header.strip_prefix("Content-Length:") {
                let len: usize = len_str
                    .trim()
                    .parse()
                    .map_err(|_| anyhow::anyhow!("invalid Content-Length: {len_str}"))?;

                let mut blank = String::new();
                reader.read_line(&mut blank).await?;

                let mut json_bytes = vec![0u8; len];
                reader.read_exact(&mut json_bytes).await?;

                let response: JsonRpcResponse = serde_json::from_slice(&json_bytes)?;
                if response.id == expected_id {
                    return Ok(response);
                }
                warn!(
                    expected = expected_id,
                    received = response.id,
                    "MCP response id mismatch; skipping"
                );
            } else {
                debug!(header = %header, "unexpected MCP header line");
            }
        }
    }
}

// ── HTTP/SSE transport ───────────────────────────────────────────────────

pub struct HttpSseTransport {
    server_id: String,
    sse_url: String,
    headers: HashMap<String, String>,
    client: reqwest::Client,
    post_url: Mutex<Option<String>>,
    pending: Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>,
    next_id: AtomicU64,
    alive: AtomicBool,
}

impl HttpSseTransport {
    pub fn new(server_id: String, url: String, headers: HashMap<String, String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(SSE_CONNECT_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            server_id,
            sse_url: url,
            headers,
            client,
            post_url: Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            alive: AtomicBool::new(false),
        }
    }

    async fn post_message(&self, body: String) -> anyhow::Result<()> {
        let post_url = self
            .post_url
            .lock()
            .await
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("MCP SSE endpoint not yet received"))?
            .clone();

        let mut request = self.client.post(&post_url).body(body);
        for (name, value) in &self.headers {
            request = request.header(name, value);
        }
        request = request.header("Content-Type", "application/json");

        let response = request.send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("MCP POST failed: {status} {text}");
        }
        Ok(())
    }

    async fn sse_reader(self: Arc<Self>, response: reqwest::Response) {
        let mut stream = response.bytes_stream();
        let mut buffer = Vec::new();
        let mut current_event: Option<String> = None;

        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => buffer.extend_from_slice(&bytes),
                Err(error) => {
                    warn!(server_id = %self.server_id, %error, "MCP SSE stream error");
                    break;
                }
            }

            // Extract complete lines from buffer.
            while let Some(pos) = buffer.iter().position(|b| *b == b'\n') {
                let line_bytes = buffer.drain(..=pos).collect::<Vec<_>>();
                let line = String::from_utf8_lossy(&line_bytes);
                let line = line.trim_end_matches('\n').trim_end_matches('\r');

                if line.is_empty() {
                    // End of event: current_event holds the data payload.
                    if let Some(data) = current_event.take() {
                        self.handle_sse_event(data).await;
                    }
                    continue;
                }

                if let Some(value) = line.strip_prefix("data:") {
                    let value = value.trim_start();
                    current_event = Some(value.to_string());
                } else if line.starts_with("event:")
                    || line.starts_with("id:")
                    || line.starts_with(':')
                {
                    // Ignore non-data fields for now.
                } else {
                    debug!(server_id = %self.server_id, line = %line, "unexpected SSE line");
                }
            }
        }

        self.alive.store(false, Ordering::SeqCst);
        // Wake up any pending callers with an error.
        let mut pending = self.pending.lock().await;
        for (id, sender) in pending.drain() {
            let _ = sender.send(JsonRpcResponse {
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32000,
                    message: "MCP SSE connection closed".into(),
                    data: None,
                }),
            });
        }
    }

    async fn handle_sse_event(&self, data: String) {
        // First event is usually the endpoint URL.
        if data.starts_with("http://")
            || data.starts_with("https://")
            || data.starts_with('/')
        {
            let url = if data.starts_with('/') {
                // Resolve relative path against SSE base URL.
                match resolve_relative_url(&self.sse_url, &data) {
                    Ok(url) => url,
                    Err(error) => {
                        warn!(server_id = %self.server_id, %error, "failed to resolve MCP endpoint URL");
                        return;
                    }
                }
            } else {
                data
            };
            debug!(server_id = %self.server_id, post_url = %url, "MCP SSE endpoint received");
            *self.post_url.lock().await = Some(url);
            return;
        }

        // Otherwise treat as JSON-RPC response.
        match serde_json::from_str::<JsonRpcResponse>(&data) {
            Ok(response) => {
                let id = response.id;
                if let Some(sender) = self.pending.lock().await.remove(&id) {
                    let _ = sender.send(response);
                } else {
                    warn!(server_id = %self.server_id, id, "MCP SSE response has no pending caller");
                }
            }
            Err(error) => {
                warn!(server_id = %self.server_id, %error, data = %data, "failed to parse MCP SSE message");
            }
        }
    }
}

#[async_trait::async_trait]
impl McpTransport for HttpSseTransport {
    async fn initialize(
        self: Arc<Self>,
        client_info: Implementation,
    ) -> anyhow::Result<InitializeResult> {
        let mut request = self.client.get(&self.sse_url);
        for (name, value) in &self.headers {
            request = request.header(name, value);
        }
        request = request.header("Accept", "text/event-stream");

        let response = request.send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("MCP SSE connection failed: {status} {text}");
        }

        self.alive.store(true, Ordering::SeqCst);

        // Spawn background SSE reader that owns this Arc.
        tokio::spawn({
            let this = self.clone();
            async move { this.sse_reader(response).await }
        });

        // Wait briefly for the endpoint event, then send initialize.
        let init_request = InitializeRequest {
            protocol_version: PROTOCOL_VERSION.into(),
            capabilities: ClientCapabilities::default(),
            client_info,
        };

        // Give the reader a moment to receive the endpoint event.
        let mut waited = 0u64;
        while self.post_url.lock().await.is_none() && waited < 50 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            waited += 1;
        }

        let init_response = self
            .call(
                JsonRpcRequest {
                    jsonrpc: "2.0",
                    id: next_id(&self.next_id),
                    method: "initialize".into(),
                    params: Some(serde_json::to_value(init_request)?),
                },
                INIT_TIMEOUT,
            )
            .await?;

        if let Some(err) = init_response.error {
            anyhow::bail!("MCP initialize error {}: {}", err.code, err.message);
        }
        let result = init_response
            .result
            .ok_or_else(|| anyhow::anyhow!("MCP initialize response missing result"))?;
        let init_result: InitializeResult = serde_json::from_value(result)?;

        debug!(
            server_id = %self.server_id,
            server_name = %init_result.server_info.name,
            protocol = %init_result.protocol_version,
            "MCP HTTP/SSE server initialized"
        );

        self.notify("notifications/initialized", None).await?;
        Ok(init_result)
    }

    async fn call(
        &self,
        request: JsonRpcRequest,
        timeout: Duration,
    ) -> anyhow::Result<JsonRpcResponse> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(request.id, tx);

        let body = serde_json::to_string(&request)?;
        if let Err(error) = self.post_message(body).await {
            self.pending.lock().await.remove(&request.id);
            return Err(error);
        }

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => anyhow::bail!("MCP SSE response channel closed"),
            Err(_elapsed) => {
                self.pending.lock().await.remove(&request.id);
                anyhow::bail!(
                    "MCP SSE call {} timed out after {:?}",
                    request.method,
                    timeout
                )
            }
        }
    }

    async fn notify(&self, method: &str, params: Option<Value>) -> anyhow::Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or(Value::Null),
        });
        self.post_message(notification.to_string()).await
    }

    async fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }
}

fn resolve_relative_url(base: &str, path: &str) -> anyhow::Result<String> {
    let base_url = reqwest::Url::parse(base)?;
    let joined = base_url.join(path)?;
    Ok(joined.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_detects_http_transport_by_url() {
        let stdio = McpServerConfig {
            id: "stdio-server".into(),
            command: "/usr/bin/server".into(),
            args: vec![],
            env: HashMap::new(),
            auto_start: true,
            url: None,
            headers: HashMap::new(),
        };
        assert!(!stdio.is_http());

        let http = McpServerConfig {
            id: "http-server".into(),
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
            auto_start: true,
            url: Some("http://127.0.0.1:3000/sse".into()),
            headers: HashMap::new(),
        };
        assert!(http.is_http());
    }

    #[test]
    fn resolves_relative_endpoint_url() {
        assert_eq!(
            resolve_relative_url("http://127.0.0.1:3000/sse", "/message").unwrap(),
            "http://127.0.0.1:3000/message"
        );
        assert_eq!(
            resolve_relative_url("http://127.0.0.1:3000/mcp/sse", "message").unwrap(),
            "http://127.0.0.1:3000/mcp/message"
        );
        assert_eq!(
            resolve_relative_url("http://127.0.0.1:3000/sse", "http://other/message")
                .unwrap(),
            "http://other/message"
        );
    }
}
