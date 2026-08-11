# Pent 与 DesRedTeam 对齐实施计划

本文回答一个问题：**Pent 要如何按照 DesRedTeam 的方式实现，以及现在已经实现到哪一步。**

阅读顺序：先看本文确定进度基线和剩余任务，再按任务编号接力。平台总体方案仍以
[Pent 智能渗透平台实现方案](./Pent智能渗透平台实现方案.md) 和
[pent-agent-service-plan](./pent-agent-service-plan.md) 为准，本文只负责
DesRedTeam 能力对照与移植进度。

## 1. 对照基线

- 参照工程：`/home/xiuran/code/DesRedTeam-Source-All/desredteam`（即 CyberStrikeAI 系
  DesRedTeam，`config.yaml` 标记 `version: "v1.0"`，Go 1.25+ 单体 + SQLite + 静态前端）。
- 本文基线日期：2026-08-11，对应本仓库 `140afff feat(pent): MCP toolchain + unified
  toolkit + NATS wiring + conversation fixes` 及其后的未提交改动。
- DesRed 架构要点（`docs/zh-CN/architecture.md`）：Gin Web 管理面 + Eino Agent（单代理、
  Deep、Plan-Execute、Supervisor）+ MCP 工具层（内置工具 / YAML 配方 / 外部 MCP 联邦）+
  HITL AI 围栏 + Workflow 节点图 + RAG 知识库 + 项目/攻击链/资产/漏洞运营面。

### 1.1 移植原则

Kairos Platform 是 Rust 多租户平台（PostgreSQL/Redis/NATS/MinIO），与 DesRed 的单机
SQLite 单体形态不同。因此：

1. **只吸收运行机制，不复制代码**：Eino 编排语义、HITL 围栏、YAML 工具配方、Markdown
   子代理、动态工具披露等机制在 `pent-server` 中用 Rust 重新实现。
2. **安全边界更严**：DesRed 的 YAML `command` 字段在 Pent 中只是描述性字段，执行一律走
   生命周期白名单 + `prlimit` 进程沙箱（见 `tooling/mcp/recipes.rs` 头注释）。
3. **高风险作战能力不移植**：C2、WebShell、DesShell、机器人接入不属于 Pent 目标范围。

## 2. 已实现对照（进度基线）

以下为代码中已验证存在的实现，后续工作在此基础上增量演进，不要重建第二套。

| DesRed 能力 | Pent 实现状态 | 主要位置 |
| --- | --- | --- |
| Eino 编排模式（deep / plan_execute / supervisor） | 已实现模式契约、提示注入和配置校验 | `pent-server/src/conversation/agent_pipeline.rs`（`OrchestrationMode`）、`agent_http.rs`（`/pent/agent/settings`，默认 `deep`） |
| Agent 运行参数（max_iterations、context_budget、摘要触发比、重试、参数/命令字节上限） | 已实现，租户级配置存入 `pent_runtime_policies.agent_config` | `agent_http.rs` 默认值与校验；前端 `views/pent/platform-settings/` 的“DesRed Agent 运行设置”卡片 |
| 工具调用前置校验（参数 256 KiB、Shell 命令 64 KiB、JSON 修复） | 已实现 | `agent_pipeline.rs::validate_tool_call` |
| 模型错误瞬时重试分类 | 已实现 | `agent_pipeline.rs::is_transient_model_error` |
| 多代理（子代理并发执行、结果收集） | 已实现 `agent_spawn` / `agent_collect` / `agent_status` / `agent_list`，含并发上限与子代理轮次/请求/空转硬限制 | `conversation/agent_manager.rs`、`tool_catalog.rs` |
| Markdown 子代理定义（DesRed `agents/*.md`） | 已实现加载器（frontmatter + 正文，拒绝路径穿越），仓库已有 3 个 profile | `conversation/agent_profiles.rs`、`agents/web-recon.md`、`vulnerability-validator.md`、`report-reviewer.md` |
| HITL / AI 围栏（审批模式、review_edit、全局+会话白名单、审批 TTL） | 已实现 | `conversation/hitl.rs`、`hitl_http.rs`、`runtime.rs` 执行前围栏调用、前端 `views/pent/hitl/` |
| 审计 Agent 复核（audit_agent，失败回退人工） | 已实现，含审计模型配置与决策落库 `pent_approvals` | `hitl.rs::review/audit` |
| YAML 工具配方 | 已实现：`tools/recipes.yaml` 单文件清单 + `import_tool_recipes` 校验导入 + 运行时内存配方表；`executor.program/args` 直接驱动执行（保留 program 前缀白名单、无 shell、Scope/目标校验） | `infrastructure/recipe.rs`、`tools/recipes.yaml`、`src/bin/import_tool_recipes.rs` |
| MCP 集成 | 已实现 stdio + HTTP/SSE JSON-RPC 客户端；内置 MCP Server 由 `McpServerConfig` 按需选择传输层 | `tooling/mcp/{client,protocol,registry,transport}.rs` |
| 动态工具披露 / tool search | 已实现：控制类与证据类工具常驻可见，其余按 `tool_search_min_tools=20`、`tool_search_top_k=24` 动态披露 | `tool_catalog.rs` |
| Skills 生命周期（draft → reviewed → published → disabled） | 已实现管理 API 与状态机，Agent 侧 `skill_list` / `skill_load` 只读已发布 Skill | `pent-server/src/skills.rs`、`pent_skills` 表、`tool_catalog.rs` |
| 知识库检索 | 已实现 `knowledge_search` 与文档存储；RMOS 记忆适配骨架 | `tool_catalog.rs`、`pent_knowledge_documents` 表、`conversation/rmos/` |
| 证据/流量回看工具组 | 已实现 `evidence_list/search/view`、`traffic_list/view` | `tool_catalog.rs` 及相关 Repository |
| 对话式自主循环（FINAL / ASK_USER / NO_PATH / REPLAN、预算、空转抑制、检查点） | 已实现 | `conversation/runtime.rs`、`scan_progress.rs`、`checkpoint.rs`、`plan_board.rs`、`context_compaction.rs` |
| 工具结果治理（兜底、截断、审计字段） | 已实现进程沙箱 + 输出上限 + 审计 | `infrastructure/process/` |
| 弹性工具执行基础设施（execution_id、并发限制、熔断、取消） | 已实现 `ExecutionRegistry` + `CircuitBreaker` + `ElasticExecutor`，并接入 `process::execute_tool` 全局路由 | `infrastructure/execution.rs`、`infrastructure/process.rs`、`services/gateway/src/main.rs` |

