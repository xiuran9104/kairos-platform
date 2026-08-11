# Pent Agent Web 微服务迁移方案

Pent 智能渗透平台的完整产品页面、能力分层、领域模块和分阶段建设范围见
[ Pent 智能渗透平台实现方案](./ Pent智能渗透平台实现方案.md)。

Pent 模块页面、组件、交互、路由、API 映射和 UI 验收标准见
[Pent 模块 UI 实现方案](./Pent模块UI实现方案.md)。

工具注册、混合检索、ToolOrchestrator 和多执行后端的具体任务拆分见
[ Pent 成熟工具路由框架实施任务书](./ Pent成熟工具路由框架实施任务书.md)。

结构化对话、上下文、模型路由、Run 指令与 SSE 的具体任务拆分见
[ Pent 成熟 LLM 对话运行时实施任务书](./ Pent成熟LLM对话运行时实施任务书.md)。

分层规划、PreFlect、动态重规划、多级反思和元认知控制的具体任务拆分见
[ Pent H-PER-M 认知运行时实施任务书](./ Pent-H-PER-M认知运行时实施任务书.md)。

与 DesRedTeam 的能力对照、已移植机制清单、剩余任务（D-xx）和移植边界见
[Pent 与 DesRedTeam 对齐实施计划](./Pent-DesRed对齐实施计划.md)。

## 1. 背景与结论

`/home/xiuran/PentAGI` 当前是一个本地优先的 Rust 安全智能工作台，主要入口是
CLI/TUI。核心能力包括 Agent 运行时、Flow、工具执行、权限审批、Run、Evidence、
Toolcall、知识检索和本地 Worker，并已定义 `UiCommand`、`UiEvent` 等 UI 协议。

Kairos Platform 已具备 Web 管理后台、用户与角色权限、租户、资产管理、扫描任务、
PostgreSQL、Redis、NATS 和 MinIO 等平台能力。将 Pent Workbench 接入 Kairos Platform
后，可以形成多人使用、后台执行、实时观察、人工审批和证据审计的完整闭环。

本方案决定：

- 在 Kairos Platform 中新增独立的 `pent-agent` 微服务。
- 迁移并复用 Pent Workbench 的核心执行能力，不在 Gateway 中复制 Agent 业务逻辑。
- Web 成为主要生产入口，CLI 保留为开发、单机和故障排查入口。
- Gateway 继续统一负责用户认证、租户上下文、权限校验和外部 API。
- Pent Agent 负责 Agent 会话、任务编排、工具执行、运行事件和执行期权限控制。

这不是把现有轻量 HTTP Server 原样搬入 Kairos Platform，也不是删除 CLI 后重写全部功能。

## 2. 建设目标

第一阶段需要打通以下业务闭环：

```text
选择项目与授权资产
  -> 创建安全任务
  -> Agent 生成执行计划
  -> 后台执行工具
  -> Web 实时展示事件
  -> 高风险操作人工审批
  -> 汇总证据与漏洞
  -> 生成可审计的运行结果
```

具体目标：

1. 用户关闭浏览器后，任务仍在服务端继续执行。
2. 每次运行都关联租户、用户、项目、资产范围和权限策略。
3. Web 可以实时展示 Agent 输出、子任务状态、工具调用和错误。
4. 需要确认的工具操作必须等待授权用户审批。
5. 证据、漏洞、HTTP 流量和报告可以检索、下载和追溯。
6. Worker 可以独立部署和扩容，任务不能被多个 Worker 重复执行。
7. CLI 与 Web 服务尽量共用同一套核心运行时和协议类型。

## 3. 非目标

首期不包含：

- 将所有本地知识库能力一次性改造成分布式知识图谱平台。
- 在浏览器中直接执行 Shell 或扫描工具。
- 让 Pent Agent 维护独立的用户、角色、租户和资产系统。
- 直接暴露 Pent Agent 内部端口给公网。
- 完整复制上游 `vxcontrol/pentagi` 的全部部署和可观测性组件。
- 首期实现多个 Agent 服务跨节点共同执行同一个 Run。

## 4. 总体架构

```text
Browser / Vben Admin
          |
          | JWT / SSE
          v
services/gateway
  - 用户认证与 RBAC
  - 租户和数据范围
  - 对外 REST/SSE API
  - 请求审计
          |
          | 内部服务凭证 + 用户执行上下文
          v
services/pent-agent
  - Agent 会话与运行
  - Flow 计划与调度
  - 工具注册和执行
  - 执行期权限审批
  - 事件发布
          |
          +----------------+----------------+----------------+
          |                |                |                |
     PostgreSQL        NATS/Redis         MinIO        外部安全工具
     元数据与状态       任务与事件        大型产物      受控子进程/容器
```

外部请求统一经过 Gateway。Gateway 可以代理普通 REST 请求；实时事件使用支持断线重连
的 SSE。需要双向低延迟交互时再引入 WebSocket，首期审批和取消操作仍使用普通 HTTP。

## 5. 建议目录结构

遵循 Kairos Platform 现有的 `api + server` 模块边界，并把可复用运行时单独保留：

