# Pent 成熟工具路由框架实施任务书

## 1. 用途与目标

本文是后续 AI 编码代理的实施合同。目标不是继续扩展静态 `match`，而是在既定边界内
完成动态工具注册、权限感知路由、多执行后端、结果标准化和反馈学习。

开始任务前必须阅读根目录 `AGENTS.md`。数据库变化必须更新参考数据库、导出
`sql/postgresql/0001_initial.sql` 并运行数据库初始化测试。

```text
Agent / Domain Flow
  -> ToolRouteRequest（意图、能力、目标、预算）
  -> PreRoutePolicy（Scope、租户、角色、风险硬过滤）
  -> ToolCatalog（PostgreSQL 动态注册表）
  -> Namespace / Capability Filter
  -> Hybrid Retriever（FTS/BM25 + Vector）
  -> Deterministic Scorer
  -> Optional Small-Model Reranker
  -> ToolRouteDecision（Top 1~3）
  -> ToolOrchestrator
  -> PreExecutePolicy（二次校验）
  -> Builtin / HostLocal / MCP / HTTP Backend
  -> ToolExecutionResult
  -> Planner Feedback + Metrics + RMOS
```

任何执行后端都不能绕过 ToolOrchestrator。LLM 不直接选择镜像、端点、Shell 或 Worker。

## 2. 已建立的框架边界

### API 契约

`crates/modules/pent-api/src/tool_registry.rs` 已定义：

- `ToolDescriptor`、`ToolExecutorSpec`、`ToolResourceProfile`
- `ToolQualityMetrics`
- `ToolRouteRequest`、`ToolRouteCandidate`、`ToolRouteDecision`
- `ToolInvocation`、`ToolPolicyDecision`

执行结果统一使用 `penta-api/src/tool_result.rs` 的 `ToolExecutionResult`，不得创建第二套
结果 Envelope。

### Core 路由

`crates/modules/pent-core/src/tool_router` 已实现可运行基线：

1. 生命周期和健康状态过滤；
2. 命名空间、目标类型、必需能力过滤；
3. 已执行工具去重；
4. 最大风险过滤；
5. 确定性评分和 Top-K。

全文、向量和小模型精排必须作为可替换阶段插入，不能把基础过滤改成 Prompt。

### ToolOrchestrator

`crates/modules/pent-server/src/tooling` 已定义四个端口：

- `ToolCatalog`：注册表读取和版本解析；
- `ToolPolicyGate`：执行前确定性策略；
- `ToolExecutionBackend`：统一执行入口；
- `ToolTelemetry`：路由与执行审计。

ToolOrchestrator 只编排端口，不包含 SQL、Docker、MCP 或模型厂商逻辑。

### 数据库

已建立：

- `pent_tool_definitions`
- `pent_tool_health`
- `pent_tool_quality_metrics`
- `pent_tool_route_decisions`
- `pent_tool_feedback`

这些表已合并进唯一初始化快照 `sql/postgresql/0001_initial.sql`。

## 3. 强制架构决策

1. Agent 选择 `capability`，路由器选择具体 `tool_id + version`。
2. Scope、权限、风险和健康过滤发生在语义检索之前。
3. 执行前再次校验 Scope Snapshot，不能信任旧路由结论。
4. MCP 只是执行后端，不承担授权和沙箱职责。
5. 第三方进程工具必须使用宿主机 allowlist、绝对路径、`prlimit`、超时和输出上限。
6. 低风险、无外部进程的能力可以使用 Rust Builtin Backend。
7. 禁止向 Agent 暴露 unrestricted `bash -c`。
8. 工具版本、本地程序路径和执行审计写入 ToolCall，Run 必须可追溯。
9. 原始大输出进入对象存储，事件只保存引用与标准化摘要。
10. 全文、向量或模型失败时必须降级到确定性能力路由。

## 4. 路由评分基线

```text
capability_match       35%
lexical/vector_match   20%
scenario_quality       20%
health                 10%
information_gain       10%
cost                    5%
- risk_penalty
- recent_failure_penalty
- duplicate_penalty
```

不得把“执行成功”和“发现漏洞”混成一个 success_rate。至少区分执行成功率、有效结果率、
漏洞确认率、误报率、平均耗时和样本量。样本不足时使用全局先验。

## 5. 可分配给其他 AI 的任务

### AI-01：PostgreSQL ToolCatalog

负责：

```text
crates/modules/pent-server/src/infrastructure/tool_catalog_postgres.rs
crates/modules/pent-server/src/infrastructure/mod.rs
```

实现 `active_tools/get/registry_revision`、JSONB 严格转换、活跃版本解析和 revision 乐观锁。
集成测试覆盖新增、更新、禁用、历史版本和并发冲突。

### AI-02：工具管理 API

负责：

```text
crates/modules/pent-api/src/commands.rs
crates/modules/pent-server/src/tooling/http.rs
apps/web/apps/web-antd/src/api/pent/tools.ts
```

接口：

```text
GET    /pent/tools
GET    /pent/tools/{tool_id}/versions/{version}
POST   /pent/tools
PUT    /pent/tools/{tool_id}/versions/{version}
POST   /pent/tools/{tool_id}/versions/{version}/enable
POST   /pent/tools/{tool_id}/versions/{version}/disable
POST   /pent/tools/{tool_id}/versions/{version}/health-check
```