### 2.1 本地工具执行链路（当前基线）

以 nmap 为例，当前完整链路：

```text
pent_tool_definitions 表（SQL 初始化快照）
  -> tool_catalog 加载 active 工具定义与 JSON Schema
  -> 模型工具清单：nmap / nmap_scan / tcp_scan
  -> 对话模式命中 has_conversation_handler，重工具提示通过 Run 执行
  -> Run Worker 调用 infrastructure/tool_registry.rs::prepare_tool
  -> lifecycle 白名单解析 /usr/bin/nmap
  -> prlimit + 超时 + 空环境 + 输出上限执行
```

核心原则：**执行权限仍受白名单、Scope 校验与沙箱约束，YAML 配方只是命令模板的编写格式**。
当前实现：`tools/recipes.yaml` 编写配方 → `import_tool_recipes` 校验（program 前缀白名单、
占位符合法性、风险/审批枚举）→ upsert 进 `pent_tool_definitions` → gateway/pent-agent 启动时
加载到内存配方表 → `prepare_tool` 优先按配方渲染命令，未覆盖工具回退硬编码分支。

架构决策（2026-08-11）：**继续半 MCP 化方向，但不把本地 CLI 工具升级为真实 MCP 子进程**
（子进程形态没有安全收益，只会引入进程生命周期、stdio 断管、重连风暴等故障面）。
YAML 配方化已落地（见 D-12）：本地 CLI 工具的命令模板由 `tools/recipes.yaml` 维护并经
`pent_tool_definitions` 驱动执行；后续新增工具只需补配方与导入，不再需要改 `prepare_tool`。

| 工具性质 | 当前通道 | 示例 |
| --- | --- | --- |
| 纯 CLI 工具 | 硬编码 `prepare_tool` + SQL 定义 | nmap、nuclei、ffuf、httpx |
| 有状态或外部维护的工具服务 | 真实 MCP（stdio/HTTP/SSE） | chrome-devtools、未来的 Burp |
| 低延迟确定性逻辑 | builtin Rust handler，不过 MCP | js_analyze、knowledge_search、证据检索 |

## 3. 差距清单