```text
crates/
  modules/
    pent-api/
      src/
        commands.rs       # 创建任务、取消、审批等命令 DTO
        events.rs         # Run 和 Agent 事件契约
        models.rs         # 对外查询 DTO
        permissions.rs    # 权限码和风险级别契约
        lib.rs
    pent-core/
      src/
        runtime/          # Agent 循环、会话、上下文
        flow/             # Flow 和 Subtask 状态机
        tools/            # 工具注册与执行
        evidence/         # 证据和漏洞领域逻辑
        permissions/      # 执行期策略和审批状态机
        protocol/         # 与 UI 无关的运行协议
        lib.rs
    pent-server/
      src/
        application/      # 创建 Run、执行、取消、审批用例
        transport/
          http/           # 内部 REST/SSE
          mq/             # NATS 命令和事件
        infrastructure/
          postgres/       # Repository 实现
          object_store/   # MinIO 产物适配
          process/        # 受控工具执行适配
        config.rs
        lib.rs
services/
  pent-agent/
    src/main.rs           # 配置加载、依赖装配和服务启动
```

依赖方向：

```text
services/pent-agent -> pent-server -> pent-core
                              |
                              +-------> pent-api

services/gateway ------------> pent-api
pent-core 不依赖 gateway、Axum、SQLx、NATS 或 MinIO
```

`pent-api` 只存放稳定契约；`pent-core` 只负责领域与运行逻辑；传输、数据库、消息队列
和对象存储实现归 `pent-server`。这样 CLI 仍可直接复用 `pent-core`，不会依赖 Web 服务。

## 6. 与现有 PentAGI 的迁移关系

| 现有能力 | 迁移目标 | 处理方式 |
|---|---|---|
| Agent runtime/session | `pent-core/runtime` | 保留核心逻辑，移除 CLI 配置耦合 |
| `UiCommand`/`UiEvent` | `pent-api` 与 `pent-core/protocol` | 稳定化并增加版本字段 |
| Flow/Subtask | `pent-core/flow` | 保留状态机，持久化改为 Repository |
| Run/Toolcall/Evidence | `pent-core` + PostgreSQL/MinIO | 领域逻辑与存储实现分离 |
| 本地 WorkerQueue | `pent-server` | 改用 PostgreSQL 租约和 NATS 通知 |
| JSONL/SQLite 状态 | 导入工具 | 只用于迁移和本地 CLI，不作为生产主存储 |
| 手写 `TcpListener` HTTP | Axum transport | 不直接迁移 |
| 内嵌 Evidence HTML | Vben 页面 | API 保留，页面重新实现 |
| CLI/TUI | 原仓库或后续 workspace binary | 保留，不作为生产调度器 |

首期不建议让 Kairos Platform 直接通过路径依赖
`/home/xiuran/PentAGI/crates/pentagi-core`。开发阶段可以用 Git 分支对照迁移，最终代码应进入
Kairos Platform workspace，避免部署依赖开发机目录。

## 7. 领域模型与存储

建议新增以下主要表。实际建表时先更新维护中的参考数据库，再重新导出并验证
`sql/postgresql/0001_initial.sql` 全量初始化快照；已有环境另行提供经过评审的升级步骤。

### 7.1 核心表

- `pent_projects`：安全项目与 Kairos Platform 业务项目/资产范围的关联。
- `pent_flows`：任务计划、版本、状态和策略快照。
- `pent_subtasks`：子任务顺序、工具、参数、状态、重试次数。
- `pent_runs`：一次执行实例、触发人、Worker、开始和结束时间。
- `pent_run_events`：可重放的有序运行事件。
- `pent_agent_sessions`：对话驱动的 Agent 会话、目标、预算和当前 Run。
- `pent_conversations`：会话中的对话线程和上下文摘要。
- `pent_conversation_messages`：用户、Agent 和系统消息及结构化消息类型。
- `pent_run_instructions`：运行中追加的调整、暂停、恢复、复核和总结指令。
- `pent_scope_proposals`：从自然语言提取的目标范围、约束和确认状态。
- `pent_tool_calls`：工具参数摘要、权限决策、耗时和执行结果。
- `pent_approvals`：审批请求、风险、审批人、决定和过期时间。
- `pent_evidence`：证据索引、哈希、来源和对象存储引用。
- `pent_findings`：漏洞标题、严重性、状态、目标和证据关系。
- `pent_artifacts`：MinIO 对象键、大小、SHA-256 和内容类型。

所有业务表至少包含：

- `tenant_id`
- `created_by`
- `created_at`
- `updated_at`

需要按项目隔离的数据还必须包含 `project_id`。任何查询都不能只依赖前端传入
`tenant_id`，租户和用户上下文必须来自 Gateway 验证后的可信上下文。

### 7.2 大小数据分离

- PostgreSQL 保存状态、索引、摘要、事件和关联关系。
- MinIO 保存完整 HTTP Body、扫描原始输出、截图、报告和大型日志。
- 数据库存储对象键和 SHA-256，不存储任意绝对文件路径。
- Agent 返回给前端的产物地址必须是短期签名 URL 或经过 Gateway 鉴权的下载接口。

## 8. API 草案

对外路径统一由 Gateway 暴露：

