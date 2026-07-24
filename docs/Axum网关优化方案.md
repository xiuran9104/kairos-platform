# Axum 网关优化方案

## 1. 背景与决策

Kairos Platform 当前的 `services/gateway` 是基于 Axum 0.8 自建的统一 API
入口。它将 system、infra、AI、toon、media 和 asset-ops 等业务模块的
`Router` 合并到同一个进程，并统一处理认证、限流、审计和 Web 中间件。

现阶段继续演进自有 Axum Gateway，不引入 Kairos-rs、Dapr、Consul 或另一套
通用 API Gateway。原因如下：

- 当前后端仍是模块化单体，业务模块之间是进程内调用，不需要服务发现和远程路由。
- 已有 JWT/数据库认证、Redis 限流、请求 ID、审计、CORS 和优雅停机基础能力。
- 替换网关会产生认证、权限、审计和响应协议的迁移成本，但暂时不能带来明显收益。
- Nginx 继续承担生产环境 TLS、静态文件和外部反向代理；Axum Gateway 负责应用层策略。

当系统拆出三个以上独立同步服务，且出现动态服务发现、负载均衡、跨服务重试和
mTLS 等明确需求时，再单独评估 Dapr、APISIX 或 Kubernetes Gateway API。

## 2. 当前架构

```text
Browser
  |
  v
Nginx / Vite development proxy
  |
  v
services/gateway (Axum, :8080)
  |
  +-- system Router
  +-- infra Router
  +-- AI Router
  +-- toon Router
  +-- media Router
  +-- asset-ops Router
  |
  +-- PostgreSQL
  +-- Redis
  +-- NATS
```

当前已经具备：

- `x-request-id` 生成与传播；
- `TraceLayer` HTTP 日志；
- 数据库认证与 JWT Token 服务；
- Redis 固定窗口限流；
- PostgreSQL API 访问审计；
- 可配置的开发环境宽松 CORS；
- `/health` 健康检查；
- SIGINT/SIGTERM 优雅停机。

## 3. 目标架构

```text
Nginx
  |
  | TLS, static files, connection protection
  v
Axum Gateway
  |
  +-- Request context / trusted client address
  +-- Request ID / tracing
  +-- Security headers / CORS
  +-- Body limit / timeout / concurrency protection
  +-- JWT authentication / RBAC
  +-- Layered rate limiting
  +-- Audit queue
  +-- Metrics / OpenTelemetry
  |
  +-- Business Routers
       +-- regular API
       +-- upload API
       +-- streaming/SSE API
       +-- administration API
```

网关只负责入口组合与横切策略，不承载新的业务规则。业务领域逻辑继续留在对应的
`crates/modules/*-server` 中，可复用基础能力放入 `crates/framework/*`。

## 4. 第一阶段：安全和过载保护

### 4.1 可信代理和客户端地址

当前限流读取 `X-Forwarded-For`。生产实现必须仅在 TCP 对端属于可信代理时接受该
请求头，防止客户端伪造来源地址绕过限流。

新增配置：

```env
TRUSTED_PROXIES=127.0.0.1/32,10.0.0.0/8
```

解析顺序：

1. 对端属于可信代理时，解析规范化的转发链；
2. 对端不是可信代理时，忽略所有转发地址头；
3. 无法确定客户端地址时使用受控的 anonymous key，而不是信任任意请求头。

Nginx 必须覆盖客户端提交的转发头：

```nginx
proxy_set_header X-Real-IP $remote_addr;
proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
proxy_set_header X-Forwarded-Proto $scheme;
```

### 4.2 请求限制

在 `framework/web` 中增加以下 Tower/Axum 层：

- 请求 Body 大小限制；
- 普通请求总超时；
- 最大并发请求数；
- 过载时快速拒绝；
- 对拒绝结果使用统一 JSON 响应。

建议默认配置：

```env
HTTP_REQUEST_TIMEOUT_SECONDS=30
HTTP_MAX_BODY_BYTES=10485760
HTTP_MAX_CONCURRENT_REQUESTS=1000
```

不同类型路由采用不同策略：

