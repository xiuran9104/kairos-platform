# Pent 智能渗透平台实现方案

## 1. 产品定位

Pent 的目标不是简单的任务和运行管理页面，而是建设一套面向授权安全测试的
AI 渗透测试运营平台。产品能力参考 J4Agent 等同类平台的交互形态，但领域模型、
Agent 运行时、工具体系、安全护栏、数据模型和服务端实现均在 Kairos Platform 中自主建设。

平台以四层能力形成闭环：

```text
感知观测
  -> 认知决策
    -> 工具执行
      -> 安全护栏
        -> 资产、漏洞、证据和记忆沉淀
          -> 下一轮任务召回与优化
```

所有主动测试只能在明确授权的项目和目标范围内执行。平台不能向浏览器提供任意
Shell，也不能允许 Agent 绕过范围校验、审批策略和审计记录。

### 1.1 VulnClaw 参考基线与对比结论

本文对 VulnClaw 的借鉴以其
[README](https://github.com/Unclecheng-li/VulnClaw/blob/a3ea7caa35ed4864535cf554c92178714ba33a8f/README.md)
和对应提交 `a3ea7caa35ed4864535cf554c92178714ba33a8f`（2026-07-25）为基线。
这里只比较 Pent 模块，不把 VulnClaw 当作 Kairos Platform 整体架构的替代方案，也不
复制其代码、Prompt、Skill 正文或品牌内容。

两者的产品边界不同：

| 维度 | VulnClaw | Pent |
| --- | --- | --- |
| 产品形态 | 本地 CLI、TUI 和单机 Web UI | 多租户 Web 平台和后台 Agent 服务 |
| 主要入口 | 命令或自然语言直接启动目标测试 | 项目、书面授权、Scope 确认后启动 Session/Run |
| 运行模型 | 模型主导的自主工具循环 | 模型主导循环 + 平台计划、审批、预算和运营工作流 |
| 工具执行 | 本地工具、MCP 和插件 | 注册表、内置处理器、NATS Worker/容器和受控第三方工具 |
| 状态与证据 | 本地 AgentState、文件和 JSONL 流量 | PostgreSQL 事实数据、MinIO 大对象、Redis 临时状态和持久化事件 |
| 安全边界 | 面向单用户授权测试和靶场 | 租户、项目、成员、授权期限、Scope Snapshot、审批和全链路审计 |
| 交付方式 | Markdown/HTML/PDF 和 PoC | 漏洞生命周期、资产风险、证据链、复测、HTML/DOCX 报告和审计导出 |
| 运维目标 | 本地环境诊断和工具可用性 | Worker 租约、并发配额、恢复、计量、可观测和高可用 |

应优先吸收的不是界面和命令数量，而是以下运行时机制：

1. **模型主导工具循环**：模型根据目标、约束、历史观察和可用工具决定下一步，框架只
   施加 Scope、策略、预算、证据和完成条件，不用固定阶段剧本代替模型判断。
2. **原始证据与活动上下文分离**：完整工具输出只保存一次，模型上下文默认接收有界的
   高信号预览，通过证据检索工具按需回看原文。
3. **结论证据闸门**：已验证漏洞、复现步骤和最终结论必须引用真实 Evidence；模型仅凭
   推测生成的结论只能进入“未验证线索”。
4. **重复与空转抑制**：识别相同工具参数、相同响应和重复查看相同证据范围；连续没有
   新证据时要求换策略、询问用户、明确受阻或结束，而不是消耗更多轮次。
5. **Skill 按需读取**：只把知识索引和摘要暴露给模型，模型需要时再读取具体方法论，
   避免把整个知识库塞入系统 Prompt。
6. **确定性报告链路**：报告中的请求、响应、命令、证据编号和复现材料来自持久化事实，
   LLM 只负责解释和归纳，不能创造不存在的证据。
7. **流量可检索与可重放**：授权范围内的 HTTP 交换保存为结构化证据，后续可列表、
   查看、差异比较和受控重放。

不直接照搬：

- 不提供绕过项目授权的快捷模式，也不因用户输入“已授权”就替代 Scope 确认；
- 不向 Web 用户开放任意 Shell、Python 执行或无限制 MCP 服务；
- 不采用本地 JSON/目录作为平台事实来源；
- 不以固定 1000 轮等大轮数代表自主性，Run 必须同时受时间、请求、Token、费用和风险
  预算约束；
- 不让模型生成的 PoC 自动进入可执行状态，PoC 必须标明来源、风险和审批要求；
- 不把某个 Skill 当成强制攻击剧本，知识建议始终服从当前授权、工具策略和证据状态。

## 2. 建设目标

Pent 最终需要提供：

1. 项目、资产、任务、Run、工具调用、证据和漏洞的全链路管理。
2. 多模型 Agent 自动规划、感知、推理、审查和复盘。
3. 内置 Rust 工具与第三方安全工具的统一注册、调度和隔离执行。
4. 静态规则、语义审计、人工审批和目标范围校验组成的安全护栏。
5. HTTP 流量、LLM 调用、工具输出和运行事件的完整审计。
6. 漏洞发现、研判、验证、定级、处置和复测的生命周期管理。
7. 基于语义、攻击路径和运行上下文的分层记忆系统。
8. Token、模型、工具、存储和任务资源的统一计量。
9. 可检索、可回放、可导出且可追责的渗透测试交付结果。

首要业务闭环为：

```text
创建项目
  -> 配置授权范围
  -> 创建任务
  -> Agent 生成计划
  -> 护栏检查
  -> 工具执行
  -> 保存流量与原始输出
  -> LLM 研判
  -> 生成候选漏洞
  -> 验证与定级
  -> 固化证据
  -> 更新资产风险
  -> 沉淀攻击路径
  -> 生成报告
```

### 2.1 当前实现基线

截至本文更新时，Pent 已不是纯规划文档。下列能力已经存在于仓库中，后续实现应在这些
边界上增量演进：

| 能力 | 当前状态 | 主要实现位置 |
| --- | --- | --- |
| 对话式模型工具循环 | 已实现，支持原生 tool call transcript 和后台 Turn 执行 | `crates/modules/pent-server/src/conversation/runtime.rs` |
| 多维预算 | 已实现轮次、时长、请求数和 Token 上限 | `crates/modules/pent-server/src/conversation/runtime.rs` |
| Scope 确认与工具校验 | 已实现 Session 授权确认和网络工具目标检查 | `crates/modules/pent-server/src/conversation/authorization.rs`、`crates/modules/pent-server/src/conversation/tool_guard.rs` |
| 工具目录 | 已从 `pent_tool_definitions` 加载，只向模型暴露有真实 Handler 的工具 | `crates/modules/pent-server/src/conversation/tool_catalog.rs` |
| 工具结果与 Evidence | 已写入 ToolCall、Evidence、上下文快照并计算内容哈希 | `crates/modules/pent-server/src/conversation/repository_postgres.rs` |
| 活动上下文压缩 | 已将模型可见工具观察限制为有界预览，完整结果保存在 Evidence | `crates/modules/pent-server/src/conversation/runtime.rs` |
| 自主循环动作 | 已支持 `FINAL`、`ASK_USER`、`NO_PATH` 和 `REPLAN`，同时支持文本前缀和 JSON 结构化动作协议，普通回答保持兼容 | `crates/modules/pent-server/src/conversation/runtime.rs` |
| 重复与空转抑制 | 已按单 Turn 的请求指纹跳过完全重复调用，并检测重复结果和连续无进展 | `crates/modules/pent-server/src/conversation/runtime.rs`、`scan_progress.rs` |
| 完成门禁 | 已取消固定工具阶段清单，改为检查实际工具进展和已验证结论的 Evidence 引用 | `crates/modules/pent-server/src/conversation/scan_progress.rs` |
| 知识检索 | 已提供 `knowledge_search` 并保存成功工具观察 | `crates/modules/pent-server/src/conversation/tool_catalog.rs`、`crates/modules/pent-server/src/conversation/memory_adapter.rs` |
| 工具路由 | 已有硬过滤和确定性评分骨架 | `crates/modules/pent-core/src/tool_router/` |
| 认知运行时 | 已有 TCG、PreFlect、Reflect 和元认知领域契约及部分实现 | `crates/modules/pent-api/src/cognition.rs`、`crates/modules/pent-server/src/cognition/` |

仍需补齐的关键差距：

| 差距 | 影响 | 目标 |
| --- | --- | --- |
| Evidence 只有存储和预览，缺少模型可调用的分页检索/查看 | 长任务无法可靠回查完整原文 | 增加 `evidence_list/search/view` |
| 重复检测仍主要限于单 Turn | 跨 Turn 的近似请求仍可能空转 | 持久化响应哈希、语义指纹和跨 Turn 窗口 |
| Evidence Gate 仍以文本标记和 Evidence ID 为主 | 尚不能校验每个字段是否真的由证据支持 | 引入结构化 Claim，并逐字段投影真实 Evidence |
| 缺少统一 HTTP Traffic 领域模型和重放入口 | 请求、响应、漏洞与报告引用不够稳定 | 增加 `traffic_list/view/diff/replay` |
| 知识检索尚未形成 Skill 索引协议 | 容易召回过多正文或缺少适用条件 | 建立 Skill Manifest 和按需引用 |
| 报告仍包含模型自由文本路径 | 可能出现不可复现描述 | 改为事实投影 + LLM 解释的双层报告 |
| 工具插件、MCP、内置 Handler 和 Worker 契约尚未完全统一 | 路由、风险和健康口径分散 | 全部映射到统一 ToolDescriptor |

2026-07 的自主循环 P0 已完成以下收敛：

- 删除”先额外调用一次模型生成纯文本攻击计划”的路径，模型直接进入原生 Tool Calling；
- 模型随 Tool Calling 返回的计划或理由保留在 assistant/tool transcript 中；
- 固定要求 `web_crawl -> dir_fuzz -> js_collect -> js_analyze -> http_request` 的门禁已移除；
- Runtime 只判断是否产生新观察、是否重复请求/结果、是否连续空转和结论是否引用 Evidence；
- 明确区分完成、询问用户、路径耗尽和重新规划，不再把所有无 Tool Call 文本都直接当成功；
- 当前轮意图已注入模型上下文，但不能覆盖 Scope、策略、审批和预算；
- **自主循环启动注入**：每轮工具循环开始时向模型注入 `AUTONOMOUS_LOOP_PROMPT`，明确告知当前处于自主执行模式，必须直接调用工具或发出 FINAL/ASK_USER/NO_PATH/REPLAN 动作，不得等待用户许可；
- **双协议动作解析**：`AgentDirective` 同时支持文本前缀（`FINAL:`、`ASK_USER:` 等及中文等价标记）和 JSON 结构化动作（`{“action”:”final”,”content”:”...”}`），JSON 格式优先匹配，实现更鲁棒的模型输出解析；
- **证据多样性闸门**：FINAL 门禁新增工具类别多样性检查（`MIN_EVIDENCE_CATEGORY_DIVERSITY = 2`），要求至少两类工具（信息收集 + 漏洞验证）产生有效观察后才接受安全测试结论，防止仅凭单次 HTTP 探测就声称完成渗透测试；
- **上下文感知防空转诊断**：空转警告根据已积累的证据类别（纯信息收集/已有漏洞验证/全部失败）给出差异化建议，而非统一文案，降低模型机械重试概率；
- **工具类别追踪**：`ScanProgress` 按 Recon / VulnValidation / Knowledge / EvidenceRead 分类追踪工具调用，用于多样性闸门和自适应诊断。

下一步不是恢复固定攻击阶段，而是把当前内存中的 Progress 状态持久化为跨 Turn
AgentState，并用结构化 Claim/Evidence 关系替代文本关键词门禁。

## 3. 平台能力分层

### 3.1 感知观测层

职责：

- 资产发现、端口扫描和服务指纹识别；
- HTTP 爬取、端点发现和接口归一化；
- 域名、IP、URL、证书、技术栈和业务标签识别；
- 扫描器输出、网络流量和工具原始结果采集；
- 内外网边界、登录入口、管理后台和门户类型判断；
- 资产变化和历史发现对比。

主要产物：

- Asset；
- Endpoint；
- Service Fingerprint；
- Raw Discovery；
- HTTP Traffic；
- Tool Output。

### 3.2 认知决策层

建议的模型角色：

| 角色 | 职责 |
| --- | --- |
| Planner | 根据项目目标和当前上下文制定执行计划 |
| Perception | 归纳资产、端点、指纹和工具输出 |
| Reasoner | 推断攻击面、漏洞可能性和后续动作 |
| Reviewer | 校验结论、证据完整性和误报风险 |
| Guard | 对高风险意图做语义安全审计 |
| Reporter | 汇总资产、漏洞、证据和修复建议 |

认知层必须输出结构化决策，包括：

- 决策原因；
- 输入证据引用；
- 建议工具；
- 目标与参数；
- 风险级别；
- 置信度；
- 是否需要人工审批；
- 预计资源和 Token 成本。

#### 模型主导的受控求解循环

Pent 采用“模型主导、平台约束”的循环。Planner 或模型可以决定使用什么工具和何时结束，
但平台始终拥有授权、预算、审批、执行和完成判定的最终控制权。

```text
加载 Objective / Scope Snapshot / Policy Snapshot / Budget Snapshot
  -> 召回近期消息、Run Summary、Evidence 摘要和可用 ToolDescriptor
  -> 模型输出 TOOL_CALL / ASK_USER / NO_PATH / FINAL
  -> TOOL_CALL 先经过 Scope、策略、风险、审批和预算检查
  -> Worker 执行并持久化完整原始结果
  -> 提取高信号观察，更新 Evidence、Traffic、Finding 和 Run Summary
  -> 重复/空转检查
  -> 下一轮模型推理
  -> FINAL/NO_PATH 经过完成闸门
  -> 生成事实报告和解释层
```

运行时动作应统一为：

| 动作 | 含义 | 平台处理 |
| --- | --- | --- |
| `TOOL_CALL` | 请求调用一个已注册工具 | 执行硬校验，必要时进入审批 |
| `ASK_USER` | 缺少凭据、业务语义或必要授权信息 | 暂停相关分支，不占用空转轮次 |
| `NO_PATH` | 在当前授权、能力和预算内无可行路径 | 检查是否仍有未处理高信号锚点 |
| `FINAL` | 提交阶段或 Run 最终结论 | 校验证据多样性、证据引用、报告结构和未完成事项 |
| `REPLAN` | 当前假设失败或环境变化 | 保留证据并生成 Plan Delta，不重置整个 Run |

固定阶段可以用于进度展示、覆盖度统计和报告组织，但不能变成强制调用某组工具的脚本。
对于目标明确的小任务，模型可以直接验证；对于广泛扫描任务，完成门禁再要求必要覆盖。

#### 上下文与证据双层模型

Pent 需要明确区分：

```text
事实层：完整工具输出、HTTP 流量、文件、哈希、时间、执行参数和审计
  -> 存 PostgreSQL / MinIO，不因上下文压缩而丢失

活动上下文层：目标、硬约束、近期消息、计划增量、高信号观察和 Evidence 引用
  -> 有 Token 上限，可重建、可压缩、不得替代事实层
```

每条 Evidence 至少包含：

```text
evidence_id
tenant_id / project_id / run_id / step_id / tool_call_id
source_type
content_type
content_hash
raw_object_key 或 inline_content
raw_size
preview
high_signal_facts[]
request_ref / response_ref
scope_decision
redaction_version
created_at
```

模型侧增加只读工具：

| 工具 | 用途 | 约束 |
| --- | --- | --- |
| `evidence_list` | 按 Run、工具、类型和时间列出证据摘要 | 默认分页，不返回 raw |
| `evidence_search` | 在当前 Run 的授权证据内检索关键词和结构化字段 | 返回命中片段、偏移和 Evidence ID |
| `evidence_view` | 分页查看指定 Evidence 的原文 | 单次大小受限，记录访问范围 |
| `traffic_list` | 列出当前 Run 捕获的 HTTP 交换 | 自动脱敏，限定 Scope |
| `traffic_view` | 查看请求、响应和关联 Finding | 响应体分页，大对象返回引用 |
| `traffic_diff` | 比较两次请求或响应的语义差异 | 保存比较结果为派生 Evidence |
| `traffic_replay` | 基于历史请求做受控重放 | 重新执行 Scope、审批、速率和凭据策略 |

同一内容通过 `content_hash` 去重。若新结果与历史原文一致，活动上下文只注入引用和
“无变化”信号，不重复放入正文；事实层仍保存本次 ToolCall 与已有 Evidence 的关联。

#### 高信号事实与完成闸门

高信号事实是从原始证据确定性提取并可独立引用的结构化观察，例如：

- HTTP 状态、响应头、表单、参数、端点和重定向；
- 开放端口、服务版本、证书和指纹；
- 源码位置、危险 Sink、鉴权分支和输入过滤；
- 请求差异、响应差异、回显标记和可复现步骤；
- 工具错误、超时、被 WAF 拦截或因 Scope/审批未执行。

Evidence 采用以下成熟度，不把“模型置信度”混同为“证据强度”：

| 等级 | 含义 | 可进入报告的位置 |
| --- | --- | --- |
| E0 | 仅有目标、线索或知识库推测 | 待验证线索 |
| E1 | 有真实观察，但尚不能证明漏洞 | 观察与安全配置问题 |
| E2 | 多次差异或工具命中支持假设 | 候选漏洞 |
| E3 | 授权范围内可重复验证，复现材料完整 | 已验证漏洞 |
| E4 | E3 基础上经过独立工具、Reviewer 或人工复核 | 已确认漏洞 |

`FINAL` 门禁至少检查：

1. 至少两类工具（信息收集 + 漏洞验证）已产生有效观察，防止单工具”走过场”报告；
2. 每个”已验证/已确认”结论都引用 E3/E4 Evidence；
3. 引用的 Evidence 属于当前租户、项目和 Run，且目标在 Scope Snapshot 内；
4. 报告中的请求、响应、命令、状态码和响应标记能从 Evidence 确定性投影；
5. 未验证线索明确列出已尝试方法、阻塞原因和下一步建议；
6. 没有把工具错误、知识库建议或模型推测误写为漏洞事实；
7. 所有高风险动作均有策略允许或审批记录；
8. 超出预算、被拒绝或未覆盖的范围在最终状态中显式披露。

`NO_PATH` 不是静默成功。如果仍存在尚未处理的高信号端点、参数、源码 Sink 或显著响应
差异，平台应拒绝提前结束并提示重新规划；若确实受工具、授权、凭据或预算阻塞，则允许
结束为 `blocked` 或 `partial`，而不是 `completed`。

#### 防空转和自适应纠偏

每轮记录工具、规范化参数、目标、响应哈希、退出状态、耗时和新增 Evidence 数。满足
以下任一条件时产生纠偏信号：

- 完全相同的工具与规范化参数已执行；
- 多次参数变化但响应哈希、长度和高信号事实没有变化；
- 连续查看同一 Evidence 的相同范围；
- 连续多个工具失败且失败类别相同；
- 连续若干轮没有新增 Evidence、Finding、Asset、Endpoint 或 Plan Delta；
- 同一漏洞假设反复尝试相同验证策略。

纠偏优先级为：

```text
复用已有 Evidence
  -> 改变验证策略或切换工具
  -> 缩小目标/提出澄清问题
  -> 记录阻塞和未验证线索
  -> NO_PATH / partial completion
```

纠偏层只提供信号和硬上限，不接管模型的领域推理。语义近似检测的阈值、窗口和触发原因
必须写入 Run 事件，便于解释为何跳过某次调用。

#### Skill、工具和插件的职责分离

| 类型 | 提供内容 | 是否可产生事实证据 |
| --- | --- | --- |
| Skill | 方法论、检查清单、适用条件、工具建议 | 否 |
| Tool | 对目标或平台数据执行受控动作 | 是 |
| Plugin | 针对输入数据执行低耦合检测或归一化 | 只有绑定真实输入和执行记录时才可以 |
| Memory | 历史经验、资产事实、成功/失败路径 | 必须保留来源 Evidence 才可作为事实 |

Skill Manifest 至少包含 `id`、`version`、`summary`、`tags`、`applicable_targets`、
`risk_notes`、`required_capabilities`、`references` 和 `content_hash`。模型默认只看到
相关 Skill 的索引；读取正文必须通过 `knowledge_search` 或后续
`load_skill_reference`，并记录命中、引用和版本。

MCP、本地宿主机工具、内置 Rust Handler、HTTP 后端和检测插件最终都映射为统一 `ToolDescriptor`。
来源不同不改变 Scope、风险、审批、超时、资源、健康检查、输入输出 Schema 和审计要求。
任何外部 MCP 服务都必须由平台管理员注册，不能接受浏览器临时传入任意启动命令。

#### VulnClaw 与 Pent 的工具体系差异

MCP 是工具发现与调用协议，不是安全沙箱，也不是工具能力本身。一个 Nmap 工具可以通过
MCP、本地进程、HTTP 服务或内置适配器暴露；采用 MCP 不会自动获得租户隔离、Scope
校验、审批、资源限制、证据持久化和审计。

VulnClaw 实际上也不是“所有工具都是 MCP”，而是四套能力共同提供给 Agent：

| VulnClaw 类型 | 示例 | 作用 |
| --- | --- | --- |
| MCP 服务 | 本地 `fetch`、`memory`，外部 Chrome DevTools、Burp | 接入独立服务提供的动态工具 |
| 内置 Agent 工具 | HTTP 批量探测、Python、Shell、Nmap、证据查看、编解码 | 直接在 Agent 运行时注册和调用 |
| 检测插件 | HTTP Header、JavaScript Endpoint、JWT 等检查 | 对已有输入做低耦合分析并生成候选发现 |
| Skill | recon、vuln-discovery、web-pentest 等 | 给模型提供方法论和参考资料，不直接执行 |

Pent 当前采用“注册表优先、执行协议可插拔”：

| Pent 类型 | 当前情况 | 与 VulnClaw 的主要区别 |
| --- | --- | --- |
| Builtin | `js_analyze`、`knowledge_search` 等进程内能力 | 只承载低资源、无任意执行的确定性逻辑 |
| HostLocal | `nmap`、`nuclei`、`ffuf`、`sqlmap` 等宿主机 allowlist 工具 | 强调绝对路径、`prlimit`、超时、输出上限和审计 |
| HTTP | 已有执行后端契约 | 适合调用平台管理的远程扫描服务 |
| MCP | Chrome DevTools 等有状态交互工具；`mcp_navigate_page` 有 CDP HTTP fallback | 作为可选接入协议，而不是全部工具的基础 |
| Wiki/Skill | 仓库保存 AboutSecurity 原始 Skill，运行时主要检索编译后的 `guide` | 原料与执行指导隔离，更保守，但缺少完整 Skill Resolver |

因此 Pent 不需要把现有工具全部改造成 MCP。建议按能力选择后端：

| 场景 | 推荐后端 | 原因 |
| --- | --- | --- |
| JSON 解析、JS 静态分析、证据检索、编码转换 | Builtin | 低延迟、确定性强、易测试 |
| Nmap、Nuclei、FFUF、Sqlmap 等进程工具 | HostLocal | 当前实现统一在网关宿主机执行，不依赖 Docker 工具镜像 |
| Chrome DevTools、Burp 等有状态交互工具 | MCP / CDP HTTP fallback | MCP 动态工具发现有价值；Chrome 导航可在 MCP 不可用时走 CDP HTTP |
| 企业扫描器、云 WAF、资产平台 | HTTP | 远程服务已有稳定 API、鉴权和任务模型 |
| 只读规则检测和结果归一化 | Plugin/Builtin | 无需引入独立服务生命周期 |

MCP 接入 Pent 后仍必须经过以下链路：

```text
MCP Server 注册
  -> 管理员审核并固定服务版本/启动配置
  -> 拉取 Tool Schema
  -> 转换为 ToolDescriptor
  -> 租户/项目可见性过滤
  -> ToolRouter 选择
  -> Scope / Policy / Approval / Budget 检查
  -> MCP Gateway 代为调用
  -> 结果标准化、脱敏和 Evidence 持久化
```

不能让浏览器或普通项目成员提交任意 `command`、`args`、SSE URL 或本地路径来启动 MCP
Server。MCP Server 的工具 Schema 也不是可信安全策略：平台必须覆盖其风险等级、审批
规则、网络策略和输出限制。

#### VulnClaw 与 Pent 的 Skill 差异

VulnClaw 已形成较完整的运行时 Skill 产品：

- `SKILL.md` 保存摘要、触发信息和方法论；
- `references/` 保存详细资料；
- Resolver 根据阶段、目标类型、技术栈、漏洞线索和显式调用做确定性评分；
- 一次只选择一个主 Skill 和少量辅助 Skill；
- Reference 由模型通过工具按需读取；
- 选择理由、置信度和实际加载的 Reference 可形成来源记录。

Pent 当前的优势是已经保存大量 AboutSecurity Skill 原料，并制定了
`safe-import-policy`、安全 Wrapper 和 `knowledge_search`；数据库初始化快照中也包含
编译后的 `guide`。主要差距是：

1. 运行时尚未直接解析统一的 Skill Manifest；
2. `knowledge_search` 主要按标题和正文关键词检索，缺少目标类型、阶段、技术栈、风险和
   所需工具等类型化路由；
3. 返回内容仍以较长 Guide 正文为主，缺少“索引先行、Reference 按需读取”；
4. 没有完整记录“选择了哪个 Skill、为什么选择、加载了哪些 Reference、最终是否有效”；
5. 原始 Skill、审查后的 Wrapper 和可执行策略之间缺少机器可校验的发布状态；
6. 知识库中可能存在互相冲突的指导。例如当前初始化快照同时强调安全 Wrapper，又存在
   “WAF Bypass — Unrestricted”且声明不限制授权和工具的 Guide。执行器硬限制仍应阻止
   越权，但该内容会误导模型规划，必须下线或重新编译。

建议的 Pent Skill 生命周期：

```text
Raw Skill
  -> 安全分类与许可证/来源记录
  -> 编译为 Reviewed Skill Manifest + 安全正文
  -> 审核发布
  -> Skill Resolver 选择主 Skill / 辅助 Skill
  -> 模型按需 load_skill_reference
  -> 依据 Skill 建议选择 ToolDescriptor
  -> 工具执行产生 Evidence
  -> 记录 Skill 对结果的贡献与误导反馈
```

Skill Manifest 建议在现有字段上再增加：

```text
source_url / source_revision / license
publication_status
agent_execution_allowed
requires_review
allowed_risk_levels
required_scope_types
blocked_actions[]
compiled_from[]
reviewed_by / reviewed_at
```

Skill 只回答“应该如何思考、检查什么、何时停止”，Tool 才回答“实际执行什么”。任何
Skill，即使标记为已发布，也不能直接扩大 Scope、跳过审批或授权任意执行后端。

#### 优化优先级

| 优先级 | 优化项 | 完成标准 |
| --- | --- | --- |
| P0 | 清理冲突或不安全的 Wiki Guide | 不存在可检索的“跳过授权/不限工具”指导；有自动 lint |
| P0 | 建立 Reviewed Skill Manifest 和发布状态 | Raw Skill 默认不可被 Agent 执行性召回 |
| P0 | 实现类型化 Skill Resolver | 可解释地选择一个主 Skill 和最多两个辅助 Skill |
| P0 | 从活动 ToolDescriptor 动态生成模型工具清单 | 不再由系统 Prompt 手工维护可能过期的工具列表 |
| P1 | 实现 `load_skill_reference` 和来源审计 | 正文按需分页加载，Run 保存 Skill/Reference 版本 |
| P1 | 补齐受控 MCP Gateway | Chrome/Burp MCP 可接入且全部经过平台硬策略 |
| P1 | 统一 Builtin/HostLocal/HTTP/MCP 的结果归一化 | 模型和报告不依赖具体执行后端 |
| P2 | Skill 效果反馈与灰度发布 | 可按证据新增率、确认率和误导率调整路由 |

### 3.3 工具执行层

工具分为：

- 自研内置工具：HTTP 请求、DNS 查询、端口连接、证书解析、内容分析等；
- 第三方工具：Nuclei、Afrog、FFUF 等；
- 平台工具：资产查询、文件读写、证据保存、报告生成；
- 受控命令工具：只允许在隔离 Worker 中按策略执行。

统一工具契约至少包含：

```text
name
version
category
risk_level
input_schema
output_schema
timeout
resource_profile
network_policy
approval_policy
health_check
```

工具不能在 Gateway 进程中运行。高资源或外部进程工具进入独立 Worker/容器，由
NATS 调度，并设置 CPU、内存、文件、进程、网络和执行时间限制。

当前基础实现已经用允许列表注册表替换模拟执行器：Worker 只能启动平台登记的绝对路径
程序，目标必须与 Run 的 Scope Snapshot 完全匹配，并通过 `prlimit`、执行超时、环境
变量清空和 1 MiB 输出上限约束进程。`ssh_exec`、`sql_query` 和任意 Shell 不在直接
执行允许列表中。现阶段属于宿主机受限进程；生产部署仍应把同一执行接口切换到独立容器
或沙箱 Worker，以获得文件系统和网络命名空间隔离。

#### 结果标准化

所有工具输出先由 `pent-core` 清洗和归一化，再通过
`tool.result.normalized` 事件回传规划层。规划层不直接依赖 Nmap、Nuclei、FFUF
等工具的原始 stdout 格式。统一结果包含：

- 工具、目标、执行状态、摘要和协议版本；
- 结构化观察、漏洞发现、证据和产物引用；
- 退出码、耗时和警告；
- 原始输出对象存储引用，不在事件中直接传输超大内容。

通用清洗器负责移除 ANSI/控制字符、限制文本大小和脱敏凭据。各工具适配器在此基础上
解析端口、HTTP 服务、模板命中等领域字段；无法识别的输出仍以低置信度通用观察返回，
保证规划层始终收到稳定结构。

首批专用适配器包括：

| 工具 | 输入格式 | 标准化内容 |
| --- | --- | --- |
| Nmap / TCP 扫描 | 文本端口表 | 端口、协议、状态、服务、产品指纹 |
| Nuclei | JSON Lines | 模板名称、等级、描述、命中目标 |
| Afrog | JSON Lines | 规则名称、等级、描述、命中目标 |
| FFUF | JSON | URL、状态码、响应长度、内容类型 |

标准结果进入确定性的规划反馈器，产生 `continue`、`replan` 或 `stop` 指令以及建议动作。
该反馈通过 `planner.feedback` 事件写入 Run，后续模型规划器只在这个稳定输入之上做动态
任务扩展。工具失败时要求重新规划；出现高风险发现时优先验证和保存证据；无有效观察时
停止当前分支，避免无意义扫描。

首条自适应闭环已经接通：TCP/Nmap 结果发现 80 或 443 端口后，规划器生成只读
`http_probe` 调用，Worker 再次校验其 URL 主机是否属于原始授权目标，然后执行、标准化
并产生新一轮 `planner.feedback`。单次 Run 当前最多规划 3 个工具步骤，后续由预算模型
把次数、时长和 Token 限额统一起来。

### 3.4 安全护栏层

安全护栏覆盖计划、工具选择、参数、网络访问、执行结果和数据展示：

1. 项目必须保存授权主体、授权期限和目标范围。
2. Run 创建时固化不可变 Scope Snapshot。
3. 所有网络工具调用统一经过 Scope Validator。
4. 域名解析后再次验证真实 IP，防止 DNS Rebinding。
5. 静态规则拦截已知危险 URL、命令和参数。
6. LLM Guard 识别删除数据、修改密码、权限变更等高危意图。
7. 高风险操作进入人工审批，超时后默认拒绝。
8. 请求和响应存档前过滤 Token、Cookie、密码和密钥。
9. 所有决策关联用户、租户、项目、Run 和 Trace。
10. 支持项目级紧急停止，撤销所有排队和执行中的工具调用。

## 4. 产品信息架构

```text
Pent AI 安全平台
├─ 安全仪表盘
├─ 项目与任务
│  ├─ 项目管理
│  ├─ 任务调度
│  └─ 运行中心
├─ 漏洞中心
│  ├─ 漏洞列表
│  └─ 漏洞生命周期详情
├─ 资产中心
│  ├─ 资产列表
│  └─ 资产详情
├─ 安全护栏
│  ├─ 拦截规则
│  ├─ 拦截记录
│  └─ 人工审批
├─ 流量与证据
│  ├─ LLM 流量日志
│  ├─ 工具调用日志
│  └─ 证据中心
├─ RMOS 记忆
│  ├─ 攻击路径图谱
│  ├─ 记忆管理
│  └─ 执行回放
├─ 工具生态
├─ 工作区文件
└─ 设置中心
   ├─ 模型供应商
   ├─ 模型角色绑定
   ├─ 成本策略
   └─ 系统策略
```

现有页面迁移关系：

| 当前页面 | 目标页面 |
| --- | --- |
| Dashboard | 安全仪表盘 |
| Projects | 项目管理 |
| Tasks | 任务调度 |
| Runs | 运行中心与执行回放 |
| Approvals | 安全护栏/人工审批 |
| Evidence | 流量与证据/证据中心 |
| Findings | 漏洞中心 |
| Settings | 拆为工具生态、模型设置和系统设置 |

## 5. UI 页面与功能

### 5.1 安全仪表盘

顶部指标：

- 总任务；
- 漏洞总数；
- 资产数量；
- 工具调用量；
- LLM 总调用；
- Token 总消耗和预估成本。

可视化区域：

- 漏洞等级分布；
- 漏洞类型 TOP 8；
- 高频工具 TOP 6；
- 模型角色调用分布；
- 资产风险排序；
- PostgreSQL、Redis、NATS、MinIO、向量/图数据服务健康状态；
- 实时任务活动流。

刷新策略：

- 汇总指标使用短周期查询或服务端推送；
- 活动流使用 SSE；
- 图表请求支持统一时间范围和项目筛选；
- 组件健康状态不得泄露连接串和内部凭据。

### 5.2 项目与任务

项目管理：

- 名称、描述、负责人、租户和成员；
- 授权文件及有效期；
- IP、CIDR、域名、URL 和端口授权范围；
- 禁止目标和例外规则；
- 项目级速率、并发和工具策略；
- 紧急停止；
- 项目资源与成本统计。

任务调度：

- 任务模板、优先级、目标范围和执行窗口；
- 排队、运行、等待审批、成功、失败、取消状态；
- 暂停、恢复、取消和重试；
- 定时任务和并发限制；
- 批量任务和依赖关系。

运行中心：

- Run 状态、当前阶段、Worker 和执行时间；
- Agent 决策、工具调用和审批事件；
- SSE 实时事件和断线重放；
- 执行步骤树与时间线；
- 取消和紧急停止；
- 证据、漏洞、产物和成本汇总。

### 5.3 漏洞中心

漏洞列表：

- 严重、严重/高危/中危/低危/信息五级筛选；
- 资产、漏洞类型、状态、来源、置信度和时间筛选；
- 批量误报、忽略、复测、改等级和创建工单；
- 支持项目、Run 和工具来源追踪；
- 支持去重、合并和历史版本。

漏洞详情：

- 描述、影响范围、利用条件、修复建议；
- 完整请求包、响应包、PoC 和原始工具输出；
- 关联资产、端点、指纹和证据；
- 发现、智能研判、验证确认、定级四阶段流程；
- LLM 研判记录；
- RunID、StepID 和 ToolCallID；
- 人工复核与复测结果；
- 状态与等级变更历史。

漏洞置信度必须由多个可解释信号计算，不能只保存模型给出的单一分数。

### 5.4 拦截中心

规则配置：

- 静态关键字、URL、命令和参数规则；
- LLM 语义审计规则；
- 项目/工具/风险级别作用范围；
- 告警、审批和直接拒绝动作；
- 规则版本、发布和回滚。

拦截记录：

- 项目、Run、工具、请求命令、命中规则和风险说明；
- 原始请求的脱敏视图；
- LLM Guard 判定文本；
- 处置结果、操作人和时间；
- 默认 15 秒刷新，也可使用 SSE 推送。

### 5.5 资产中心

资产列表支持表格和卡片双视图，展示：

- IP、域名或 URL；
- 内外网标签；
- 开发语言和技术栈；
- 风险评分；
- 各等级漏洞数量；
- 端口与服务指纹；
- 业务标签；
- 最近发现和扫描时间。

资产详情包含九个页签：

1. 概览；
2. 报告；
3. 漏洞；
4. 端点；
5. 指纹；
6. 原始发现；
7. 时间线；
8. 攻击图谱；
9. 历史路径。

报告支持 HTML 和 DOCX。所有报告内容必须能回溯到对应证据，且导出前执行敏感信息
过滤。

### 5.6 LLM 流量日志

检索条件：

- Method；
- URL；
- 状态码；
- 来源；
- 模型角色；
- Run；
- 工具；
- 耗时；
- 时间范围。

详情展示：

- 脱敏后的请求和响应原文；
- 请求头和平台专属 User-Agent；
- TraceID、RunID 和 ToolCallID；
- Token、模型、耗时和成本；
- 流量是否被护栏拦截；
- 原始数据对象存储引用。

平台测试流量必须使用可配置且明确的 User-Agent，便于授权方 WAF 与业务流量区分。

### 5.7 RMOS 记忆系统

采用三层记忆：

| 记忆层 | 内容 | 建议存储 |
| --- | --- | --- |
| 语义记忆 | 资产特征、漏洞知识和历史结论 | PostgreSQL + pgvector |
| 路径记忆 | 攻击步骤、节点关系和成功路径 | 首期 PostgreSQL，后续图数据库 |
| 运行上下文 | 当前 Run 的计划、观察和临时状态 | Redis + PostgreSQL 事件 |

页面功能：

- 全局攻击路径图谱；
- 侦察、扫描、验证等步骤序列回放；
- 已验证、待确认、理论三级置信度；
- 有用/无用人工反馈；
- 记忆来源、证据和适用范围；
- 召回命中记录；
- 记忆禁用、过期和删除。

跨项目召回必须遵守租户、授权范围和数据分级，不得将一个客户的资产信息泄露给另一个
客户。

### 5.8 工具生态

页面展示：

- 总工具数、内置工具数、第三方工具数和累计调用数；
- 工具就绪状态、版本、依赖和最近健康检查；
- 调用排行、成功率、P95 耗时和失败原因；
- 自研工具和第三方工具分类；
- 风险级别和审批策略；
- Worker 可用性。

“67 个工具”是产品目标，不作为首期一次性交付条件。工具必须逐个完成契约、
隔离、超时、输出解析、安全测试和健康检查后才标记为 ready。

### 5.9 工作区文件

功能：

- 按租户、项目和 Run 隔离；
- 文件列表、搜索、预览和下载；
- 爬虫、扫描、证据、报告和中间产物分类；
- 存储占用统计；
- 生命周期和过期策略；
- 清空空间需要二次确认和审计；
- 禁止路径穿越和跨项目访问。

大文件存入 MinIO，PostgreSQL 只保存元数据、哈希和对象键。

### 5.10 设置中心

模型供应商：

- OpenAI、DeepSeek、Claude 及兼容服务；
- API Endpoint、模型、超时、代理和限额；
- API Key 加密存储；
- 连通性测试；
- 页面仅显示掩码，不回显明文。

模型角色绑定：

- Planner、Perception、Reasoner、Reviewer、Guard 和 Reporter；
- 主模型和降级模型；
- 最大 Token、温度、超时和预算；
- 项目级覆盖；
- 成本上限和熔断策略。

### 5.11 对话式启动与 Agent 协作工作台

Pent 的主要入口采用自然语言目标输入，不要求普通用户先理解扫描器参数：

```text
用户描述测试目标
  -> 系统提取目标、范围、重点、禁止项和预算
  -> 展示结构化授权确认
  -> 启动 Agent
  -> Agent 后台自主运行
```

启动页包含：

- 自然语言目标输入；
- 项目和授权文件选择；
- 自动提取的目标范围；
- 测试重点和禁止操作；
- 时间、并发、Token 和费用预算；
- 授权范围确认；
- 自主、监督和隔离靶场执行模式。

Run 协作工作台包含：

```text
+----------------------------+--------------------------+
| 执行计划与实时活动         | Agent 协作               |
| 当前阶段                   | 用户与 Agent 消息        |
| 步骤和工具调用             | 追加上下文与调整指令     |
| 审批、证据和漏洞           | 解释、复核和阶段总结     |
+----------------------------+--------------------------+
| 流量 / 证据 / 漏洞 / 成本 / 产物 / 审计              |
+-------------------------------------------------------+
```

对话用于启动、解释和调整，但不是 Run 持续执行的必要条件。用户关闭浏览器后 Agent
继续在 Worker 中运行，重新进入页面后恢复消息、执行计划和事件。

对话允许：

- 提供业务背景和测试账号引用；
- 调整授权范围内的测试重点；
- 跳过目标或步骤；
- 请求漏洞复核和解释；
- 请求阶段性总结；
- 暂停、恢复和停止。

对话不允许直接扩大授权范围、关闭硬性安全限制或替代高风险操作审批。文件上传、
存储配额、路径安全、目标范围、网络隔离、资源上限、凭据保护和全链路审计始终由平台
强制执行。

## 6. 全链路标识

所有核心数据通过以下标识关联：

```text
tenant_id
user_id
project_id
task_id
run_id
step_id
tool_call_id
asset_id
endpoint_id
vulnerability_id
evidence_id
traffic_id
memory_id
trace_id
```

任何漏洞都必须能够反向查询：

```text
漏洞
  -> 证据
    -> 工具调用
      -> Agent 决策
        -> Run 事件
          -> 项目授权范围
```

## 7. 后端模块边界

### 7.1 API 契约

```text
crates/modules/pent-api/src/
  project.rs
  task.rs
  run.rs
  asset.rs
  vulnerability.rs
  evidence.rs
  traffic.rs
  guardrail.rs
  memory.rs
  tool.rs
  workspace.rs
  model.rs
  metrics.rs
  events.rs
  permissions.rs
```

`pent-api` 只放稳定 DTO、命令、事件和权限常量，不依赖 Axum、SQLx、NATS 或具体
模型 SDK。

### 7.2 服务实现

```text
crates/modules/pent-server/src/
  project/
  scheduler/
  runtime/
  asset/
  vulnerability/
  evidence/
  traffic/
  guardrail/
  memory/
  tool/
  workspace/
  report/
  model/
  metering/
  transport/
    http/
    sse/
    mq/
```

职责约束：

- Gateway 负责外部认证、租户、RBAC、请求审计和 API 聚合；
- Pent Server 负责领域用例、数据访问和执行策略；
- Pent Worker 负责 Agent 循环和受控工具执行；
- `pent-core` 负责与传输、数据库无关的状态机和领域规则；
- 前端不能直接连接 Worker、数据库、Redis、NATS 或 MinIO。

## 8. 数据与基础设施

| 数据 | 首选存储 |
| --- | --- |
| 项目、任务、Run、资产、漏洞 | PostgreSQL |
| Run 事件和审批状态 | PostgreSQL 为事实来源，NATS 用于通知 |
| 临时上下文、租约和并发控制 | Redis |
| 工具任务与事件通知 | NATS |
| 请求响应、扫描文件、证据和报告 | MinIO |
| 语义向量 | PostgreSQL + pgvector |
| 攻击路径 | 首期 PostgreSQL，规模扩大后评估图数据库 |
| 全文日志 | 首期 PostgreSQL，规模扩大后评估 OpenSearch |
| 指标 | Prometheus |
| Trace | OpenTelemetry + Tempo/Jaeger |

不在首期直接引入 Neo4j。先用关系表保存节点、边和步骤序列，只有图遍历成为明确瓶颈
后再引入独立图数据库。

## 9. 多任务调度

调度器需要支持：

- 优先级队列；
- 项目、租户和全局并发上限；
- Worker 租约；
- 幂等领取；
- 心跳和超时回收；
- 暂停、恢复、取消和重试；
- 不可重试工具策略；
- 执行窗口；
- Worker 能力标签；
- 资源配额；
- 服务重启后的确定性恢复。

数据库是任务状态事实来源，NATS 只负责通知，不以消息是否存在判断最终任务状态。

## 10. 计量与成本

需要计量：

- LLM 请求、输入 Token、输出 Token 和费用；
- 工具调用次数、耗时、成功率和资源使用；
- 项目、租户、模型和角色维度成本；
- MinIO 存储量；
- 活跃 Run 和 Worker 时间；
- 报告生成和导出次数。

所有费用计算保存计价版本、模型名称和供应商，避免供应商价格调整后无法解释历史账单。

## 11. 权限模型

建议权限域：

```text
pent:project:read/write/admin
pent:task:read/create/cancel
pent:run:read/create/cancel
pent:approval:read/decide
pent:asset:read/write
pent:vulnerability:read/triage/export
pent:evidence:read/download
pent:traffic:read
pent:guardrail:read/write
pent:memory:read/feedback/admin
pent:tool:read/admin
pent:workspace:read/delete
pent:model:read/admin
```

除权限码外，必须校验租户、项目成员关系、资产数据范围和授权有效期。

## 12. 分阶段实施

### 第一阶段：真实执行闭环

- 项目与不可变授权范围；
- 对话式 Agent Session；
- 自然语言目标、范围、约束和预算提取；
- Scope Proposal 结构化确认；
- 用户离线后的后台自主执行；
- Run 中追加指令和协作消息；
- 任务、Run 和 Worker 租约；
- Agent 基础状态机；
- SSE 事件持久化与重放；
- 工具注册、调用和取消；
- Evidence 原文、哈希、有界预览和分页查看；
- HTTP Traffic 列表、查看和受控重放；
- 完全重复工具调用抑制；
- 高风险审批；
- 资产、证据和漏洞入库；
- 逐结论 Evidence Gate；
- 事实投影生成的基础 HTML 报告；
- 全链路标识。

完成标准：可以在授权范围内创建任务，后台执行工具，处理审批，生成证据和漏洞，并
从漏洞追踪回完整 Run。

### 第二阶段：运营页面

- 完整仪表盘；
- 漏洞生命周期；
- 资产列表和九页签详情；
- LLM/HTTP 流量日志；
- 拦截中心；
- 工具健康状态；
- 工作区文件；
- HTML/DOCX 报告。

### 第三阶段：Agent 智能化

- 多模型供应商；
- 模型角色绑定；
- Planner/Perception/Reasoner/Reviewer 协作；
- `ASK_USER`、`NO_PATH`、`FINAL` 和 `REPLAN` 统一动作；
- 跨 Turn 的重复响应、证据空转和失败模式识别；
- 高信号事实提取与近成功防误停；
- 漏洞自动研判；
- 置信度计算；
- 端点归一化和去重；
- Skill Manifest、按需引用和版本审计；
- Token 与成本计量；
- 模型降级和预算控制。

### 第四阶段：RMOS 与攻击图谱

- pgvector 语义记忆；
- 攻击步骤和路径记忆；
- 历史路径召回；
- 人工反馈；
- 图谱展示；
- 执行回放；
- 记忆过期、禁用和隔离策略。

### 第五阶段：规模化

- 工具数量逐步扩展到目标规模；
- Worker 弹性扩缩容；
- 大规模日志全文检索；
- 独立图数据库评估；
- 多租户配额和成本结算；
- 高可用和故障演练。

## 13. 测试策略

### 领域测试

- Scope Validator；
- Agent 状态机；
- 工具策略和审批；
- 漏洞生命周期；
- 置信度计算；
- 记忆隔离；
- 计价规则。

### 集成测试

- PostgreSQL Repository；
- Redis 租约和限流；
- NATS 重复消息；
- MinIO 权限和对象生命周期；
- Worker 崩溃恢复；
- SSE 重连和事件重放；
- 模型超时和降级。

### 安全测试

- 越权和跨租户访问；
- 授权范围逃逸；
- DNS Rebinding；
- SSRF；
- 命令和参数注入；
- 路径穿越；
- 恶意工具输出和提示词注入；
- 密钥、Token、Cookie 和密码泄漏；
- 未审批高风险调用；
- 紧急停止有效性。

### 端到端测试

```text
创建项目和授权范围
  -> 创建任务
  -> 创建 Run
  -> 观察实时事件
  -> 处理人工审批
  -> 工具执行完成
  -> 查看资产与流量
  -> 查看证据和漏洞
  -> 复测与定级
  -> 生成报告
  -> 回放攻击路径
```

## 14. 完成标准

平台首个可用版本必须满足：

1. 所有主动测试都绑定有效项目授权范围。
2. 浏览器关闭后 Run 继续执行。
3. Worker 崩溃不会造成同一工具调用并发重复执行。
4. 高风险操作必须经过策略允许或人工审批。
5. Run 事件可持久化、重放和断线恢复。
6. 资产、流量、证据和漏洞形成可追溯关系。
7. 漏洞支持完整生命周期、人工复核和复测。
8. 大型产物进入 MinIO，不使用不受控本地绝对路径。
9. 模型密钥加密存储且前端不回显。
10. 计量结果可以按租户、项目、模型和工具统计。
11. 通过领域、集成、安全和关键端到端测试。
12. 数据库变更按仓库要求重新导出并验证完整初始化快照。
13. “已验证漏洞”中的每项结论都能解析到 E3/E4 Evidence，报告复现数据不由模型虚构。
14. 完整工具输出不因模型上下文裁剪而丢失，并可按权限分页检索和查看。
15. 相同工具请求、相同响应和重复证据读取不会形成无上限空转。
16. `NO_PATH`、预算耗尽、授权阻塞和工具不可用分别形成可区分的 Run 终态与审计事件。
17. Skill、Memory 和模型推测不能脱离真实 Evidence 升级为已验证事实。

## 15. 与 Pent 服务迁移方案的关系

本文定义最终产品能力、页面和领域边界；现有
[Pent Agent Web 微服务迁移方案](./pent-agent-service-plan.md)
定义从本地 Agent 工作台迁移到 Kairos Platform 服务架构的技术过程。

实施时以本文确定产品范围，以迁移方案确定服务拆分、协议、部署和演进步骤。若两份
文档出现实现顺序冲突，优先保证“授权范围、工具隔离、人工审批、证据审计”组成的
安全执行闭环。
