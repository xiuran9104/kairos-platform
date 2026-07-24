---
type: reference
title: PentAGI Agent Architecture Plan
tags: [pentagi, agent-runtime, claude-code, claw-code, llmwiki, architecture]
related: [references/index, references/claude-code-agent-design]
---

# PentAGI Agent Architecture Plan

## 目标定位

PentAGI 不是普通聊天机器人，也不是直接复用 Codex。它的定位是一个
Claude Code 风格的工程化 AI 自动化渗透 Agent：

1. 用户只面对一个主 Agent 会话入口。
2. Agent 内部通过模型、工具、权限、上下文、审计日志完成任务闭环。
3. 普通 CLI 不暴露大量工具命令；工具是 Agent 内部能力。
4. LLMWiki 是长期知识层，负责沉淀方法、工具说明、目标笔记、报告证据和复盘。
5. 后续可以接入 MCP、TUI、多 Agent、任务队列和自动报告，但核心 loop 先稳定。

参考对象：

- Claude Code / deep-dive-claude-code：对话、工具、权限、上下文、MCP、状态管理的主线。
- Claw Code：Rust CLI agent harness、health check、provider setup、mock parity harness、事件/报告契约。
- geronimo-llm-wiki：Markdown 源文件 + Tantivy 索引 + schema 驱动的知识库方向。

## 实现顺序

严格按下面顺序推进，避免先做高级能力导致返工：

1. Agent 对话骨架：message、session、model、agent、store。
2. 工具调用闭环：tool schema、registry、executor、tool result 回灌。
3. 基础错误处理：参数校验、异常归一、超时、重试、失败可恢复。
4. 上下文、权限、安全：LLMWiki 检索注入、压缩、审批、审计、沙箱边界。
5. 高级能力：自动修复、多 Agent、TUI、MCP 插件、报告生成、任务编排。

当前项目已经进入第 4 步初始阶段：工具闭环、权限、审计、LLMWiki 检索注入已经有基础实现。

## 工作区结构

建议长期保持这个分层：

```text
PentAGI/
  crates/
    pentagi-core/      # Agent 核心库，不绑定 CLI UI
    pentagi-cli/       # 命令行入口，负责交互、审批、参数解析
  wiki/                # LLMWiki Markdown 知识库，长期知识源
  schema/              # 系统 prompt、规范、未来 schema
  scripts/             # 一次性迁移、编译、知识导入脚本
  raw/                 # 原始资料、payload、字典、工具资料
```

核心原则：

- `pentagi-core` 不知道终端 UI，只暴露模型、Agent、工具、权限、审计、上下文能力。
- `pentagi-cli` 只负责把用户输入、审批和显示接到 core。
- `wiki/` 是源数据，不把知识写死进 Rust。
- `target/`、索引缓存、会话缓存都是派生数据，不作为知识源。

## Core 模块边界

```text
pentagi-core
  message     消息结构、角色、tool_call、tool_result
  session     多轮会话、系统提示、消息历史
  model       多模型适配，Anthropic-compatible / OpenAI-compatible
  agent       单轮/多轮工具调用编排
  tool        工具协议、注册表、执行接口
  tools       内置工具：fs、notebook、shell、todo、web、task、mcp placeholder
              以及独立 pentest 工具层
  permission  能力分级、审批决策、默认安全策略
  context     上下文压缩、LLMWiki 检索注入
  llmwiki     Markdown 扫描、Tantivy BM25 索引、搜索
  store       会话持久化
  audit       JSONL 审计日志
```

模块依赖方向：

```text
CLI -> Agent -> Model
           -> ToolRegistry -> ToolExecutor
           -> PermissionPolicy
           -> ContextManager -> LlmWiki
           -> SessionStore / AuditLogger
```

不要让工具层反向调用 CLI；不要让模型层知道文件系统工具；不要让 LLMWiki 绑定某个 UI。

## Agent Loop 模式

运行时采用 Claude Code 风格的 tool-first loop：