```text
POST   /pent/projects
GET    /pent/projects
GET    /pent/projects/:id

POST   /pent/sessions
GET    /pent/sessions/:id
POST   /pent/sessions/:id/messages
GET    /pent/sessions/:id/messages
POST   /pent/sessions/:id/scope/propose
POST   /pent/sessions/:id/scope/confirm
POST   /pent/sessions/:id/runs
GET    /pent/sessions/:id/events/stream

POST   /pent/tasks
GET    /pent/tasks
GET    /pent/tasks/:id
POST   /pent/tasks/:id/cancel

POST   /pent/flows/plan
GET    /pent/flows/:id

POST   /pent/runs
GET    /pent/runs
GET    /pent/runs/:id
POST   /pent/runs/:id/cancel
POST   /pent/runs/:id/instructions
GET    /pent/runs/:id/events
GET    /pent/runs/:id/events/stream

GET    /pent/runs/:id/tool-calls
GET    /pent/runs/:id/evidence
GET    /pent/runs/:id/findings
GET    /pent/runs/:id/artifacts

GET    /pent/approvals
POST   /pent/approvals/:id/decision

GET    /pent/tools
GET    /pent/tool-profiles
```

创建任务时不能直接接受任意可执行命令。请求只允许引用服务端工具目录中的工具和配置，
工具参数必须由对应 Schema 校验。

## 9. 实时事件协议

事件使用单调递增的 `sequence`，写入 `pent_run_events` 后再发布。SSE 客户端通过
`Last-Event-ID` 恢复遗漏事件。

基础事件类型：

```text
run.created
run.started
run.cancel_requested
run.cancelled
run.completed
run.failed
session.created
conversation.user_message
conversation.agent_message.delta
conversation.agent_message.completed
scope.proposed
scope.confirmed
scope.rejected
run.instruction.received
run.instruction.applied
run.instruction.rejected
flow.planned
planner.feedback
subtask.started
subtask.completed
subtask.failed
agent.message.delta
agent.message.completed
tool.requested
tool.started
tool.completed
tool.failed
tool.result.normalized
approval.requested
approval.decided
evidence.created
finding.created
artifact.created
```

事件载荷包含：

- `event_id`
- `run_id`
- `sequence`
- `event_type`
- `occurred_at`
- `payload`
- `schema_version`

事件必须先持久化再推送，NATS 只承担通知和分发，不能成为唯一事实来源。

`tool.result.normalized` 的 `payload` 使用 `pent-api::ToolExecutionResult` 稳定契约，
包含状态、摘要、结构化观察、发现、产物引用、退出码、耗时和警告。Worker 必须先经过
`pent-core` 清洗器移除控制字符、限制长度并脱敏凭据，再把结果交给规划层。规划层不得
直接解析异构工具 stdout；原始大输出只保存对象存储引用。

基础执行器采用显式允许列表和绝对程序路径，不经过 Shell。每次调用校验目标必须等于
Scope Snapshot 中的授权目标，并设置超时终止、空环境和输出上限。默认执行后端为
Docker；目标网络必须通过 `PENTA_DOCKER_TARGET_NETWORK` 指向由部署方配置了出口策略的
专用网络，未配置时网络工具失败关闭。生产构建不支持宿主机 `prlimit` 直连后端；
`PENTA_TOOL_EXECUTOR=host` 仅供 Rust 单元测试使用，部署配置不得依赖该模式。

生产环境应配置 `PENTA_EGRESS_POLICY_COMMAND` 为出口策略控制器的绝对路径。Worker 会以
`prepare <run_id> <target>` 调用它，并要求返回
`{"network_name":"...","policy_id":"..."}`；Run 结束或中断后以
`release <policy_id>` 清理策略。控制器负责创建只允许访问该目标的 Docker 网络和出口
ACL。未配置控制器时仅允许使用预先配置的静态 `PENTA_DOCKER_TARGET_NETWORK`。

工具镜像不得使用 `latest`。生产默认要求 digest，通过
`PENTA_TOOL_IMAGE_TCP_SCAN`、`PENTA_TOOL_IMAGE_HTTP_PROBE`、
`PENTA_TOOL_IMAGE_HTTP_REQUEST`、
`PENTA_TOOL_IMAGE_NUCLEI`、`PENTA_TOOL_IMAGE_AFROG`、
`PENTA_TOOL_IMAGE_DNS_LOOKUP`、`PENTA_TOOL_IMAGE_SSH_EXEC` 和
`PENTA_TOOL_IMAGE_SQL_QUERY` 配置 `name@sha256:...`。仅本地开发可以设置
`PENTA_REQUIRE_IMAGE_DIGEST=false` 使用固定版本标签。

对话 HTTP 工具同样通过 Docker 隔离执行器运行；其他尚未容器化的对话网络工具不会进入
模型工具目录。HTTP Cookie Jar 与 CSRF 状态保存在 `PENTA_TOOL_STATE_ROOT` 下的会话
隔离目录，并只读写挂载到对应工具容器。需要完整网络访问时还必须显式设置
`PENTA_ALLOW_FULL_NETWORK=true` 和 `PENTA_DOCKER_FULL_NETWORK`。

