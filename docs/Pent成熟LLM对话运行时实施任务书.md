# Pent 成熟 LLM 对话运行时实施任务书

## 1. 目标

将当前“消息只入库、Worker 不消费、 Pent 绕过 AI Server 调模型、前端展示原始 JSON”的
链路替换为可流式、可恢复、可审计、可控制 Run 的 Agent Conversation Runtime。

```text
Web / CLI
  -> AgentTurnRequest
  -> ConversationRuntime
     -> ContextSource
     -> IntentDetector
     -> ModelGateway
     -> InstructionSink
     -> Repository
     -> EventSink
  -> AgentTurnResult / SSE
```

## 2. 已建立的稳定框架

### API

`crates/modules/pent-api/src/conversation.rs` 已定义：

- `AgentTurnRequest`、`AgentTurnResult`
- `ConversationMessageType`、`ConversationTurnStatus`
- `ConversationIntent`
- `ConversationContext`
- `AgentMessage`
- `ConversationPlanUpdate`
- `ConversationCitation`
- `ModelUsage`
- `ConversationTurnError`

后续实现不得以任意 `serde_json::Value` 取代这些主协议。

### Core

`crates/modules/pent-core/src/conversation/` 已提供：

- 无模型时的确定性意图分类；
- 上下文 Token 估算与最近消息裁剪；
- `AgentTurnResult` 结构校验。

### Server

`crates/modules/pent-server/src/conversation/` 已定义：

- `ConversationRepository`
- `ConversationContextSource`
- `ConversationIntentDetector`
- `ConversationModelGateway`
- `ConversationInstructionSink`
- `ConversationEventSink`
- `ConversationRuntime`

Runtime 只负责编排，不能包含 SQL、Provider HTTP 细节或 Vue 展示逻辑。

### Database

唯一初始化快照 `sql/postgresql/0001_initial.sql` 已包含：

- `pent_conversation_turns`
- `pent_context_snapshots`
- `pent_model_role_bindings`
- `pent_model_calls`
- Conversation Message 的 Turn、状态、格式、引用、用量等扩展字段

## 3. 强制规则

1. Pent 必须复用 `AiModelFactory`，不得自行请求 `/chat/completions`。
2. 用户消息使用 `client_request_id` 保证幂等。
3. Run 活跃时，steer/summary/review/stop 必须进入 `pent_run_instructions`。
4. 模型输出先做 Schema 校验，再做 Scope/Policy 校验，最后才能更新计划。
5. 原始工具 stdout、密钥、Cookie 和认证 Header 不进入 Prompt。
6. SSE 事件必须先持久化再推送，并支持 sequence 恢复。
7. 模型失败必须返回明确错误，禁止伪装成成功的模拟回复。
8. 不保存完整 reasoning，只允许保存安全的 reasoning summary。
9. 每次模型调用记录模型角色、Token、耗时、费用、重试和错误码。
10. Tool Schema 每轮最多注入 8 个。

## 4. 可分配任务

### LLM-01：PostgreSQL ConversationRepository

负责：

```text
crates/modules/pent-server/src/conversation/repository_postgres.rs
```

实现 begin/complete/fail，保证：

- `(session_id, client_request_id)` 幂等；
- user/assistant message 与 Turn 事务一致；
- 状态只能按合法状态机转换；
- 重试不会生成重复 assistant message。

验收：并发重复请求只创建一个 Turn。

### LLM-02：ContextSource

负责：

```text
crates/modules/pent-server/src/conversation/context_source.rs
crates/modules/pent-core/src/conversation/context_builder.rs
```

上下文分层：

```text
system instruction
objective/scope/policy/budget snapshot
run summary
最近 12 条消息
相关 evidence
RMOS Top-K
工具路由 Top-K
```

超过预算时优先保留 Scope、Policy、未处理指令和最新消息；旧内容先摘要再裁剪。每次保存
`pent_context_snapshots` 和 content hash。

### LLM-03：统一 ModelGateway

负责：

```text
crates/modules/pent-server/src/conversation/model_gateway.rs
crates/modules/pent-server/src/conversation/model_router.rs
```

使用 `AiModelFactory`，支持模型角色：

```text
intent
conversation
planner
reviewer
guard
summarizer
```

从 `pent_model_role_bindings` 解析项目级配置和回退模型。实现超时、有限重试、熔断及
`pent_model_calls` 计量。删除 `penta-server/src/lib.rs` 中直接 Reqwest 的
`ai_analyze()`。

### LLM-04：结构化意图和输出

负责：

```text
crates/modules/pent-core/src/conversation/intent.rs
crates/modules/pent-core/src/conversation/output_validator.rs
crates/modules/pent-server/src/conversation/structured_output.rs
```