修改需管理员权限，不能保存密钥；更新使用 revision；所有变更产生审计事件。

### AI-03：硬策略引擎

负责：

```text
crates/modules/pent-core/src/tool_policy/
crates/modules/pent-server/src/tooling/policy_adapter.rs
```

输入 ToolDescriptor、ToolInvocation、Scope/Policy Snapshot 和身份上下文，输出
`allow/ask/deny + reason_codes + constraints`。

必须覆盖 CIDR、端口、协议、URL Path、DNS Rebinding、授权期限、审批、路径逃逸和预算。

### AI-04：PostgreSQL 全文召回

负责：

```text
crates/modules/pent-core/src/tool_router/retrieval/
crates/modules/pent-server/src/infrastructure/tool_search_postgres.rs
```

实现中英文全文搜索、能力和命名空间过滤、Top-20 召回及 search_document 更新。搜索失败
必须退回硬过滤候选，不能使 Run 失败。

### AI-05：向量检索

负责 `crates/modules/pent-server/src/infrastructure/vector/`，定义并实现：

```text
ToolVectorIndex
  upsert(tool_id, version, embedding)
  delete(tool_id, version)
  search(query_embedding, filter, top_k)
  health()
```

先实现 pgvector 或 Qdrant。记录 embedding 模型、维度和版本，支持后台重建及双版本读取。

### AI-06：小模型精排

负责 `crates/modules/pent-server/src/tooling/reranker/`。输入最多 8 个候选，输出最多 3 个。
返回工具必须来自候选集；超时、格式错误、模型不可用时回退确定性排序。

### AI-07：多执行后端

负责：

```text
crates/modules/pent-server/src/tooling/executors/
  builtin.rs
  container.rs
  mcp.rs
  http.rs
```

- Container：锁定 digest、只读根目录、无特权、资源限制和临时容器；
- MCP：连接池、超时、能力变更通知和服务认证；
- HTTP：mTLS/服务认证、重试幂等键；
- Builtin：结构化 Rust 函数，不接受任意 Shell；
- 全部输出进入统一 Normalizer。

现有 `infrastructure/process.rs` 是迁移期原型，不得继续堆积其他协议。

### AI-08：NATS Worker 调度

负责 `transport/mq.rs` 和 `tooling/dispatcher.rs`，按队列调度：

```text
penta.native.fast
penta.scan.light
penta.scan.heavy
penta.browser
penta.dangerous
penta.report
```

实现幂等、租约、心跳、取消、重试、死信和并发限制。消息不传大文件，只传对象引用。

### AI-09：质量反馈与 RMOS

负责：

```text
crates/modules/pent-core/src/tool_feedback/
crates/modules/pent-server/src/tooling/feedback_repository.rs
```

采集执行结果、漏洞确认、人工反馈、Run 结果和历史路径复用效果，异步聚合场景化质量指标。
人工反馈优先级高于模型自评，原始反馈事件不可覆盖。

### AI-10：工具生态 UI

负责 `apps/web/apps/web-antd/src/views/pent/tools/`：

- 命名空间树和工具列表；
- 版本、来源、风险、健康和执行后端；
- 本地程序路径、路由质量和失败冷却；
- 启停、健康检查、路由决策详情和人工反馈。

UI 不得显示密钥、认证 Header 或未脱敏输出。

## 6. 集成顺序

```text
AI-01 Catalog
  -> AI-02 管理 API
  -> AI-03 策略
  -> AI-04 全文召回
  -> AI-05 向量索引
  -> AI-06 小模型精排
  -> AI-07 多执行后端
  -> AI-08 Worker 队列
  -> AI-09 反馈学习
  -> AI-10 UI
```

AI-03、AI-04、AI-07 可在契约稳定后并行，但不同代理不得同时修改同一文件。

## 7. 事件协议

```text
tool.registry.changed
tool.health.changed
tool.route.requested
tool.route.decided
tool.route.fallback
tool.policy.decided
tool.invocation.queued
tool.invocation.started
tool.result.normalized
tool.feedback.recorded
```

路由决策必须保存 registry revision、policy version、策略、候选分数、选择结果和拒绝摘要。

## 8. Definition of Done

1. 新增、更新、禁用工具无需重启 Agent。
2. API、UI、路由器和执行器读取同一注册中心。
3. 100 个工具时每轮只注入不超过 8 个 Schema。
4. 禁用、越权、不健康和风险超限工具不会进入语义检索。
5. 向量库或模型不可用时可确定性降级。
6. 所有执行前完成二次 Scope/策略检查。
7. 不存在 unrestricted Shell。
8. 每次执行记录版本、本地程序路径和执行审计。
9. 结果全部转换为 `ToolExecutionResult`。
10. 人工反馈能够影响后续场景化路由分数。
11. Run 可取消、恢复、重放和审计。
12. 数据库、Rust、前端类型与生产构建验证全部通过。

## 9. 验证命令

```bash
cargo test -p kairos-platform-pent-api -p kairos-platform-pent-core -p kairos-platform-pent-server
bash script/test-database-migrations.sh
pnpm --dir apps/web --filter @vben/web-antd run typecheck
pnpm --dir apps/web --filter @vben/web-antd run build
git diff --check
```
