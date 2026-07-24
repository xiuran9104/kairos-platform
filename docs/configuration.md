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

## 前端环境变量

开发配置位于 `apps/web/apps/web-antd/.env.development`：

- `VITE_PORT=5666`
- `VITE_BASE_URL=http://127.0.0.1:8080`
- `VITE_GLOB_API_URL=/api`
- `VITE_UPLOAD_TYPE=server`

生产环境应通过部署环境文件或运行时 `_app-config` 将 API 地址设置为网关地址。

## 统一模型管理

AI 页面和 Toonflow 共用 `ai.model_configs`，业务表只保存模型 ID。模型记录包含：

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

Toonflow 的图片、视频、配音和 Agent 必须选择类型匹配且启用的统一模型 ID，不支持旧 `vendor:model` 配置。

## 异步任务状态

AI 图片和音乐统一采用：

- `10`：处理中
- `20`：成功
- `30`：失败

服务启动后每 10 秒恢复并轮询数据库中的待处理任务；也可调用对应 `/poll` 接口触发立即同步。任务、错误、结果 URL、按钮和轮询次数均持久化，因此重启不会丢失任务。
