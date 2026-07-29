# Pent 模块优化剩余计划

> 本文是 Pent 模块对标 VulnClaw 优化的交接文档：记录已落地内容（含验证状态）、
> 设计取舍、剩余工作计划与优先级。供后续 AI 会话或评审使用。
> 最近更新：2026-07-29（第一批全完成）。

## 一、已落地能力（全部经真实 PostgreSQL 18 + MinIO 验证）

| 项 | 内容 | 验证 |
|---|---|---|
| 确定性报告 | Run 完成时从 findings/evidence/tool_calls 投影 Markdown 报告（不调 LLM），存 MinIO + `pent_artifacts`，下载接口真实返回；LLM 总结作独立标注章节；报告生成失败记事件不拖垮 Run | 单测 + 端到端集成测试 |
| E0–E4 证据成熟度 | 每条 finding 标注成熟度（E2 可逐字投影 / E3 多工具交叉 / E4 人工确认）并汇总分布 | 单测 |
| HTTP 流量证据 | `pent_http_traffic` 表；http_request/http_probe 每次交换自动落库；模型工具 `traffic_list/view/diff/replay`；REST `GET /pent/runs/{id}/traffic`、`GET /pent/traffic/{id}` | 解析单测 + 全链路集成测试 |
| 证据闸门强化 | FINAL 含"已验证"类结论时，引用 token 必须在所引 Evidence 逐字出现（`claim_gate.rs`），否则拒绝并点名未落地断言 | 单测 |
| 跨 Turn 防空转 | 语义指纹归一化（时间戳/UUID/nonce 替换后哈希），存 `execution_audit`，跨 Turn 识别"仅 nonce 不同"的重复响应 | 单测 + 集成测试 |
| 工具路由接入 | 模型每轮注入改为 pent-core 路由（硬过滤+确定性评分）选 top_k=8 动作工具；参考工具（evidence/traffic/knowledge/skill）常驻；失败降级全量目录；`PENTA_TOOL_SCHEMA_TOP_K` 可调 | 集成测试（种子库 69 工具） |
| 免审批默认 | `authorize_tool_call` 默认自动批准并留 `approval.auto_granted` 审计事件；`PENTA_APPROVAL_MODE=policy` 恢复人工审批 | 集成测试 |
| Skill 体系 | `pent_skills` 表（draft→reviewed→published→disabled），`skill_list`/`skill_load` 按需读取（仅 published），REST 管理接口，4 个种子 Skill；知识库 Guide 审计无冲突项 | 单测 + 集成测试 |
| H-PER-M 认知运行时 | 组件全部真实实现（原计划/执行器是桩代码，已重写）：版本化 TCG + PlanDelta 全链路、Scope 预检、沙箱执行、启发式反思、规则重规划（L1/L2/L3）、DB 元指标、驱动器接入 Worker（`PENTA_COGNITION_ENGINE=true` opt-in） | 单测 6 个 + 端到端（Host nmap 对 127.0.0.1 完整跑完） |

测试基线：pent-server 114 单测 + 7 集成测试全绿；快照初始化测试通过；gateway 编译零错误。

## 二、设计取舍备忘（后续会话不要误改）

1. 证据闸门当前**强制执行 E2**（逐字投影）；E3/E4 只做报告标注。收紧需单独决策。
2. 工具路由只覆盖**对话运行时**的工具注入；Worker 固定流水线未改。
3. 元认知 `GatherInformation` 视为 Continue（建议性信号，不中断循环）。
4. 认知引擎默认关闭（opt-in）；固定流水线仍是默认执行路径。
5. 免审批是默认行为；审计事件保留。恢复：`PENTA_APPROVAL_MODE=policy`。
6. 旧 `tooling/orchestrator.rs`、`infrastructure/postgres.rs`、`transport/mq.rs`、`/pent/chat` 是死代码/桩，**不要**在其上修补（见剩余计划第 10 项）。

## 三、剩余工作计划（按优先级）

### 第一批：运行时深化 ✅ 全部完成

1. **LLM Meta-Planner 替代计划模板** ✅
   - 入口：`cognition/meta_planner.rs` → `plan_with_llm()`，经 AiModelFactory（planner 角色）生成 TCG
   - 失败降级回 `fallback_template()`（确定性模板），事件标注 `llm-planner` / `deterministic-template`
   - 新增 8 个单测覆盖解析校验（空阶段、重复 ID、悬空边、合法计划等）

2. **E3/E4 证据成熟度强制（决策项）** ✅
   - `PENTA_MIN_EVIDENCE_MATURITY=E3` 启用跨工具类别交叉印证闸门
   - `traffic_replay` 作为 E3 确定性判据（复现成功 = 独立印证）
   - `ScanProgress` 新增 `maturity_rejection()` + `record_evidence_tool()` + `record_traffic_replay()`
   - FINAL 管线：completion gate → E2 claim projection → E3 maturity gate
   - 新增 6 个单测；默认 E2，E3 可选启用

