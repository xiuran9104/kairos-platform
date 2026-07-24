# Pent H-PER-M 认知运行时实施任务书

## 1. 目标架构

H-PER-M 表示：

```text
Hierarchical Planner
  + PreFlect
  + Deterministic Executor
  + Dynamic Replanner
  + Multi-level Reflector
  + Metacognitive Monitor
```

完整循环：

```text
Meta-Planner 创建 TCG
  -> Stage Planner 展开阶段
  -> Step Planner 提出 ActionProposal
  -> PreFlect 评估必要性、信息增益和风险
  -> Policy Gate 确定性准入
  -> ToolOrchestrator 执行
  -> ToolExecutionResult
  -> Action Reflector
  -> Meta Monitor
  -> Replanner 生成 PlanDelta
  -> Stage/Run Reflector
  -> 验证后的 Lesson 写入 RMOS
```

## 2. 已建立的稳定框架

### API

`crates/modules/pent-api/src/cognition.rs` 已定义：

- `TaskCoordinationGraph`
- `PlanNode`、`PlanEdge`、`PlanBudget`
- `PlanDelta`、`PlanDeltaOperation`、`ReplanLevel`
- `ActionProposal`
- `PreFlection`
- `ActionReflection`
- `MetaMetrics`
- `MetaCognitiveSnapshot`
- `MetaIntervention`

### Core

`crates/modules/pent-core/src/cognition/` 已实现：

- DAG 节点、边和循环校验；
- 基于 `base_version` 的 PlanDelta 应用；
- 越界、预算、循环、冲突和低置信度的元认知基线；
- 重复动作、预算和 Scope 的确定性 PreFlect 基线。

### Server

`crates/modules/pent-server/src/cognition/` 已定义：

- `PlanRepository`
- `StepPlanner`
- `PreReflector`
- `CognitiveExecutor`
- `ActionReflector`
- `DynamicReplanner`
- `MetaMetricsSource`
- `CognitionEventSink`
- `CognitionRuntime`

### Database

唯一初始化快照 `sql/postgresql/0001_initial.sql` 已包含：

- `pent_plan_versions`
- `pent_plan_deltas`
- `pent_pre_reflections`
- `pent_reflections`
- `pent_metacognitive_snapshots`
- `pent_agent_lessons`

## 3. 不可变架构规则

1. TCG 是计划事实来源，不能维护第二套可执行计划。
2. PlanDelta 是唯一计划修改方式，模型不能覆盖原计划 JSON。
3. Agent 规划 capability，ToolRouter 选择 tool/version。
4. PreFlect 可建议，但不能覆盖 Policy Gate。
5. Executor 不使用 LLM，只执行已经批准的 ToolInvocation。
6. 所有 Observation 使用 `ToolExecutionResult`。
7. Meta Monitor 的安全、预算和停止判断以确定性代码为主。
8. 反思只保存结构化结论和 reasoning summary，不保存私有思维链。
9. RMOS 只接收验证过的 Lesson，单次模型自评不能直接晋升长期记忆。
10. 新发现目标超出 Scope 时只能隔离记录，不能自动扩大授权。

## 4. TCG 约束

节点层级：

```text
Stage
  -> Task
     -> Action
```

边类型：

```text
requires
conditional
produces
invalidates
```

节点状态：

```text
pending -> ready -> running -> completed
                          |-> failed
                          |-> blocked
completed -> stale
```

上游 Observation 失效时，依赖节点必须标记 `stale`，不能继续使用旧 completed 结果。

渗透五阶段只是模板：

```text
资产侦察 -> 爬虫枚举 -> 漏洞扫描 -> 受控验证 -> 报告
```

阶段按条件激活。漏洞利用、写入和持久化操作默认不自动启用。

## 5. 可分配任务

### HPM-01：PostgreSQL PlanRepository

负责：

```text
crates/modules/pent-server/src/cognition/plan_repository_postgres.rs
```

实现 current/save_initial/apply_delta 和反思、Meta Snapshot 持久化。应用 Delta 必须：

1. 锁定当前 plan version；
2. 检查 base version；
3. 调用 Core DAG 校验；
4. 检查 Scope、预算和循环；
5. 原子写入新版本和 Delta 状态；
6. 并发冲突返回 `PLAN_VERSION_CONFLICT`。

### HPM-02：Meta-Planner

负责：

```text
crates/modules/pent-server/src/cognition/meta_planner.rs
crates/modules/pent-core/src/cognition/plan_validator.rs
```

输入 Objective/Scope/Policy/Budget、历史 RMOS 路径和可用命名空间，输出初始 TCG。模型输出
必须经过 JSON Schema、DAG、Scope 和预算校验。失败时使用只读五阶段模板降级。

### HPM-03：Stage Planner

负责：

```text
crates/modules/pent-server/src/cognition/stage_planner.rs
```

每次只展开一个激活 Stage，最多接收 8 个 Top-K capability/tool schema。输出 Task/Action
子图和成功标准，不得生成具体容器命令。无 Web 服务时不得激活 crawler 阶段。

### HPM-04：Step Planner 与 ToolRouter Adapter

负责：

```text
crates/modules/pent-server/src/cognition/step_planner.rs
crates/modules/pent-server/src/cognition/tool_router_adapter.rs
```

从 Ready Action 生成单个 ActionProposal，经 ToolRouter 选择具体工具，再生成 ToolInvocation。
参数模板必须经过 Schema 验证；LLM 不能传递 Shell 字符串、镜像名或远程 Endpoint。

### HPM-05：PreFlect

负责：

```text
crates/modules/pent-server/src/cognition/preflector.rs
crates/modules/pent-core/src/cognition/preflect.rs
```

顺序：