模型只输出结构化 Intent/PlanUpdate。JSON 不合法时最多执行一次修复请求，仍失败则使用
确定性分类或返回 `MODEL_OUTPUT_INVALID`。模型不能直接生成最终 Tool argv。

### LLM-05：Run Instruction Adapter

负责：

```text
crates/modules/pent-server/src/conversation/instruction_sink.rs
```

映射：

| 意图 | Run Instruction |
|---|---|
| Steer | `steer` |
| ProvideContext | `provide_context` |
| RequestSummary | `request_summary` |
| RequestReview | `request_review` |
| Stop | `stop` |

Worker 在每个工具步骤前消费 pending 指令，并产生 applied/rejected 事件。Stop 和扩大目标
范围必须要求明确确认，指令不能修改固化的 Scope Snapshot。

### LLM-06：SSE 与恢复

负责：

```text
crates/modules/pent-server/src/conversation/stream.rs
crates/modules/pent-server/src/conversation/http.rs
```

接口：

```text
POST /pent/sessions/{id}/turns
GET  /pent/sessions/{id}/turns
GET  /pent/sessions/{id}/events
POST /pent/turns/{id}/cancel
POST /pent/turns/{id}/retry
```

事件：

```text
conversation.turn.created
conversation.intent.detected
conversation.context.ready
conversation.message.delta
conversation.message.completed
conversation.turn.failed
conversation.turn.cancelled
conversation.instruction.created
model.request.started
model.request.completed
model.request.failed
```

支持 `Last-Event-ID`、心跳、断线补发、慢客户端和取消生成。

### LLM-07：前端结构化对话

负责：

```text
apps/web/apps/web-antd/src/api/pent/conversation.ts
apps/web/apps/web-antd/src/views/pent/agent-session/
```

组件：

```text
AgentMessageCard
ToolCallCard
PlanningCard
FindingCard
ApprovalCard
TurnErrorCard
ModelUsagePopover
```

发送时先生成 client_request_id 并乐观显示；SSE delta 更新同一消息；刷新后从数据库恢复。
不得继续用 `JSON.stringify(event.payload)` 作为主要 UI。

### LLM-08：摘要、记忆与质量评估

负责：

```text
crates/modules/pent-core/src/conversation/summarizer.rs
crates/modules/pent-server/src/conversation/memory_adapter.rs
```

实现阶段摘要、长会话压缩、RMOS 召回和引用。评估指标包括意图准确率、结构化输出成功率、
指令应用率、用户重试率、首 Token 延迟、总延迟、Token 和成本。

## 5. 错误码

```text
MODEL_UNAVAILABLE
MODEL_TIMEOUT
MODEL_OUTPUT_INVALID
CONTEXT_TOO_LARGE
SCOPE_NOT_CONFIRMED
RUN_NOT_ACTIVE
INSTRUCTION_REJECTED
POLICY_DENIED
BUDGET_EXHAUSTED
STREAM_INTERRUPTED
TURN_CANCELLED
```

错误返回必须携带 `turn_id` 和 `retryable`。

## 6. 集成顺序

```text
LLM-01 Repository
  -> LLM-02 Context
  -> LLM-03 ModelGateway
  -> LLM-04 Structured Output
  -> LLM-05 Instruction
  -> LLM-06 SSE
  -> LLM-07 UI
  -> LLM-08 Summary/RMOS
```

LLM-02、LLM-03、LLM-05 可在稳定契约上并行，但不同代理不得同时修改同一文件。

## 7. Definition of Done

1. 发送消息会产生真实 assistant 回复。
2. Pent 不再直接访问模型 HTTP Endpoint。
3. 运行中的 steer 会被 Worker 消费。
4. 重复 client_request_id 不产生重复回复。
5. SSE 断开后可恢复且不重复 delta。
6. 模型失败不会返回模拟成功文本。
7. 规划输出经过 Schema、Scope 和 Policy 校验。
8. Prompt 不包含密钥和未脱敏原始输出。
9. Session、Run、模型维度的 Token、费用、耗时可统计。
10. 长上下文自动摘要且不丢 Scope/Policy。
11. 前端显示结构化卡片而不是原始事件 JSON。
12. Rust、数据库、前端类型检查和生产构建全部通过。

## 8. 验证

```bash
cargo test -p kairos-platform-pent-api -p kairos-platform-pent-core -p kairos-platform-pent-server
bash script/test-database-migrations.sh
pnpm --dir apps/web --filter @vben/web-antd run typecheck
pnpm --dir apps/web --filter @vben/web-antd run build
git diff --check
```