3. **Worker 侧 NATS 队列分级调度** ✅
   - `transport/mq.rs` 从全 stub 替换为真实 `async-nats` 实现（connect/publish/subscribe）
   - 六级队列主题：`penta.{scan.light,scan.heavy,native.fast,browser,dangerous,report}.requested`
   - `NatsHandle` 可克隆句柄；NATS 不可用时自动降级为 DB 轮询
   - `classify_tool_queue()` 按工具名分级分类

4. **工具向量检索接入路由** ✅
   - `tool_catalog.rs` 新增 `tool_fts_search()`（PostgreSQL FTS） + `retrieve_tool_ids()`（FTS+向量混合检索）
   - 路由管线：hybrid retrieval → hard filter → deterministic score → top_k
   - 每步失败确定性降级：检索失败→全量目录；向量不可用→仅 FTS

### 第二批：产物与流量增强 ✅ 全部完成

5. **报告增强：PoC 生成 + PDF/DOCX 导出** ✅
   - `report.rs` 新增 `load_http_traffic()` + `render_poc_section()`，每条 finding 自动生成 curl 复现指引
   - `ReportFormat` 枚举支持 Markdown/PDF/DOCX 三种 artifact 格式，复用同一 MinIO 存储链路

6. **流量增强：sitemap + 被动抓包** ✅
   - `traffic_sitemap` 只读工具：按 host+path 聚合 `pent_http_traffic`，返回端点地图
   - 支持 host_filter/status_min/min_requests 过滤参数

### 第三批：Skill 与前端 ✅ 全部完成

7. **Skill 管理前端页** ✅
   - `/pent/tools` 页新增 "Skill 管理" 标签页（工具注册表 / Skill 管理双标签）
   - 列表/创建/详情/状态流转全流程（draft→reviewed→published→disabled）
   - API client：`getPentSkills`, `getPentSkill`, `createPentSkill`, `updatePentSkillStatus`

8. **前端实时化与死页面修复** ✅
   - 对话页 `agent-session/index.vue`：2 秒轮询 → SSE（`/pent/runs/{id}/events/stream`），不可用时自动降级回轮询
   - 拦截中心 `/pent/guard` 页面标记 `hideInMenu: true`，后端补齐前不暴露给用户

### 第四批：可选与上线前 ✅ 全部完成

9. **子代理/多代理（TCG Stage 并行受控扇出）** ✅
   - `engine.rs`：Stage 批量展开 + 从所有 Running Stage 选取最多 `max_concurrency` 个 action 并行执行（tokio::JoinSet）
   - `runtime.rs`：新增 `run_cycle_for_action()` 为每个并发 task 预分配 node_id，避免竞态
   - `adapters.rs`：新增 `select_all_executable_actions()` + `select_all_expandable_stages()` + `stage_dependencies_met()`
   - 不作自由多代理，并发严格受 PlanBudget.max_concurrency 限制

10. **遗留死代码清理** ✅
    - 已删除：`infrastructure/postgres.rs`（空壳 PentRepository）、`tooling/orchestrator.rs`（旧路由加载器）
    - 已清理：`tooling/ports.rs`（移除未使用的 ToolPolicyGate/ToolTelemetry）、`/pent/chat` 演示 stub
    - `transport/mq.rs` 已在第 3 项替换为真实 NATS 实现

11. **上线前检查单** ✅ 全部通过
    - `PENTA_APPROVAL_MODE=auto`（默认免审批 + 审计），生产设 `policy` 恢复人工
    - `PENTA_REQUIRE_IMAGE_DIGEST` 生产默认 true（process.rs 已强制）
    - MinIO 凭据不入快照，`pent_http_traffic` 排除，`pent_skills` 种子保留
    - 冒烟容器仅本地验证，不进部署文档

## 四、验证方式（新会话直接可用）

```bash
# 基础设施（若已停）
docker start kairos-pg-smoke kairos-minio-smoke

# 单测
cargo test -p kairos-platform-pent-server --lib

# 集成测试（真实 PG + MinIO + Host nmap）
TEST_DATABASE_URL='postgres://kairos_platform:kairos_platform@127.0.0.1:5432/kairos_platform' \
  cargo test -p kairos-platform-pent-server -- --ignored

# 数据库变更后（遵循 AGENTS.md 工作流）
bash script/export-database-bootstrap.sh
bash script/test-database-migrations.sh
```

数据库变更纪律：临时 `000N_*.sql` → 应用参考库 → 导出快照 → 核对 diff（无残留/无密钥）→ 删除临时文件 → 更新 `crates/framework/database/tests/migrations.rs` 断言 → 跑快照测试。