| 路由类型 | 超时与限制 |
| --- | --- |
| 普通查询和写操作 | 默认 30 秒 |
| 文件上传 | 独立 Body 上限，最长 5 分钟 |
| SSE/流式 AI | 不使用普通总超时，配置空闲超时 |
| 扫描和媒体任务 | 快速创建任务并返回，不在 HTTP 请求内等待执行完成 |

### 4.3 CORS 和安全响应头

生产环境使用明确的 Origin 白名单：

```env
CORS_ALLOWED_ORIGINS=https://admin.example.com
CORS_ALLOWED_METHODS=GET,POST,PUT,PATCH,DELETE
CORS_ALLOW_CREDENTIALS=true
```

统一增加：

- `X-Content-Type-Options: nosniff`；
- `X-Frame-Options: DENY`；
- `Referrer-Policy`；
- `Content-Security-Policy`；
- HTTPS 环境下的 `Strict-Transport-Security`。

## 5. 第二阶段：策略化限流

限流从当前单一固定窗口升级为分层策略：

```text
global concurrency
  -> client IP
    -> user / tenant
      -> route policy
```

建议初始策略：

| 接口类型 | 建议策略 |
| --- | --- |
| 登录 | 每 IP 10 次/分钟 |
| 普通查询 | 每用户 300 次/分钟 |
| 普通写操作 | 每用户 100 次/分钟 |
| 扫描任务 | 每租户 10 次/分钟 |
| AI 请求 | 每用户最大 3 个并发请求 |
| 文件上传 | 每用户最大 2 个并发请求 |

实现要求：

- 鉴权完成后直接使用可信 `user_id` 和 `tenant_id`，不哈希完整 JWT；
- Redis 使用 Lua 脚本保证计数与 TTL 原子性；
- 返回 `Retry-After`、`X-RateLimit-Limit`、`X-RateLimit-Remaining` 和
  `X-RateLimit-Reset`；
- 登录和安全类接口在 Redis 故障时采用 fail-closed；
- 普通读取接口可按配置采用 fail-open；
- `/health` 和 `/ready` 不参与业务限流。

`governor` 可用于单进程并发或突发流量保护，但不能替代多实例共享的 Redis 业务限流。

## 6. 第三阶段：异步审计队列

当前实现为每个请求创建一个 `tokio::spawn` 写 PostgreSQL。优化后采用有界队列：

```text
HTTP audit middleware
  |
  | try_send
  v
bounded mpsc channel
  |
  v
Audit Worker
  |
  | batched insert
  v
PostgreSQL
```

实现要求：

- 队列容量、批量大小和刷新间隔可配置；
- 普通访问日志队列满时允许丢弃，并增加丢弃指标；
- 安全敏感操作使用可靠审计路径；
- 数据库临时故障不得阻塞 HTTP 响应；
- 关闭进程时停止接收新事件并在期限内排空队列；
- 默认只记录 URL path，不记录完整 query string；
- 过滤 token、签名、密码、临时密钥等敏感字段；
- 增加用户、租户、可信客户端地址和标准化路由信息。

建议配置：

```env
AUDIT_QUEUE_CAPACITY=10000
AUDIT_BATCH_SIZE=100
AUDIT_FLUSH_INTERVAL_MS=500
AUDIT_SHUTDOWN_TIMEOUT_SECONDS=10
```

## 7. 第四阶段：可观测性

### 7.1 链路追踪

使用现有 `tracing` 体系接入 OpenTelemetry OTLP：

```text
Axum / tracing
  |
  v
OpenTelemetry Collector
  +-- Tempo or Jaeger
  +-- Prometheus
  +-- Grafana
```

每个请求 Span 至少包含：

- `request_id`；
- `trace_id`；
- HTTP method；
- 标准化 route；
- status code；
- module；
- user/tenant 上下文；
- duration。

不得将 JWT、密码、云厂商密钥、请求正文或完整查询参数写入 Span。

### 7.2 Prometheus 指标

至少提供：

```text
http_requests_total
http_request_duration_seconds
http_requests_in_flight
rate_limit_rejections_total
authentication_failures_total
audit_queue_depth
audit_events_dropped_total
database_pool_connections
redis_operation_duration_seconds
```