`PENTA_ENV=development` 时可设置 `PENTA_ALLOW_DIRECT_NETWORK_TOOLS=true`，允许对
本机隔离靶场进行直连开发测试。需要跳过中高风险审批时，还可设置
`PENTA_AUTO_APPROVE_ALL_TOOLS=true`；每次自动批准仍写入 Run 事件审计。生产部署必须将
`PENTA_ENV` 设为 `production`（或不设置），此时上述两个开发开关均无效。

首批解析器覆盖 Nmap/TCP 扫描、Nuclei、Afrog 和 FFUF。标准结果随后生成
`planner.feedback`，其中包含 `continue`、`replan` 或 `stop` 指令、原因与建议动作。
任何下一步工具选择仍须重新经过 Scope Validator 和权限策略，规划反馈本身不能绕过
审批执行工具。

已实现的首条规划循环为 `tcp_scan -> 端口观察 -> http_probe -> 新规划反馈`。只有
标准化端口观察包含 80/443 时才会追加 HTTP 探测；由授权主机派生的 HTTP/HTTPS URL
必须再次通过 Scope Validator。规划器设置最多 3 个已完成工具步骤的硬上限，防止循环
失控。

## 10. 对话驱动与自主运行模型

### 10.1 产品原则

Pent 采用“对话优先、后台自主、按需干预”的交互模型：

```text
用户用自然语言描述测试目标
  -> 服务端提取目标、范围、约束和预算
  -> 用户结构化确认授权范围
  -> 创建 Agent Session 和 Run
  -> Agent 在后台自主规划和执行
  -> 用户可以离开页面
  -> 需要时追加指令或询问
  -> 高风险或范围不明确时暂停等待处理
```

对话是启动、解释和调整入口，不是 Agent 继续运行的必要条件。浏览器断开、用户离开
页面或 CLI 退出后，Run 必须继续由服务端 Worker 执行。

### 10.2 会话与运行边界

`AgentSession` 保存相对长期的协作上下文：

```text
AgentSession
  session_id
  tenant_id
  project_id
  objective
  constraints
  budget
  scope_proposal
  confirmed_scope_snapshot
  conversation_id
  active_run_id
  status
```

`Run` 保存一次确定执行的不可变安全边界和运行状态：

```text
Run
  run_id
  session_id
  objective_snapshot
  scope_snapshot
  policy_snapshot
  budget_snapshot
  plan
  state
  events
  tool_calls
  approvals
  evidence
  findings
  usage
```

一个会话可以产生多个 Run；历史 Run 的范围和策略快照不能被后续对话修改。

### 10.3 消息语义

消息不能只保存自由文本，服务端需要将消息归一为结构化类型：

```text
UserMessage
  ask
  provide_context
  steer
  request_review
  request_summary
  pause
  resume
  stop

AgentMessage
  answer
  plan_update
  observation
  decision
  warning
  approval_request
  summary
```

所有用户指令先写入数据库，再发布给 Worker。Worker 使用稳定的 `instruction_id`
幂等处理，并产生 applied 或 rejected 事件。

### 10.4 目标与范围提取

用户可以输入：

```text
对 test.example.com 做一次授权安全评估，重点检查登录、越权和文件上传，
不要修改生产数据，最多运行两小时。
```

模型只负责提取候选结构：

- 测试目标；
- 域名、IP、CIDR、URL 和端口；
- 测试重点；
- 禁止操作；
- 时间、并发和 Token 预算；
- 是否提供测试账号；
- 需要审批的风险级别。

候选结构必须经过确定性 Schema 校验和项目授权数据校验，并由用户明确确认后才能创建
Run。LLM 的解析结果不能直接成为网络访问许可。

### 10.5 运行中动态干预

运行中的消息可以：

- 调整测试重点和步骤优先级；
- 补充业务背景或测试账号引用；
- 跳过某个已授权目标；
- 要求复核某个漏洞；
- 请求阶段性总结；
- 暂停、恢复或停止 Run。

运行中扩大 IP、域名、端口或操作权限，不能通过普通消息直接生效。此类请求必须生成新
Scope Proposal 或 Policy Proposal，重新校验并明确确认，随后产生新 Run 或受控策略
版本变更。

### 10.6 对话、审批与硬限制

三者职责不同：

- 对话：表达目标、补充上下文、解释和调整方向；
- 审批：对某一次具体高风险操作作出明确决定；
- 硬限制：由确定性代码和隔离环境强制，不可通过对话关闭。

用户在对话中输入“允许删除”不能等同于批准某个删除请求。审批必须包含工具、目标、
参数摘要、影响、风险、有效期和审批人，并通过独立审批 API 处理。

文件大小、内容类型、路径穿越、恶意压缩包、存储配额、目标范围、网络隔离、资源上限、
凭据保护和审计等平台限制始终生效。Agent 的自主性只存在于这些边界以内。

### 10.7 Web 与 CLI 共用运行时

Web 和 CLI 都是 Pent 服务客户端，不能各自维护一套 Agent 循环：

```text
Web UI --\
          -> Gateway/ Pent API -> Agent Runtime -> Worker
CLI -----/
```

CLI 至少支持：

```text
penta session create
penta session chat <session-id>
penta agent start <session-id>
penta run watch <run-id>
penta run pause <run-id>
penta run resume <run-id>
penta run cancel <run-id>
penta approval decide <approval-id>
penta evidence export <run-id>
```

