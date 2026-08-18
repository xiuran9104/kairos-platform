# 配置与模型接入

## 后端环境变量

| 变量 | 必填 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `DATABASE_URL` | Gateway 是；pent-agent 否 | 无 | Gateway PostgreSQL 连接串；不得注入 pent-agent |
| `DATABASE_MIN_CONNECTIONS` | 否 | `1` | 最小连接数 |
| `DATABASE_MAX_CONNECTIONS` | 否 | `20` | 最大连接数 |
| `DATABASE_ACQUIRE_TIMEOUT_SECONDS` | 否 | `5` | 获取连接超时 |
| `JWT_SECRET` | 是 | 无 | 至少 32 字节 |
| `JWT_ISSUER` | 否 | `kairos-platform` | Token 签发方 |
| `JWT_AUDIENCE` | 否 | `kairos-platform-api` | Token 受众 |
| `JWT_ACCESS_TOKEN_TTL_SECONDS` | 否 | `900` | Access Token 有效期 |
| `GATEWAY_HOST` | 否 | `0.0.0.0` | 网关监听地址 |
| `GATEWAY_PORT` | 否 | `8080` | 网关端口 |
| `WEB_PERMISSIVE_CORS` | 否 | `false` | 开发调试跨域开关，设为 `true`/`1`/`yes` 时允许任意来源 |
| `REDIS_URL` | 否 | 无 | 不配置时缓存和限流自动关闭 |
| `REDIS_KEY_PREFIX` | 否 | `kairos-platform` | Redis 键前缀 |
| `NATS_URL` | Pent 必填 | `nats://localhost:4222` | Gateway 与 pent-agent 共用；对话 recipe 工具通过该通道委派给 pent-agent |
| `BOOTSTRAP_ADMIN_USERNAME` | 否 | `admin` | 初始管理员用户名 |
| `BOOTSTRAP_ADMIN_PASSWORD` | 否 | 无 | 不配置时跳过管理员初始化 |
| `RUST_LOG` | 否 | 框架默认 | tracing 日志过滤规则 |
| `PENTA_BWRAP_PATH` | 否 | `/usr/bin/bwrap` | pent-agent 强制使用的 bubblewrap 可执行文件 |
| `PENTA_SANDBOX_ROOT` | 否 | `/tmp/kairos-pent-sandbox` | run 级 workspace/artifact 宿主机根目录 |
| `PENTA_SANDBOX_ALLOWED_ROOTS` | 否 | 无 | 额外允许读写挂载到沙箱的绝对目录，逗号分隔 |
| `PENTA_TOOL_DIR` | 否 | `~/.kairos/tools/bin` | 受控工具目录，pent-agent 启动时把 PATH 上发现的工具软链至此，并只读挂载进沙箱、前置到沙箱 PATH |
| `PENTA_TRAFFIC_CAPTURE` | 否 | `true` | 启用任务 HTTP/HTTPS MITM 流量记录；仅显式设为 `false`/`0`/`no`/`off` 时关闭 |
| `PENTA_TRAFFIC_PROXY` | 否 | `http://127.0.0.1:8085` | pent-agent 内置 MITM 的本地 IPv4 代理地址 |
| `PENTA_FORCE_WEB_PROXY` | 否 | `true` | 为 Web 工具启用 ProxyChains 连接拦截；启用时缺少运行库会使 pent-agent 拒绝启动 |
| `PENTA_PROXYCHAINS_LIBRARY` | 否 | 自动探测 | 自定义 `libproxychains.so.4` 绝对路径 |
| `PENTA_RUN_RECOVERY_SECONDS` | 否 | `60` | Gateway 的租约/延迟重试恢复周期；正常 Run 由 NATS 事件立即触发 |
| `PENTA_WORKER_HEARTBEAT_SECONDS` | 否 | `10` | pent-agent 通过 NATS 上报工具能力的周期 |
| `PENTA_AUTO_INSTALL_TOOLS` | 否 | `true` | 缺失工具是否在 pent-agent 注册能力路由前尝试安装；生产建议预装并设为 `false`，避免延长启动时间 |

## Pent 本地工具执行

Pent 的主动检测工具统一在 pent-agent 宿主机通过 bubblewrap 执行，不使用 Docker/OCI 工具镜像或容器执行器。
Run 与对话中的 recipe 工具均由 pent-agent 执行；Gateway 不安装或启动安全二进制。节点按已安装的具体工具注册能力路由，没有对应能力时会快速失败并返回可用替代工具。pent-agent 不需要也不允许配置 `DATABASE_URL`。
Docker Compose 仍只用于 PostgreSQL、Redis、NATS、MinIO、Neo4j、Graphiti、
headless Chrome 和 mitmproxy 等本地基础设施。