| 差距 | DesRed 现状 | Pent 现状 | 影响 |
| --- | --- | --- | --- |
| 工具配方数量 | `tools/` 95 个 YAML，覆盖扫描/Web/云/二进制/后渗透全链路 | YAML 配方机制已落地（D-12），现有约 60 个 CLI 工具全部配方化；DesRed 95 个配方尚未批量转换 | 配方化与可持续维护机制已就绪，攻击工具面可按需增量扩充 |
| 子代理 profile 数量 | `agents/` 15 个（recon、penetration、privilege-escalation、orchestrator 等） | 3 个 | 多代理编排缺少专业分工 |
| 角色系统（`roles/` 12+ YAML，提示词 + 工具策略预设） | 完整 | 未实现 | 用户无法按场景一键切换测试角色 |
| Workflow 节点图（start/agent/tool/condition/hitl/output/end） | 完整引擎 + 页面 | 未实现（现有 `pent-core/flow` 是状态机，不是可视化编排） | 缺少可复用的固定流程编排入口 |
| MCP 传输 | stdio + HTTP + SSE + 外部联邦 | stdio + HTTP/SSE；外部联邦配置待补充 | 已可接入 HTTP/SSE 形态的远程 MCP 服务；联邦发现机制待补充 |
| 弹性工具执行（阻塞调用 `execution_id` 多轮等待、主动取消、单 server 熔断、并发限制） | 完整 | 基础设施已就位：`ExecutionRegistry` 分配 `execution_id`、按工具名并发限制、按工具熔断、`CancellationToken` 主动取消；`process::execute_tool` 在网关启动后通过 `ElasticExecutor` 全局路由。对话运行时尚未切换到“异步等待 execution_id 不占轮次”协议 | 长耗时工具仍会占住 Agent 轮次，需后续在 `conversation/runtime.rs` 工具调用循环中实现异步等待协议 |
| 知识库 multi-query + 精排 | 完整链路 | 有 `tooling/reranker/` 骨架，链路未闭合 | 召回质量不稳定 |
| 视觉分析（独立视觉模型读截图/验证码） | 已实现 | 未实现 | Web 测试缺少截图理解能力 |
| 批量任务队列 | 完整 | 有任务与 task-monitor 页面，批量编排能力待核实补齐 | 批量目标场景效率低 |
| 工具通道双轨 | 单一 YAML/MCP 通道 | 2 个配方走内部 MCP，其余宿主工具仍走旧直接函数通道，catalog 去重 + 旧名兼容 fallback 并存 | 参数校验、错误格式、审计字段存在分叉风险，迁移不能停在半路 |
| 内部 MCP 网络策略 | 按工具/角色控制 | `registry.rs` 内部调用固定 `NetworkPolicy::Full`，未从 Run 策略快照推导 | 网络出口约束依赖上游 Scope 校验，MCP 层没有独立收口 |

## 4. 明确不移植

以下 DesRed 能力与本平台安全定位冲突或超出范围，不进入 Pent 计划：

- `internal/c2` 内置 C2、`internal/` 下 WebShell、`desshell/`、`gsl5/`；
- 微信/企微/钉钉/飞书/Telegram/Slack/Discord/QQ 机器人接入（`internal/robot`）；
- `plugins/burp-suite`、`plugins/browser-extension`（如确需流量回传，另行评审为 HTTP 流量
  接入，不复制插件代码）；
- `desworkflow/` 攻击脚本集与 `recon/` 供应链脚本（逐案评审后可转为受审 Skill/工具配方，
  不原样引入）；
- SQLite 单体存储、`run.sh`/`upgrade.sh` 部署形态。

## 5. 剩余实施任务

每项任务单独提交，验收标准必须满足后才标记完成。

### P0：补齐攻击工具面

| 编号 | 任务 | 主要输出 | 验收 |
| --- | --- | --- | --- |
| D-01 | ~~工具配方批量导入第一批~~ 改为：基于现有硬编码注册表补齐 Web/扫描工具缺口（ffuf、gobuster、feroxbuster、dirsearch、sqlmap、nikto、httpx、subfinder、amass、dalfox、wafw00f、arjun 等大部分已存在；本次新增 httpx、katana、gau，修复 sslscan 执行路径） | `pent_tool_definitions` 新行 + `prepare_tool` 白名单 + `has_conversation_handler` 登记 + 风险分级 | 新增工具通过 Schema 校验；目标校验、超时、输出上限生效；未安装工具调用返回明确不可用错误；`cargo test` 与 `cargo check --workspace` 通过 |
| D-02 | 子代理 profile 补全 | 从 DesRed `agents/*.md` 转换并安全审查后写入 `agents/`（recon、penetration、vulnerability-triage、privilege-escalation、reporting-remediation 等） | 每个 profile 不含越权/绕过授权表述；`agent_profiles::load` 可加载；多代理 Run 能按角色委派 |
| D-03 | 角色系统决策与实现 | 决策：用“角色 = 提示词预设 + 工具白名单 + HITL 策略”建模，落库表与 API | 角色可创建/绑定项目；切换角色后模型工具面与提示词随动 |
| D-04 | 工具通道双轨收敛 | 全部宿主工具配方化后，删除旧直接函数通道与 `execute_mcp_tool` 中无前缀旧名 fallback；内部 server 工具评估短名豁免以降低 token 开销 | 模型工具清单每个工具只有一个入口；旧路径删除后回归测试通过 |