Web 与 CLI 的消息、事件、审批和状态必须完全一致。

## 11. 任务执行与并发控制

推荐执行流程：

1. Gateway 校验用户权限、租户和资产数据范围。
2. Pent Agent 在同一事务内创建任务、Run 和首个事件。
3. 事务提交后发布 `penta.run.requested`。
4. Worker 使用数据库租约领取 Run，记录 `worker_id` 和 `lease_expires_at`。
5. Worker 执行 Flow，每个状态变化先写数据库再发布事件。
6. Worker 定期续租；租约过期后任务可以被安全恢复。
7. 取消操作写入取消标记并发布通知，Worker 在安全检查点终止子进程。
8. 服务重启后扫描 `queued/running` 且租约失效的 Run，进行恢复或标记失败。

需要使用幂等键避免重复创建任务；每个工具调用使用稳定的 `tool_call_id`。对于不能安全
重试的外部操作，状态必须进入人工确认，而不是自动再次执行。

## 12. 权限与安全设计

### 12.1 平台权限

建议权限码：

```text
pent:project:read
pent:project:create
pent:task:create
pent:task:read
pent:task:cancel
pent:run:read
pent:run:execute
pent:approval:read
pent:approval:decide
pent:evidence:read
pent:artifact:download
pent:tool:manage
```

Gateway 负责验证这些平台权限和数据范围。 Pent Agent 不能信任浏览器直接传递的角色、
权限或租户字段。

### 12.2 执行期权限

平台 RBAC 与 Agent 工具权限是两层不同的控制：

- RBAC 决定用户能否创建、查看、取消或审批任务。
- 工具策略决定某个 Run 是否能进行网络访问、写文件或执行外部程序。

每个 Run 创建时保存不可变的策略快照，包括：

- 授权目标和网段。
- 允许、询问和禁止的工具。
- 最大并发、超时、重试和资源限制。
- 发起人及其数据范围。
- 审批规则和策略版本。

即使用户拥有 `pent:run:execute`，Agent 也不能越过该 Run 的目标范围和工具策略。

### 12.3 服务和执行隔离

- Pent Agent 只监听内网地址，使用服务身份验证 Gateway 和 Worker。
- 外部工具优先在受限容器中运行，限制 CPU、内存、进程、网络和挂载目录。
- 禁止将宿主机 Docker Socket 直接交给 Agent。
- API Key 通过服务端密钥管理注入，不写入事件、Toolcall 或模型上下文。
- 日志和证据写入前过滤 Authorization、Cookie、Token 和常见密钥字段。
- 工具参数、审批结果和产物哈希进入审计记录。
- 所有主动安全测试必须绑定经过授权的资产范围。

## 13. 前端页面

建议在 Vben 中新增 `views/pent`：

```text
views/pent/
  dashboard/       # 运行数、失败率、风险和待审批统计
  agent-start/     # 自然语言目标输入、范围提取和授权确认
  agent-session/   # Agent 协作对话和会话历史
  projects/        # 项目与资产范围
  tasks/           # 任务列表和创建入口
  runs/            # Run 列表
  run-detail/      # 实时时间线、Agent 输出和子任务
  approvals/       # 待审批操作
  evidence/        # 证据和 HTTP 流量
  findings/        # 漏洞列表与详情
  settings/        # 工具目录、模型和策略
```

Run 详情页是首期核心页面，至少包含：

- 运行状态、目标、发起人和策略摘要。
- Flow/Subtask 进度。
- Agent 协作对话、消息状态和工具调用时间线。
- 用户追加指令、请求复核、暂停、恢复和停止入口。
- 实时审批卡片。
- Evidence、Finding 和 Artifact 标签页。
- 取消任务和重新连接事件流的能力。

## 14. 可观测性

所有服务之间传递：

- `request_id`
- `trace_id`
- `tenant_id`
- `user_id`
- `project_id`
- `run_id`
- `tool_call_id`

指标至少包括：

- 排队、运行、等待审批、成功和失败的 Run 数。
- Run 和 Subtask 执行时长。
- 工具成功率、超时率和重试次数。
- 等待审批时间。
- SSE 当前连接数和重连次数。
- Worker 租约丢失次数。
- 模型 Token 和费用用量。

## 15. 分阶段实施计划

### 阶段 0：契约冻结与代码梳理

- 盘点 PentAGI 可迁移模块及其 CLI、文件系统耦合。
- 冻结首版 `Command`、`Event`、Run、Flow、Toolcall 和 Evidence 契约。
- 建立 `pent-api`、`pent-core`、`pent-server` 和服务入口骨架。

验收：workspace 编译通过，依赖方向符合本方案，核心层不依赖传输与存储实现。

### 阶段 1：最小后台执行闭环

- 创建项目、任务和 Run。
- 创建 Agent Session，通过消息提取目标并确认 Scope Proposal。
- 用户离线后 Run 继续执行，重新连接后恢复消息和事件。
- PostgreSQL 状态存储。
- 单 Worker 领取、执行和取消。
- Gateway 鉴权与基础权限码。
- Run 列表、详情和轮询展示。

验收：用户可以从 Web 创建一个只读安全任务，在关闭页面后任务继续运行，并能重新查看结果。

### 阶段 2：实时事件与人工审批