```text
user input
  -> ContextManager.prepare_turn
  -> model.complete_chat_with_tools(messages, tool_definitions)
  -> assistant text or tool_calls
  -> if tool_calls:
       permission check
       approval if needed
       execute tool
       append tool_result
       repeat model call
  -> final assistant answer
  -> persist session
  -> append audit events
```

关键约束：

- 读工具默认允许。
- 写文件、命令执行、MCP、网络、破坏性动作默认需要审批或明确 allow flag。
- 工具失败不能让进程崩溃，要变成 `tool_result` 错误交给模型继续修正。
- 每次工具调用都写审计日志，后续报告系统直接消费 audit JSONL。

当前 Agent runtime 已具备：

- `AgentRunConfig`：统一配置最大工具轮次、模型重试次数、重试延迟。
- 稳定 `AgentEvent` JSON contract：事件使用 tagged JSON，audit 直接记录事件结构。
- mock parity harness：`MockParityModel` 可固定模型输出和工具轨迹，不依赖真实 API。
- `pentagi doctor`：检查 workspace、session/audit store、provider 配置、wiki index、外部工具。
- 真正流式事件接口：`run_tool_loop_stream` 按模型/权限/工具事件逐步 yield。
- 取消 token：`CancellationToken` 可让 Agent 在模型调用前或工具执行前停止。
- 后台 Bash session：`bash background=true` 返回 `bash_id`，`BashOutput` 读取输出，`KillBash` 终止任务。
- Claude Code 第 10 章风格 Agent 模型：`AgentDefinition`、`AgentCatalog`、内置 Agent、Markdown/JSON 定义解析、工具白/黑名单过滤。
- 子 Agent 编排：`SubAgentOrchestrator` 支持并发运行多个子任务；CLI chat 会把 `Task` 工具替换成模型绑定的子 Agent runner，并按 `agent_type` 路由到内置 Agent。
- 模型临时失败重试：失败事件进入 `AgentEvent::ModelCallRetry`，CLI 和 audit 可记录。
- 工具错误回灌：unknown tool、参数解析错误、执行异常都转成失败 `tool_result`，不会直接打断会话。
- 权限拒绝回灌：拒绝执行也作为失败 `tool_result` 返回模型，模型可以改用更安全方案。
- 工具轮次上限：超过上限后写入 assistant 终止消息，避免无限循环。

## CLI 模式

面向用户保持最小表面：

```bash
pentagi chat
```

内部诊断命令可以存在，但默认隐藏：

```bash
pentagi tool ...
pentagi wiki ...
```

下一阶段建议增加一个公开健康检查命令：

```bash
pentagi doctor
```

`doctor` 不执行渗透动作，只检查：

- Cargo/Rust 构建是否正常。
- 当前 provider 是否配置 key/base/model。
- 选定模型是否可用。
- session store 是否可写。
- audit root 是否可写。
- wiki root 是否存在。
- Tantivy index 是否存在、是否过期、是否可重建。
- shell / pwsh / 常用安全工具是否存在。
- 当前权限策略是否符合预期。

## 多模型适配模式

模型层只抽象两类传输协议：

1. Anthropic-compatible Messages API。
2. OpenAI-compatible Chat Completions API。

Provider 只是预设配置：

```text
claude              -> Anthropic-compatible
deepseek-anthropic  -> Anthropic-compatible
deepseek            -> OpenAI-compatible
glm                 -> OpenAI-compatible
qwen                -> OpenAI-compatible
siliconflow         -> OpenAI-compatible
openrouter          -> OpenAI-compatible
ollama              -> OpenAI-compatible
```

配置优先级：

1. CLI 参数。
2. `PENTAGI_*` 环境变量。
3. provider 官方环境变量。
4. `~/.claude/settings.json` 中的 Anthropic 风格配置。
5. provider 默认值。

Agent loop 不区分模型品牌，只消费统一的：

```text
ChatOutput {
  content,
  tool_calls
}
```

## 工具模式

当前分两类工具：Claude-compatible 通用工具层，以及 PentAGI pentest
领域工具层。PentAGI 同时暴露
snake_case 原生工具和 Claude Code PascalCase 兼容别名，工具定义数量已经是
40+。已实现/注册的通用工具集合：

