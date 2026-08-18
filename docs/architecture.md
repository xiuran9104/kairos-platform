# Kairos Platform 架构

本文档是平台当前真实架构的唯一权威说明。随代码演进维护，其余历史设计文档已并入本文与 `status.md`。

## 1. 定位

启元智能中台（Kairos Platform）是面向智能业务研发与运营的一体化技术平台：以 Rust/Axum 作为统一服务底座，以 Vue 3 + Vben + Ant Design Vue 提供管理控制面，通过模块化能力承载系统治理、AI 模型与 Agent、资产运营、自动化扫描和授权渗透测试等业务域。

## 2. 技术栈

| 层 | 选型 |
| --- | --- |
| 后端 | Rust（edition 2024）+ Axum 0.8 + Tokio 1.44 + SQLx 0.8 |
| 前端 | Vue 3 + TypeScript + Ant Design Vue + Vben Admin（`@vben/web-antd`），端口 5666 |
| 主数据库 | PostgreSQL 18（`sql/postgresql/0001_initial.sql` 为唯一初始化快照） |
| 缓存 | Redis 8（缓存、限流） |
| 消息 | NATS（工具执行结果、事件总线） |
| 对象存储 | MinIO（报告、证据、流量 body 归档） |
| 证书 | rustls + rcgen（MITM 代理动态签发） |

## 3. 目录结构

```text
crates/
  framework/               # 公共能力：common/database/web/security/redis/mq/tenant/telemetry
  modules/
    system-api|server      # 认证、用户、角色、权限、菜单、租户
    infra-api|server       # 配置、文件、任务、日志、数据源
    ai-api|server          # 模型管理、聊天/SSE、工具调用、知识库、图片/音乐/语音/Embedding
    media-api|server       # 素材上传与媒体能力
    pent-api               # Pent 契约：模型、事件、工具注册表、认知
    pent-core              # Pent 领域：工具归一化、路由、认知图、规划
    pent-runtime           # AgentRunner、调度、Context/Observation 生命周期
    pent-tooling           # Tool catalog/dispatcher/backend 与 MCP 实现
    pent-server            # HTTP、PostgreSQL、NATS、模型、控制面与执行协议适配器
    mitm-proxy             # 进程内 MITM 代理：CA、TLS 终止、HTTP 转发、流量捕获
    scan-api|server        # 扫描器、扫描任务、资产发现与归一化
services/
  gateway/                 # 统一入口（8080），组合各模块路由与横切中间件
  pent-agent/              # 无数据库执行节点（8081 仅健康检查）：Tool/Sandbox、终端、MITM
  scan-agent/              # 扫描执行节点
apps/web/                  # Vben Admin 前端 monorepo
sql/postgresql/            # 0001_initial.sql 全量初始化快照
script/                    # docker-compose、数据库导出/备份/迁移测试
```

Pent 依赖方向：`pent-server -> pent-runtime|pent-tooling|pent-core -> pent-api`；`pent-runtime` 与 `pent-tooling` 不依赖 `pent-server`，`pent-core` 不依赖异步运行时或文件系统。Gateway 承担 Agent/Run 控制面与 MCP 注册表，但不启动安全二进制；pent-agent 只通过 NATS 接收配置和执行请求，不能连接主数据库。

## 4. 业务域

- **System**：认证、用户、角色、权限、菜单、租户及后台管理。
- **Infra**：配置、文件、任务、日志、数据源。
- **AI**：统一模型管理、聊天/SSE、工具调用、知识库、图片、Midjourney、音乐、语音、Embedding、写作。
- **Pent**：授权渗透任务、Agent 会话、工具路由、安全护栏、运行时编排与结果追踪。
- **Scan**：扫描器、扫描任务、独立 Agent 的并发端口/只读协议探测、服务指纹、数据库精确版本漏洞目录、证据驱动风险规则与复扫生命周期，以及异构扫描结果归一化。版本目录只在产品和版本证据同时精确命中时生成待复核风险。
- **Asset Ops**：资产、机房、区域、端点、指纹与风险运营。
- **Media**：素材上传及媒体基础能力。

## 5. Pent 关键机制

### 5.1 工具执行链路

```text
pent_tool_definitions 表（初始化快照）
  -> tool_catalog 加载 active 工具定义与 JSON Schema
  -> 模型工具清单（动态披露 + 常驻证据/流量工具）
  -> Gateway Agent runtime 统一编排对话与 Run
  -> MCP：Gateway 维护注册表并调用外部服务
  -> CLI：按具体工具能力路由到 pent-agent，tool_registry::prepare_tool 按 YAML 配方渲染命令
  -> 白名单 + Scope 校验
  -> bubblewrap（最小只读根、workspace/artifact 挂载、PID/user/IPC/UTS namespace、capdrop、seccomp）
  -> prlimit + cgroup v2 资源限额
  -> execute_tool（经 ElasticExecutor：并发限制 + 熔断 + 取消）
```

安全边界：无 shell（execve 数组）、`program` 前缀白名单、目标 Scope 校验、最小 mount namespace、capabilities 全量清空、seccomp-bpf、输出字节上限。工具复用宿主机网络栈，不启用 network namespace；网络策略是应用层目标校验，不是内核级内网隔离。

### 5.2 工具通道