- 持久化事件和 SSE。
- Agent 消息、Subtask、工具调用实时展示。
- 高风险工具审批与超时处理。
- 断线恢复和取消传播。

验收：事件顺序稳定，刷新页面不丢事件；未经审批的受控工具不会执行。

### 阶段 3：证据、漏洞与产物

- Evidence、Finding 和 Artifact 数据模型。
- MinIO 产物存储。
- HTTP 流量详情、漏洞详情和报告下载页面。
- 敏感字段过滤和内容哈希校验。

验收：任一漏洞可以追溯到 Run、Subtask、Toolcall 和原始证据。

### 阶段 4：可靠性与扩容

- NATS 任务通知和事件广播。
- 多 Worker 数据库租约、续租、恢复和幂等。
- 资源限制、容器隔离、指标和告警。
- 失败重试与不可重试工具策略。

验收：Worker 异常退出后任务不会被并发重复执行，符合策略的任务可以恢复。

### 阶段 5：知识与高级能力

- 迁移本地 Wiki、向量记忆和知识回调。
- 接入 pgvector/Graphiti 等可选后端。
- 多 Agent 角色、模型路由和成本控制。

该阶段不阻塞前四阶段的平台闭环。

## 16. 测试策略

- `pent-core`：Flow 状态机、权限策略、取消、重试和事件生成单元测试。
- `pent-server`：Repository、租约、幂等、MinIO 引用和敏感信息过滤集成测试。
- Gateway：JWT、租户隔离、RBAC、数据范围和服务身份测试。
- SSE：顺序、重连、重复事件和慢客户端测试。
- 对话：消息幂等、离线续跑、指令应用/拒绝和范围变更重新确认测试。
- CLI/Web 一致性：同一 Session 的消息、审批和 Run 状态一致。
- 端到端：创建任务、等待审批、同意/拒绝、完成、查看证据。
- 安全测试：越权访问、目标范围逃逸、路径穿越、命令注入和密钥泄漏。
- 故障测试：Worker 退出、NATS 不可用、MinIO 不可用和数据库短暂断开。

数据库变更后必须重新导出并验证全量初始化快照：

```bash
bash script/export-database-bootstrap.sh
bash script/test-database-migrations.sh
cargo test --workspace
pnpm --dir apps/web --filter @vben/web-antd run typecheck
```

前端完成后还应执行生产构建检查。

## 17. 完成标准

Pent Agent Web 微服务可以视为首版完成，必须同时满足：

1. 服务作为独立进程部署，Gateway 中没有 Agent 执行循环。
2. 用户、租户、项目和资产范围贯穿任务的完整生命周期。
3. Run 在浏览器断开后继续执行，并支持取消和服务重启后的确定性处理。
4. 实时事件可持久化、可重放、可断线恢复。
5. 高风险工具操作必须经过策略允许或人工审批。
6. Evidence、Finding 和 Artifact 能追溯到对应 Run 与 Toolcall。
7. 大型产物进入 MinIO，数据库中不存在不受控的本地绝对路径。
8. CLI 和 Web 服务复用核心运行时，没有两套独立的 Agent 逻辑。
9. 通过后端测试、数据库迁移测试、前端类型检查和关键端到端测试。
10. 用户可以通过自然语言创建 Session，确认授权范围后启动 Run。
11. 用户不保持对话或关闭浏览器时，Run 仍能继续执行。
12. 对话指令不能绕过授权范围、审批和平台硬限制。
13. Web 与 CLI 共用同一消息、事件和 Agent Runtime。

## 18. 主要风险与应对

| 风险 | 应对 |
|---|---|
| 直接复制本地状态代码导致多实例数据竞争 | 先抽象 Repository，再替换为 PostgreSQL 实现 |
| Gateway 逐渐承载 Agent 业务 | 只在 Gateway 保留认证、授权、代理和审计 |
| NATS 消息丢失造成状态不一致 | 数据库作为事实来源，消息只用于通知 |
| Agent 越过用户资产范围 | Run 保存不可变目标范围，所有网络工具统一校验 |
| SSE 断线导致页面状态缺失 | 持久化序号事件并支持 `Last-Event-ID` |
| 外部工具拖垮宿主机 | 容器隔离、资源限额、超时和并发限制 |
| CLI 与 Web 分叉 | 两种入口共同依赖 `pent-core` 和稳定协议 |
| 首期范围过大 | 优先完成任务、Run、审批、证据闭环，知识平台后置 |

## 19. 产品 UI 现状审计

截至当前版本，Pent 已具备 Agent 会话、项目、任务、Run、审批、证据、漏洞和工具目录
等基础页面，但距离完整安全智能平台仍有明显差距。现状不能按“菜单存在”判定完成，
必须同时验证真实 API、持久化数据、权限、审计和端到端行为。