```text
确定性重复/预算/Scope/风险检查
  -> 必要时快速模型预测结果和替代方案
  -> Policy Gate
```

低风险确定性动作可跳过模型。每个 Action 最多一次模型 PreFlect，避免反思不执行。

### HPM-06：Executor Adapter

负责：

```text
crates/modules/pent-server/src/cognition/executor_adapter.rs
```

只把批准后的 ActionProposal 转成 ToolOrchestrator 调用，并把结果映射回 Action Observation。
必须记录 plan/action/tool call 关联、镜像 digest、策略决策和证据引用。

### HPM-07：动态 Replanner

负责：

```text
crates/modules/pent-server/src/cognition/replanner.rs
crates/modules/pent-core/src/cognition/replan_trigger.rs
```

确定性触发器：

- 连续失败；
- 低信息增益或高重复度；
- 工具失败、矛盾观察；
- 新资产或高危发现；
- 用户 steer；
- 预算压力；
- Meta Monitor 干预。

分级处理：

| Level | 处理 |
|---|---|
| L1 Parameter | 修改当前动作参数 |
| L2 Tool | 替换同能力工具 |
| L3 Stage | 修改当前 Stage 子图 |
| L4 Global | Meta-Planner 全局调整 |

输出只能是 PlanDelta。

### HPM-08：多级 Reflector

负责：

```text
crates/modules/pent-server/src/cognition/reflector/
  action.rs
  stage.rs
  run.rs
  aggregator.rs
```

- Action Critic：工具选择、结果质量、信息增益；
- Stage Critic：覆盖率、成功标准、遗漏；
- Run Critic：目标完成、证据、成本和安全；
- Safety Critic：Scope 和破坏性风险。

Critic 只读取对应摘要。Aggregator 处理冲突，不能使用简单平均掩盖 Safety Critical。

### HPM-09：元认知监控

负责：

```text
crates/modules/pent-server/src/cognition/meta_metrics.rs
crates/modules/pent-core/src/cognition/metacognition.rs
```

指标：

```text
plan_confidence
observation_confidence
evidence_coverage
information_gain
loop_similarity
failure_streak
budget_pressure
scope_risk
model_disagreement
```

安全优先级最高，其次预算、循环、冲突和低置信度。实现 Continue/Gather/Replan/Ask/
Approval/Pause/Stop 干预。

### HPM-10：RMOS Lesson Pipeline

负责：

```text
crates/modules/pent-server/src/cognition/lesson_pipeline.rs
crates/modules/pent-server/src/cognition/memory_adapter.rs
```

Lesson 生命周期：

```text
candidate -> validated -> promoted
                    |-> rejected
```

至少经过人工确认或多次跨 Run 验证才能 promoted。召回时按目标指纹、技术栈、阶段和能力
过滤，不能把旧目标参数直接复用到新 Scope。

### HPM-11：认知观测 UI

负责：

```text
apps/web/apps/web-antd/src/views/pent/cognition/
```

展示：

- TCG DAG 和 Plan Version；
- PlanDelta 前后差异；
- PreFlect 决策；
- Action/Stage/Run Reflection；
- Meta State、指标和干预；
- Token、费用和预算；
- Lesson 晋升与拒绝。

UI 只展示 reasoning summary，不显示模型私有思维链。

## 6. 预算和停止条件

默认硬限制：

```json
{
  "max_preflections_per_action": 1,
  "max_replans_per_stage": 3,
  "max_action_retries": 2,
  "max_critic_rounds": 1,
  "max_tool_steps": 30,
  "min_information_gain": 0.1,
  "max_consecutive_low_gain": 3,
  "reflection_token_budget": 4000
}
```

达到限制后只能停止、降级或请求用户，不得由 LLM自行提高限制。

## 7. 事件

```text
plan.hierarchy.created
plan.action.proposed
preflect.started
preflect.completed
preflect.blocked
action.observed
replan.triggered
plan.delta.applied
reflection.action.completed
reflection.stage.completed
reflection.run.completed
metacognition.state.changed
metacognition.intervention
lesson.created
lesson.promoted
```

事件先持久化再推送，并携带 plan_id/version/action_id。

## 8. 集成顺序

```text
HPM-01 Repository
  -> HPM-02 Meta-Planner
  -> HPM-03 Stage Planner
  -> HPM-04 Step Planner
  -> HPM-05 PreFlect
  -> HPM-06 Executor
  -> HPM-07 Replanner
  -> HPM-08 Reflector
  -> HPM-09 Meta Monitor
  -> HPM-10 RMOS
  -> HPM-11 UI
```

HPM-05、HPM-08、HPM-09 可基于稳定契约并行，不同代理不得修改同一文件。

## 9. Definition of Done

1. 每个 Run 有版本化 TCG。
2. 所有计划修改通过 PlanDelta。
3. 并发 Delta 不能覆盖新版本。
4. 每个 Action 执行前经过 PreFlect 和 Policy Gate。
5. Executor 不调用 LLM。
6. 工具结果全部使用 ToolExecutionResult。
7. 连续失败、低增益、循环和预算压力能触发确定性干预。
8. 新发现越界目标不会进入可执行计划。
9. Action、Stage、Run 反思可追溯到证据。
10. 只有验证后的 Lesson 进入长期记忆。
11. 不保存私有思维链或敏感原始输出。
12. Rust、数据库和前端构建全部通过。

## 10. 验证

```bash
cargo test -p kairos-platform-pent-api -p kairos-platform-pent-core -p kairos-platform-pent-server
bash script/test-database-migrations.sh
pnpm --dir apps/web --filter @vben/web-antd run typecheck
pnpm --dir apps/web --filter @vben/web-antd run build
git diff --check
```