### P1：执行韧性

| 编号 | 任务 | 主要输出 | 验收 |
| --- | --- | --- | --- |
| D-05 | MCP HTTP/SSE 传输 | `tooling/mcp/transport.rs` 新增 `HttpSseTransport`；`McpServerConfig` 扩展 `url`/`headers` 字段；`McpClient` 通过配置自动选择 stdio 或 HTTP/SSE | 可注册并调用远程 HTTP/SSE MCP server；断线后 `is_alive` 返回 false 并触发错误事件；单元测试通过；`cargo test -p kairos-platform-pent-server` 通过 |
| ~~D-06~~ | ~~弹性工具执行~~ | `infrastructure/execution.rs`：`ExecutionRegistry`（`execution_id`、按工具并发限制、`CancellationToken` 取消、结果等待）、`CircuitBreaker`（按工具失败计数与熔断恢复）、`ElasticExecutor`（组合注册表与熔断器）；`process::execute_tool` 全局路由接入 | `cargo test -p kairos-platform-pent-server infrastructure::execution` 通过；`cargo test -p kairos-platform-pent-server` 通过；`cargo check --workspace` 通过。当前实现提供取消/并发/熔断能力；对话运行时“阻塞工具不占 Agent 轮次”的异步等待协议需后续在 `conversation/runtime.rs` 中接力 |
| ~~D-12~~ | ~~YAML 工具配方加载器~~ | `infrastructure/recipe.rs`（YAML 解析/校验/模板渲染/全局配方表）、`tools/recipes.yaml`（现有约 60 个工具全覆盖）、`src/bin/import_tool_recipes.rs`（校验 + upsert `pent_tool_definitions`）；`prepare_tool` 优先按配方渲染，gateway/pent-agent 启动时从库加载配方表 | `import_tool_recipes --check` 校验 66 条配方通过；`cargo test -p kairos-platform-pent-server` 通过；`script/test-database-migrations.sh` 通过；`cargo check --workspace` 通过。安全边界：无 shell、program 前缀白名单、Scope/目标校验不变；`http_request` 等有状态工具保留内置 handler |
| D-07 | 知识库检索闭环 | multi-query 改写 + reranker 接通 + 检索日志 | 召回结果可追溯来源文档；检索日志可查询 |
| D-08 | 内部 MCP 网络策略收紧 | `registry.rs` 内部调用从 Run 策略快照推导 `NetworkPolicy`，替代固定 `Full` | 策略快照禁止网络时内部 MCP 工具调用被拒绝并留审计 |

### P2：编排与体验（先做决策评审再动工）

| 编号 | 任务 | 主要输出 | 验收 |
| --- | --- | --- | --- |
| D-09 | Workflow 节点图评审 | 决策文档：复用 `pent-core/flow` 扩展条件/审批节点，还是新建编排引擎 | 评审通过后再排实施任务 |
| D-10 | 视觉分析 | 独立视觉模型适配器，对话中只保留文字摘要 | 截图分析结果入 Evidence，不污染主模型上下文预算 |
| D-11 | 批量任务编排 | 批量目标入队、并发配额、结果汇总 | 批量任务与单目标 Run 共用同一套执行与审批链路 |

## 6. 接力规则

在 `pent-agent-service-plan.md` 第 24 节规则之上补充：

1. 开始任何 D-xx 任务前先读本文第 2 节，确认不要重建已有机制。
2. 从 DesRed 转换任何 prompt / 配方 / profile 时必须做安全审查：删除“跳过授权、不限
   工具、无限制绕过”类表述；执行边界以 Pent 白名单和 Scope 校验为准。
3. DesRed 的 SQLite 结构、配置字段名只作语义参考；存储落 `pent_*` 表，配置落
   `pent_runtime_policies` / 项目策略，不引入第二套配置中心。
4. 数据库变化仍按 AGENTS.md 流程：改参考库 → 重导出 `sql/postgresql/0001_initial.sql`
   → 跑快照恢复测试。
5. 每项任务交付说明包含：修改文件、数据库变化、API 契约、权限码、测试结果、残余风险、
   下一项可接力任务，并同步更新本文第 2、3 节的状态。