```text
read_file / read       读文件
write_file / write     写文件
list_dir / ls          列目录
search_files           字符串搜索
glob                   glob 文件匹配
grep                   regex 文本搜索
edit_file / edit       精确字符串替换
multi_edit             多段精确字符串替换
notebook_read          Jupyter notebook cell 摘要
notebook_edit          替换单个 notebook cell source
bash                   Bash 执行
Bash                   Claude Code 兼容别名
bash_output            后台 Bash 输出占位
BashOutput             Claude Code 兼容别名
kill_bash              后台 Bash 终止占位
KillBash               Claude Code 兼容别名
powershell             PowerShell 执行
todo_read              读取 Agent checklist
TodoRead               Claude Code 兼容别名
todo_write             更新 Agent checklist
TodoWrite              Claude Code 兼容别名
web_fetch              HTTP(S) 抓取
WebFetch               Claude Code 兼容别名
web_search             Web 搜索抓取
WebSearch              Claude Code 兼容别名
task                   子 Agent 占位，后续接真实 orchestration
Task                   Claude Code 兼容别名
exit_plan_mode         退出计划模式兼容工具
ExitPlanMode           Claude Code 兼容别名
mcp_call               MCP 占位，后续接真实 transport
```

`task` 在默认工具注册表里仍可作为占位存在；在真实 chat runtime 中会通过
`tools::task::register_subagent` 绑定当前模型，支持：

```json
{
  "agent_type": "explore",
  "description": "analyze target surface",
  "tasks": [
    { "agent_type": "explore", "description": "inspect routes", "prompt": "focus on web routes" },
    { "agent_type": "verification", "description": "inspect auth", "prompt": "focus on auth paths" }
  ],
  "max_tool_rounds": 3
}
```

每个子 Agent 使用独立 `Session`，并发执行，返回各自事件和最终消息。
内置 Agent 当前包括：

```text
general-purpose
explore
plan
claude-code-guide
verification
```

Agent 定义支持 `tools`、`disallowed_tools`、`model`、`effort`、
`permission_mode`、`max_turns`、`memory`、`isolation`、`omit_claude_md`
等字段。当前已实现定义解析、内置 catalog、工具过滤和 `max_turns` 生效；
不同模型路由、plugin/user/project 覆盖扫描、hooks 执行仍是下一步。

Pentest 工具层已经单独建模，后续具体实现都放在 `tools/pentest/`，不混入
Claude-compatible 工具层：

```text
tools/pentest/
  scope.rs       授权目标范围
  external.rs    外部工具适配器入口
  evidence.rs    证据归档入口
  workflow.rs    流程化渗透工具入口
```

当前已注册的 pentest 工具：

```text
pentest_scope_set
pentest_scope_get
pentest_tool_check
nmap_scan
ffuf_fuzz
nuclei_scan
sqlmap_scan
http_probe
recon_basic
web_discovery
vuln_scan_basic
evidence_collect
report_draft
```

除 `pentest_scope_*` 和 `pentest_tool_check` 外，其他 pentest 工具目前是架构占位：
已经有参数 schema、权限能力、scope 前置校验和注册入口，但还没有调用真实外部
工具。Agent runtime 容错稳定后，再逐个实现 `nmap_scan`、`http_probe`、
`ffuf_fuzz`。

文件编辑必须保持精确语义：

- `old_string` 找不到：失败。
- 默认多处匹配：失败。
- 只有显式 `replace_all=true` 才允许批量替换。
- 路径必须限制在 workspace root 内。

Shell 执行必须有边界：

- 默认需要审批。
- 有超时。
- stdout/stderr 截断。
- 后续加入危险命令识别、网络动作识别和沙箱运行器。

MCP 属于工具调用，但不应该直接混在普通函数里失控执行。它应通过统一工具协议进入 Agent loop，受权限、审计和超时控制。

## 权限和安全模式

能力分级：

```text
Read         默认允许
Write        默认询问
Exec         默认询问
Network      默认询问或按任务 scope 决定
Destructive  默认拒绝或强审批
Mcp          默认询问
```

审批返回：

```text
allow once
allow always for this tool in current session
deny
```

安全原则：