pent-agent 启动时强制检查 bubblewrap；流量采集开启时还会检查 ProxyChains-ng，缺失时拒绝启动。安全工具仍可按现有生命周期策略安装。
生产环境建议通过 `script/install-pent-tools.sh` 提前安装并锁定版本。

常用宿主机工具：

```bash
for tool in bwrap proxychains4 nmap nuclei sqlmap ffuf gobuster whatweb nikto subfinder amass wfuzz dirsearch feroxbuster dalfox; do
  command -v "$tool" >/dev/null && echo "ok $tool" || echo "missing $tool"
done
```

核心执行约束：

- 工具命令必须来自 Rust allowlist，不能由模型传入任意程序路径。
- bubblewrap 构造最小只读文件系统，仅将 run 级目录读写挂载为 `/workspace`、`/artifacts`。
- PID/user/IPC/UTS/cgroup namespace、capabilities drop、seccomp-bpf 与 cgroup/prlimit 同时生效。
- 按当前部署决策不启用 network namespace；工具复用宿主机网络栈，`TargetOnly` 继续做应用层授权范围校验。
- Web 工具同时注入标准代理变量和 ProxyChains 连接拦截；动态程序直接 `connect(2)` 也会被送入 MITM。Go 等静态程序通过标准代理变量进入同一链路。
- Nmap、TLS 专项检查和其他非 HTTP 工具不启用连接拦截，避免改变端口扫描及真实证书检查语义。
- 仍拒绝空目标、`-` 开头目标、控制字符、link-local、广播和部分基础设施地址。
- `http_request` 的 Cookie 状态目录由 `PENTA_TOOL_STATE_ROOT` 管理，并显式挂载为沙箱内 `/state`。

### MCP 与浏览器工具

`headless-chrome` 通过 Compose 暴露 Chrome DevTools Protocol：

```bash
curl -fsS http://127.0.0.1:9222/json/version
```

`mcp_navigate_page` 首选已连接的 MCP Server；当 `chrome-devtools-mcp` stdio 初始化失败或
MCP registry 不可用时，会 fallback 到 Chrome DevTools HTTP API 打开页面：

```bash
curl -fsS -X PUT 'http://127.0.0.1:9222/json/new?http%3A%2F%2F127.0.0.1%3A5666%2F'
```

`PENTA_MCP_SERVERS` 可选，用于在 Gateway 显式声明外部 MCP Server。未配置时 Gateway 会尝试默认的
Chrome DevTools MCP；该 MCP 初始化失败不影响 `mcp_navigate_page` 的 HTTP fallback。

## 前端环境变量

开发配置位于 `apps/web/apps/web-antd/.env.development`：

- `VITE_PORT=5666`
- `VITE_BASE_URL=http://127.0.0.1:8080`
- `VITE_GLOB_API_URL=/api`
- `VITE_UPLOAD_TYPE=server`

生产环境应通过部署环境文件或运行时 `_app-config` 将 API 地址设置为网关地址。

## 统一模型管理

AI 页面统一使用 `ai.model_configs`。模型记录包含：

- `platform`：供应商协议，如 `OpenAICompatible`、`OpenAI`、`Anthropic`、`Gemini`、`AzureOpenAI`、`Midjourney`、`Suno`。
- `type`：`chat`、`image`、`video`、`speech`、`transcription`、`music`、`embedding` 或 `rerank`。
- `model`、`url`、`apiKey`：模型标识、服务根地址和密钥。
- `config`：供应商差异化 JSON 配置。

OpenAI 兼容服务常用配置键：

| 配置键 | 默认值 | 用途 |
| --- | --- | --- |
| `textPath` | `/chat/completions` | 聊天及流式聊天 |
| `imageGeneratePath` | `/images/generations` | 图片生成 |
| `videoGeneratePath` | `/videos/generations` | 视频生成 |
| `speechPath` | `/audio/speech` | TTS |
| `embeddingPath` | `/embeddings` | Embedding |
| `musicGeneratePath` | `/music/generations` | 音乐提交 |
| `musicTaskPath` | `/music/tasks/{taskId}` | 音乐轮询 |
| `midjourneyImaginePath` | `/mj/submit/imagine` | Midjourney Imagine |
| `midjourneyActionPath` | `/mj/submit/action` | Midjourney Action |
| `midjourneyTaskPath` | `/mj/task/{taskId}/fetch` | Midjourney 轮询 |
| `authHeader` | `Authorization` | 自定义鉴权头；Authorization 自动使用 Bearer |

## 异步任务状态

AI 图片和音乐统一采用：

- `10`：处理中
- `20`：成功
- `30`：失败

服务启动后每 10 秒恢复并轮询数据库中的待处理任务；也可调用对应 `/poll` 接口触发立即同步。任务、错误、结果 URL、按钮和轮询次数均持久化，因此重启不会丢失任务。
