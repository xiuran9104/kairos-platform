# 配置与模型接入

## 后端环境变量

| 变量 | 必填 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `DATABASE_URL` | 是 | 无 | PostgreSQL 连接串 |
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
| `BOOTSTRAP_ADMIN_USERNAME` | 否 | `admin` | 初始管理员用户名 |
| `BOOTSTRAP_ADMIN_PASSWORD` | 否 | 无 | 不配置时跳过管理员初始化 |
| `RUST_LOG` | 否 | 框架默认 | tracing 日志过滤规则 |

## Pent 本地工具执行

Pent 的主动检测工具统一在网关宿主机本地执行，不再通过 Docker 工具镜像或容器执行器运行。
Docker Compose 仍只用于 PostgreSQL、Redis、NATS、MinIO、Neo4j、Graphiti、
headless Chrome 和 mitmproxy 等本地基础设施。

网关启动时会检查常用宿主机工具，并尝试通过系统包管理器或语言工具链快速安装缺失项。
自动安装失败不会阻止网关启动，但对应工具调用会不可用。生产环境建议提前安装并锁定版本。

常用宿主机工具：

```bash
for tool in nmap nuclei sqlmap ffuf gobuster whatweb nikto subfinder amass wfuzz dirsearch feroxbuster dalfox; do
  command -v "$tool" >/dev/null && echo "ok $tool" || echo "missing $tool"
done
```

核心执行约束：

- 工具命令必须来自 Rust allowlist，不能由模型传入任意程序路径。
- 命令通过 `/usr/bin/prlimit -- <tool>` 执行，带 CPU、内存、超时和输出截断限制。
- `TargetOnly` 不再做任务授权范围拦截；只保留目标格式安全检查。
- 仍拒绝空目标、`-` 开头目标、控制字符、link-local、广播和部分基础设施地址。
- `http_request` 的 Cookie 状态默认保存到 `/tmp/kairos-pent-tool-state/cookies.txt`。
  可通过 `PENTA_TOOL_STATE_ROOT` 调整状态目录。

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

`PENTA_MCP_SERVERS` 可选，用于显式声明外部 MCP Server。未配置时网关会尝试默认的
Chrome DevTools MCP；该 MCP 初始化失败不影响 `mcp_navigate_page` 的 HTTP fallback。

Pent 同时提供一个内部 MCP Server（`kairos-local`）。将 YAML 配方置于
`PENTA_TOOLS_DIR`（默认仓库根目录的 `tools/`）后，模型会动态收到
`mcp_kairos-local__<工具名>`。配方只描述参数模式和命令模板，不能引入任意程序：
工具名必须已在 Pent 的 Rust 生命周期白名单中，实际执行仍经过现有的资源限制、
输出截断与审计。示例见 `tools/nmap.yaml`。

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