指标标签只允许低基数字段，例如 method、route template、status code 和 module。
用户 ID、IP、Request ID、资产 ID 和完整 URL 不得作为指标标签。

## 8. 第五阶段：健康检查与关闭流程

健康检查拆分为：

| 端点 | 含义 |
| --- | --- |
| `/health` | 进程存活，避免因短暂依赖故障反复重启 |
| `/ready` | 是否可以接收业务流量 |

`/ready` 至少检查 PostgreSQL。Redis 和 NATS 是否影响就绪状态由依赖等级配置决定：

- 必需依赖异常：返回非 2xx；
- 可降级依赖异常：返回 200，但在响应和指标中标记 degraded。

收到 SIGTERM 后：

1. 就绪状态变为 false；
2. 停止接收新请求；
3. 等待在途请求完成；
4. 排空审计队列；
5. 关闭数据库、Redis 和 NATS 资源；
6. 在总关闭期限内退出。

## 9. 推荐代码边界

```text
services/gateway/src/
  main.rs                 # 初始化并启动
  app.rs                  # Router 组合和层顺序
  config.rs               # Gateway 专属配置
  audit.rs                # 审计事件和 HTTP 中间件
  audit_worker.rs         # 队列消费与批量落库
  readiness.rs            # 依赖就绪状态
  openapi.rs

crates/framework/web/src/
  middleware.rs           # 通用 Web 层组装
  request_context.rs      # Request ID、客户端地址
  trusted_proxy.rs        # 可信代理解析
  request_limits.rs       # Body、超时、并发和过载
  security_headers.rs

crates/framework/redis/src/
  rate_limit/
    mod.rs
    config.rs
    key.rs
    middleware.rs
```

依赖方向保持：

```text
gateway -> module-server -> module-api
gateway -> framework/*
module-server -> framework/*
framework/* -X-> business modules
```

只有可被多个服务复用的通用能力进入 `framework`。路由组合、网关策略选择和部署相关
行为留在 `services/gateway`。

## 10. 实施批次

### 批次一：入口安全

- 可信代理；
- Body 限制；
- 超时、并发和过载保护；
- CORS 白名单；
- 安全响应头。

### 批次二：流量治理

- 用户/租户身份限流；
- 路由级策略；
- Redis 原子计数；
- 标准限流响应头；
- Redis 故障策略测试。

### 批次三：审计可靠性

- 有界审计队列；
- 批量写入；
- 敏感字段过滤；
- 队列和丢弃指标；
- 关闭时排空。

### 批次四：可观测和运行

- OpenTelemetry OTLP；
- Prometheus metrics；
- `/ready`；
- 依赖降级状态；
- 完整优雅关闭。

## 11. 验收标准

- `cargo test --workspace` 通过；
- 普通 API、上传和 SSE 分别验证正确的超时策略；
- 客户端伪造 `X-Forwarded-For` 无法绕过限流；
- 限流计数在多个 Gateway 实例之间保持一致；
- Redis 故障行为符合接口的 fail-open/fail-closed 配置；
- 审计数据库故障不会拖慢或阻塞普通 API；
- 审计日志不包含密码、Token、签名和云厂商密钥；
- `/metrics` 可被 Prometheus 抓取；
- 请求能在 Trace 后端形成完整链路；
- `/health` 与 `/ready` 语义不同且测试覆盖；
- SIGTERM 后在途请求和审计队列能在期限内完成；
- 生产构建和部署文档包含所有新增环境变量。

## 12. 暂不实施的内容

本阶段不实施：

- 通用反向代理或动态 upstream 路由；
- Consul/Etcd 服务发现；
- 自定义分布式熔断状态机；
- Dapr Sidecar；
- Gateway 内的跨服务负载均衡；
- 将所有业务模块拆成独立进程。

扫描、AI、媒体等长耗时业务优先通过 NATS 拆成异步 Worker。只有同步微服务数量和
调用复杂度实际增长后，才引入服务发现、重试、熔断和服务间 mTLS。