| 工具性质 | 通道 |
| --- | --- |
| 纯 CLI 工具 | YAML 配方（`tools/recipes.yaml`）驱动 `prepare_tool` |
| 有状态/外部服务 | Gateway 控制面的真实 MCP（stdio / HTTP / SSE） |
| 低延迟确定性逻辑 | builtin Rust handler（js_analyze、knowledge_search、证据/流量检索） |
| 平台内联工具 | `pent-runtime` 的 `InlineToolExecutor` 契约与 `pent-server` 基础设施适配器 |

### 5.3 MITM 流量抓取

架构决策：**代理为主，不升级为 eBPF 主层**。Burp 式明文记录依赖 TLS 中间人，eBPF 只能拿到密文与连接元数据。

```text
Web 工具进程（注入 http_proxy/https_proxy + ProxyChains connect 拦截 + 平台 CA 信任）
  -> mitm-proxy（127.0.0.1:8085）
     普通 HTTP：absolute-form 转发
     CONNECT：识别 HTTP/TLS；TLS 终止（CA 动态签发叶子证书）-> 解密后转发
     非 HTTP/TLS CONNECT：保持原始 TCP 隧道，不写 HTTP 流量表
  -> 捕获 request/response -> NATS TrafficSink
  -> Gateway 持久化 -> pent_http_traffic（元数据 + body，PG）
```

- 模块：`crates/modules/mitm-proxy`（`ca.rs` / `tls.rs` / `proxy.rs` / `lib.rs`）。
- CA 持久化于 `PENTA_MITM_CA_DIR`（默认 `storage/mitm/`，已 gitignore，私钥不入库）。
- 落库：pent-agent 对大 Body 按 `PENTA_MITM_INLINE_BODY_LIMIT` 分流到 MinIO，并通过 NATS 发送有界元数据；Gateway 写入 `pent_http_traffic`，数据库保留摘要、预览与对象键。
- 注入：`infrastructure/process.rs` 在流量采集开启时注入代理地址与 CA；`infrastructure/traffic_capture.rs` 为 Web 工具生成 run 级 ProxyChains 配置，使忽略代理变量的动态客户端也不能绕过采集。Nmap 与 TLS 专项检查不接入该拦截层。
- 启动：`pent-agent` 调 `mitm_sink::spawn_mitm_proxy_nats`，全程不访问 PostgreSQL。

### 5.4 对话运行时与护栏

- 自主循环：FINAL / ASK_USER / NO_PATH / REPLAN，预算、空转抑制、检查点、上下文压缩。
- HITL：审批模式、全局+会话白名单、审批 TTL、审计 Agent 复核。
- 多代理：`agent_spawn`/`agent_collect`，子代理并发与轮次上限。
- 结果治理：进程沙箱、输出上限、审计字段、电路熔断。

### 5.5 Run 调度与授权红队模式

- 唯一业务 API 前缀是 `/pent/*`，旧 `/api/tasks/*` 和 `/api/reports/*` 已移除。
- Gateway 订阅 `penta.run.requested` 作为主调度信号；PostgreSQL 租约只用于多 Gateway 排他、启动恢复和低频过期修复，不再由 pent-agent 每 3 秒轮询。
- 项目可选择 `assessment` 或 `red_team` Engagement。红队模式必须显式确认授权截止时间、资产范围和交战规则；授权撤销或过期后禁止创建或恢复 Run。
- 有效红队授权固化进 Run Scope 快照。范围内普通工具不重复逐次审批，`critical` 或 `approval_policy=always` 仍保留单动作审批。
- pent-agent 按实际安装的具体工具注册 NATS 能力路由；缺失工具不会被调度，并向 Agent 返回健康的同能力替代项。
- Worker 心跳与实际订阅使用同一份可执行配方清单；普通节点不会把特权工具误报为健康。模型认证/密钥错误直接结束 Run，不进入退避重试。
- HTTP/HTTPS 工具流量由 pent-agent 的 MITM 捕获后经 NATS 交给 Gateway 持久化；小响应内联存储，大响应正文落 MinIO，并保留 SHA-256 与对象键。
- 本地真实靶场由 `bash script/red-team-labs.sh up` 启动，全部只绑定 loopback。

## 6. 数据库

`sql/postgresql/0001_initial.sql` 是完整 `pg_dump` 快照，为 PostgreSQL 数据卷的唯一初始化来源。schema 变更流程见 `AGENTS.md`：改参考库 → `script/export-database-bootstrap.sh` 重导出 → 删除临时增量脚本 → `script/test-database-migrations.sh` 验证空库初始化。

## 7. 服务入口

| 服务 | 端口 | 职责 |
| --- | --- | --- |
| gateway | 8080 | 统一路由、鉴权、Run/Agent 编排、租约恢复、MCP、审计与持久化 |
| pent-agent | 8081（仅健康检查） | 无数据库的工具/Sandbox、受控终端与 MITM 执行节点 |
| scan-agent | — | 扫描执行节点；通过 Gateway 注册、心跳续租、领取任务并回传资产与端口结果 |
| mitm-proxy | 8085（内嵌） | 流量抓取代理 |

本地基础设施（PostgreSQL/Redis/NATS/MinIO）由 `script/docker/docker-compose.yml` 启动。