| 产品域 | 当前基础 | 尚未完成 |
|---|---|---|
| 安全仪表盘 | 项目、任务、Run、审批、Session 计数 | 漏洞等级、工具排行、Token、资产风险、组件健康、实时活动流 |
| 资产中心 | 列表/卡片、IP、网络区域、端口、人工权重 | 九页签档案、URL/语言/指纹/漏洞聚合、自动风险评分、报告 |
| 漏洞中心 | Finding 列表和基础详情 | 生命周期、批量处置、证据链、LLM 研判、复测 |
| LLM 流量 | 可导航页面和前端 API 契约 | 采集、存储、全文检索、报文详情、脱敏 |
| RMOS | 可导航页面、运行时 memory adapter 骨架 | 三层记忆持久化、向量召回、攻击图谱、路径回放和反馈闭环 |
| 工具生态 | 工具注册 API、基础目录 | 汇总指标、健康列表、依赖检查、分类、调用排行 |
| 拦截中心 | 审批状态机和页面骨架 | 静态规则、语义规则、拦截记录、规则管理 |
| 工作区 | 页面和 API 契约 | 项目文件树、预览、配额、归档、下载和安全清理 |
| 平台设置 | 页面和 API 契约 | 模型服务商、角色绑定、运行策略、成本和密钥管理 |

当前资产页中的 `weight` 是人工资产权重，不得继续作为自动风险评分使用。后续应新增
独立的 `risk_score` 和 `risk_breakdown`，并记录评分规则版本、计算时间和输入证据。

## 20. 已搭建的前端基础框架

以下框架已经存在，后续 AI 应在原目录内继续实现，不再创建第二套同义页面：

```text
views/pent/
  assets/
    index.vue               # 复用资产列表入口
    detail.vue              # 九页签资产档案
  traffic/index.vue         # LLM 流量日志
  rmos/index.vue            # RMOS 记忆与攻击路径
  tools/index.vue           # 工具生态
  guard/index.vue           # 拦截中心
  workspace/index.vue       # 工作区文件
  platform-settings/index.vue
  components/foundation-page.vue

api/pent/
  assets.ts
  traffic.ts
  rmos.ts
  tools.ts
  guard.ts
  workspace.ts
  settings.ts
```

资产详情固定使用以下九个页签，不随实现过程拆成互不关联的独立资产系统：

1. 概览
2. 报告
3. 漏洞
4. 端点
5. 指纹
6. 原始发现
7. 时间线
8. 攻击图谱
9. 历史路径

数据库菜单由 `system_menu` 提供，并包含在
`sql/postgresql/0001_initial.sql`。新增页面完成后必须同步检查组件定位符、
菜单权限和动态路由，不能只修改本地 `router/routes/modules/pent.ts`。

## 21. 目标产品信息架构

```text
安全智能
├─ 安全仪表盘
├─ Agent 工作台
│  ├─ Agent 对话
│  ├─ 项目
│  ├─ 任务
│  ├─ 运行
│  └─ 审批
├─ 资产中心
│  ├─ 资产列表
│  └─ 资产详情（隐藏动态路由）
├─ 漏洞与证据
│  ├─ 漏洞
│  └─ 证据
├─ 流量与审计
│  ├─ LLM 流量
│  └─ 拦截中心
├─ RMOS 记忆
├─ 工具生态
├─ 工作区文件
└─ 平台设置
```

Asset Ops 继续负责机房、云平台、服务商、工单和基础资源台账；Pent 资产中心负责从攻击
视角聚合资产、端点、漏洞、证据、攻击路径和报告。两者引用同一资产主键，不复制资产
主表。

## 22. 后端领域与存储补全方案

### 22.1 资产安全档案

责任目录：

- 契约：`crates/modules/pent-api/src/assets.rs`
- 应用逻辑：`crates/modules/pent-server/src/assets/`
- 前端：`views/pent/assets/`、`api/pent/assets.ts`

建议新增：

- `pent_asset_profiles`：资产风险评分、业务标签、边界类型、聚合时间。
- `pent_asset_endpoints`：归一化 URL、Method、参数签名、来源和去重哈希。
- `pent_asset_fingerprints`：服务、框架、语言、中间件、版本和置信度。
- `pent_asset_timeline`：发现、扫描、验证、处置、复测和报告事件。
- `pent_attack_path_nodes`、`pent_attack_path_edges`：关系图投影与外部图数据库同步状态。

风险评分必须由确定性代码计算，LLM 只能提供分类和置信度建议。建议评分输入包括漏洞
严重度、可利用性、暴露面、业务权重、凭据风险、攻击路径位置和证据置信度。

### 22.2 LLM 流量与全链路审计

责任目录：

- 契约：`pent-api/src/traffic.rs`
- 采集与脱敏：`pent-server/src/traffic/`
- 前端：`views/pent/traffic/`

建议表：

- `pent_http_traffic`
- `pent_http_traffic_bodies`
- `pent_llm_usage_ledger`

大响应体写入 MinIO，数据库保存摘要、对象键、哈希和可检索文本。入库前统一去除
Authorization、Cookie、API Key、密码和令牌。每条记录必须关联 tenant/project/session/
run/subtask/tool_call，并标记业务流量或授权测试流量。平台专属 UA 建议为
`Kairos-Pent-Agent/<version>`。

### 22.3 RMOS

RMOS 是 Runtime Memory Operating System，不再使用 MemOS 命名。三层模型：

- 语义记忆：目标特征、漏洞描述、工具经验，进入向量索引。
- 程序记忆：成功/失败攻击步骤、前置条件、工具序列，进入 PostgreSQL 与图投影。
- 运行记忆：当前 Session/Run 上下文，进入 PostgreSQL 快照与 Redis 热缓存。