- 任何实际渗透动作都必须绑定授权 scope。
- 工具输出和证据进入 audit，不只进入聊天历史。
- 失败、拒绝、超时都要作为可恢复事件进入 Agent loop。
- 不让模型直接绕过工具层访问文件系统或执行命令。

## LLMWiki 模式

LLMWiki 的源数据是 Markdown，不是数据库。索引是派生缓存。

当前模式：

```text
wiki/*.md
  -> Markdown chunk
  -> Tantivy BM25 index
  -> search(query)
  -> ContextManager 注入相关片段
```

当前 LLMWiki index 已具备：

- `WikiIndexStatus`：`Missing` / `Fresh` / `Stale` / `Corrupt`。
- `state.json`：记录 file count、content hash、built_at。
- `ensure_index`：搜索前按需重建，不再每次全量 rebuild。
- 损坏恢复：索引打开失败时自动 rebuild 后重试搜索。
- 动态 schema 发现：读取 `wiki.toml` 的 `[types]` 和 `schemas/*.json`，通过 `WikiConfigSummary` 暴露。
- LLM 总结式压缩接口：`ContextSummarizer` / `LlmContextSummarizer` 已可接模型生成摘要，默认仍用本地 extractive summarizer。
- 任务级/目标级记忆：`MemoryStore` 已支持 task memory 和 target memory 的 JSON 文件读写。

下一步对齐 geronimo-llm-wiki：

1. 将 `schemas/*.json` 字段映射进 Tantivy 动态索引。
2. 增量更新单文件 chunk，不只做按需全量 rebuild。
3. 将 memory 与 LLMWiki 页面/目标 scope 关联。
4. 后续暴露 MCP search/read/list 工具。

知识库内容分层：

```text
wiki/tools/        工具说明和用法
wiki/skills/       方法论和操作流程
wiki/playbooks/    任务剧本
wiki/vulns/        漏洞知识
wiki/history/      项目记忆和复盘
wiki/references/   外部设计参考
```

## 审计和报告模式

Audit JSONL 是后续报告系统的事实来源：

```text
user_input
assistant_message
permission_requested
tool_call_requested
tool_call_finished
error
```

后续报告生成不应该从聊天文本里猜，而应从 audit events 中抽取：

- 执行了什么工具。
- 输入参数是什么。
- 输出证据是什么。
- 哪些动作被拒绝或失败。
- 哪些结论由哪些证据支撑。

未来可以增加：

- audit replay
- finding extraction
- evidence normalization
- Burp-style request/response 归档
- Markdown / HTML / JSON 报告输出

## 测试模式

必须建立三层测试：

1. Unit tests：消息、工具、权限、路径解析、LLMWiki chunk/search。
2. Mock parity harness：固定模型响应、固定 tool_call、固定 tool_result，验证 Agent loop。
3. Live smoke tests：只在显式配置 key 时跑，验证 provider 可用。

不要依赖真实模型来证明基础逻辑正确。真实模型只验证 provider 兼容性和端到端冒烟。

## 下一阶段路线

短期：

1. 增加 `pentagi doctor`。
2. 增加 mock parity harness。
3. 完善 `AgentEvent` 稳定 JSON contract。
4. 增加工具失败的标准错误结构。
5. 增加命令执行危险动作识别。

中期：

1. LLMWiki 对齐 `wiki.toml` 和 `schemas/*.json`。
2. 增量索引和索引状态。
3. MCP stdio transport。
4. 任务 scope 和授权边界。
5. 报告生成 pipeline。

长期：

1. TUI。
2. 子 Agent / 多 Agent。
3. 自动修复和计划-执行-验证循环。
4. 沙箱执行器。
5. 面向授权渗透的任务队列和证据库。

## 架构底线

PentAGI 的底线是：

- 主入口像 Claude Code 一样简单。
- 内部能力像工程系统一样分层。
- 工具调用必须可审计、可审批、可恢复。
- 模型可替换，Agent loop 不绑定具体厂商。
- Markdown LLMWiki 是长期知识源，索引只是缓存。
- 先把单 Agent 工具闭环打稳，再做多 Agent 和 TUI。