LLM 不直接写入高置信度记忆。所有候选记忆经过结构校验、证据绑定、去重、置信度计算
和生命周期策略后再落库。人工“有用/无用”反馈进入路由与召回评分，但不能直接覆盖原始
执行证据。

### 22.4 工具生态

现有 `pent_tool_definitions`、`pent_tool_health`、`pent_tool_quality_metrics` 和
`pent_tool_route_decisions` 作为事实来源。需要补齐聚合查询，而不是在前端遍历所有 Run
临时统计。

工具页至少提供：

- 总工具、内置工具、健康工具、累计调用。
- namespace/source/risk/status 多条件过滤。
- 版本、依赖、执行后端、超时、并发限制。
- 成功率、P50/P95 延迟、失败原因和任务贡献度。
- 单工具健康检查、启停、版本切换和反馈。

### 22.5 工作区与报告

工作区元数据进入 PostgreSQL，文件内容进入 MinIO。对象键必须使用
tenant/project/run/artifact 层级生成，禁止接收客户端提供的绝对路径。

报告生成分两层：

1. 先生成稳定的结构化报告 JSON。
2. 由模板渲染器输出 HTML，再由独立适配器输出 DOCX。

报告必须记录模板版本、数据快照时间、Finding/Evidence 引用和内容哈希。

## 23. 可交给其他 AI 的实施任务

每个任务应单独提交，禁止一个 AI 同时重写全部页面和后端。

### P0：核心闭环

| 编号 | 任务 | 主要输出 | 验收 |
|---|---|---|---|
| P0-01 | 资产档案查询模型 | `PentAssetProfile` 后端契约和聚合 Repository | 单资产一次查询返回九页签摘要 |
| P0-02 | 资产概览页 | 风险评分、基础属性、技术栈和业务标签 | 无模拟数据，空/错/加载状态完整 |
| P0-03 | 端点与指纹 | 归一化端点、指纹入库和分页 API | 相同端点稳定去重并保留来源 |
| P0-04 | 漏洞生命周期 | Finding 状态机、证据链、批量处置 | 非法状态转换被后端拒绝 |
| P0-05 | HTML 报告 | 报告 JSON、HTML 模板、MinIO 产物 | 可以从资产详情生成并下载 |
| P0-06 | 模型配置 | 服务商密钥、模型角色绑定 API/UI | 密钥只写不回显，变更有审计 |

### P1：自动化与可观测

| 编号 | 任务 | 主要输出 | 验收 |
|---|---|---|---|
| P1-01 | LLM 流量采集 | 出站请求采集、脱敏、大小数据分离 | 可按 Method/URL/状态/来源/耗时检索 |
| P1-02 | LLM 流量 UI | 查询栏、分页表格、报文详情 | 万级数据使用服务端分页 |
| P1-03 | 工具统计 API | 工具概览、健康、排行和质量指标 | 聚合查询有索引且不做 N+1 |
| P1-04 | 工具生态 UI | 分类、状态、版本、指标和操作 | 健康检查结果实时刷新 |
| P1-05 | RMOS 语义记忆 | 向量适配、召回、引用和反馈 | 召回结果能追溯原始 Run/Evidence |
| P1-06 | 攻击路径回放 | 程序记忆、图数据投影、步骤时间线 | 可回放完整工具调用和结果摘要 |

### P2：安全与交付

| 编号 | 任务 | 主要输出 | 验收 |
|---|---|---|---|
| P2-01 | 双引擎护栏 | 静态规则、语义审计、确定性裁决 | 语义模型不可绕过硬拒绝规则 |
| P2-02 | 拦截中心 | 规则管理、记录检索、原始证据 | 每次阻断关联 Run 和 Toolcall |
| P2-03 | 工作区 | 文件树、预览、配额、下载、清理 | 租户隔离和路径穿越测试通过 |
| P2-04 | DOCX 报告 | DOCX 渲染适配器和模板版本管理 | 内容与同快照 HTML 一致 |
| P2-05 | 实时指标 | Dashboard 聚合、SSE/WebSocket 推送 | 断线恢复且不会重复累计 |

## 24. 接力实现规则

后续 AI 开始任务前必须：

1. 阅读本文件及相关专项任务书。
2. 查询现有表、路由、DTO 和页面，禁止创建第二套同义模型。
3. 在 `pent-api -> pent-core -> pent-server -> gateway/frontend` 依赖方向内实现。
4. 所有列表使用服务端分页、筛选和稳定排序。
5. 所有写操作包含 tenant、project、actor、request_id 和审计记录。
6. 所有工具、网络和文件操作先经过 Scope/Policy/Guard，不由 LLM 直接执行。
7. 页面不得使用硬编码统计数字冒充真实指标。
8. 占位接口返回明确的未实现状态；实现后删除占位提示。
9. 数据库变化必须更新参考数据库、覆盖导出 `sql/postgresql/0001_initial.sql` 并执行快照恢复测试。
10. 至少运行对应 Rust 测试、前端 typecheck 和生产构建。

单项任务的交付说明必须包含：修改文件、数据库变化、API 契约、权限码、测试结果、残余
风险和下一项可接力任务。
