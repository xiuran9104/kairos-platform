# Pent 模块靶场效果优化实施计划

日期:2026-08-19
背景:juice-shop/dvwa 靶场实测漏洞发现率低、Agent 执行质量差。对标 DesRedTeam(`~/code/desredteam-v2`)源码分析后形成本计划。所有代码位置已核实。

## 执行状态(2026-08-19)

- 项 1:已完成,提交 `5523451 feat(pent): force strategy rotation after tool failures`。
- 项 2:已完成,提交 `76c7cc8 feat(pent): enforce reproducible finding quality`。
- 项 3:已完成,提交 `b62439f feat(pent): improve security context engineering`。
- 项 4:已完成,14 个 playbook(`skill-seed-playbook-*`,category=playbook)已合并进 `0001_initial.sql` 快照,bootstrap 断言已加入 `migrations.rs`,空库初始化测试通过。
- 待做:全量 `cargo test --workspace`;靶场前后对比验证(项 5 的验证步骤)。

## 现状修正

之前评估说"没有收尾闸门"不准确。实际 `crates/modules/pent-server/src/conversation/runtime.rs:1035-1145` 已有 6 道收尾闸门(后台工具清空、编排完成、Web 覆盖、扫描进度/证据、声明-证据一致性、成熟度)。真实差距在闸门规则细度、技能内容深度、finding 校验、上下文工程四处。

## 项 1:收尾行为强化(防止浅尝辄止)

现状:
- 自主循环提示 `AUTONOMOUS_LOOP_PROMPT`(`runtime.rs:58-87`),无"结束条件与停止约束"。
- 无进展活性闸门 `MAX_CONSECUTIVE_NO_PROGRESS_RESULTS = 12`(`runtime.rs:53`)过于宽松,且无"同工具连续失败强制换策略"机制。
- 空响应已有重试(`empty_model_retry`,`runtime.rs:309-320`),不用改。

改动:
1. 循环内跟踪同一工具名连续失败次数,达到 3 次注入推进提示(参照 stall 提示 `runtime.rs:924-935` 写法):停止同类参数重试,列出替代路径(换工具/换 payload 家族/换枚举维度),成功即重置。
2. `AUTONOMOUS_LOOP_PROMPT` 增补"结束条件与停止约束"小节(≤10 行):收尾前自检——每个声称的漏洞已 `record_finding` 且附证据;空结果/404/超时不得直接下"不存在"结论,必须换策略再验一次;确认无洞须说明已覆盖的攻击面。
3. 检查 6 道现有闸门拒绝文本(`push_gate_rejection`,`runtime.rs:2439` 起),拒绝时附可执行的下一步指引;已有则不动。

涉及文件:`crates/modules/pent-server/src/conversation/runtime.rs`
验收:新增计数器单元测试;`cargo check/test -p kairos-platform-pent-server` 零告警全过。
工作量:小。

## 项 2:finding 服务端必填校验

现状:`record_finding`(`crates/modules/pent-server/src/conversation/run_bridge.rs:395-468`)已强制 title/severity 枚举/evidence_ids 非空且属当前 run;但 target 允许空串、description 可空。对标 DesRedTeam 九项必填。

改动:
1. `record_finding` 增加硬校验:`target` trim 后非空;`description` trim 后 ≥ 20 字符。错误消息一次性列出全部缺失字段及原因。
2. 适配两条自动产生路径,使其满足新校验:
   - 确定性投影 `finding_projection.rs:14,50-80`(target 从证据/参数取,description 用规则描述+证据摘要生成);
   - 收尾兜底提取 `extract_and_persist_findings`(`run_bridge.rs:472`)。
3. `tool_catalog.rs:443` 的 record_finding 工具描述补充质量要求:"按仅看本记录即可复现的标准填写 description(触发点、payload、响应特征);target 必须是具体 URL/主机:端口"。
4. 不动表结构(DB CHECK 已强制 evidence 非空)。

涉及文件:`run_bridge.rs`、`finding_projection.rs`、`tool_catalog.rs`(仅 record_finding 定义区)
验收:缺 target/description 过短/正常通过/投影路径产出合规 四类单测;pent-server 测试全过。
工作量:小。

## 项 3:上下文工程(头尾预览 + 领域摘要)

现状:
- 工具输出超 8000 字符只留头部(`crates/modules/pent-runtime/src/observation.rs:6,62-72`),尾部证据丢失;完整原文已在 `pent_evidence.source`,可用 `evidence_view`/`evidence_search` 回读,但截断提示未指引。
- 上下文压缩(`pent-runtime/src/context_compaction.rs:11-40`,触发于 `model_gateway.rs:420-434`)摘要提示词非领域定制。

改动:
1. `observation.rs` 截断改为头(约 2/3)+尾(约 1/3)双预览,中间标记含:省略字符数、总字符数、完整内容获取方式(有 evidence id 带 id,否则指引 evidence_search)、"不得凭截断内容下结论"。
2. 压缩摘要提示词改领域定制版,强制保留:已确认漏洞及攻击链、凭证/令牌、有效 payload、失败死路(防重复尝试)、资产/端点枚举结论、当前计划进度。`fallback_summary` 同步体现这些类目。
3. pent-server 侧若有依赖旧截断格式的断言需同步。

涉及文件:`pent-runtime/src/observation.rs`、`pent-runtime/src/context_compaction.rs`(必要时 `model_gateway.rs` 触发处)
验收:头尾保留/短内容不截断/摘要提示词含必备类目 单测;`cargo test -p kairos-platform-pent-runtime -p kairos-platform-pent-server` 全过。
工作量:小。

## 项 4:漏洞 playbook 技能库(内容建设,工作量最大)

现状:
- skills 存 `pent_skills` 表,Agent 经 `skill_list`/`skill_load` 按需拉取,未 load 前扫描类工具被硬门控(`runtime.rs:2844-2896`)——机制完整,缺的是内容。
- 现有种子:4 条手工(`skill-seed-*`)+ pentagi-wiki 导入的若干;`skill_load` 截断 12k 字符(`skills.rs:17`)。
- 种子走数据库快照工作流,无运行时自动播种。

改动:
1. 编写约 14 个 Web 漏洞 playbook(覆盖靶场主要漏洞类):sqli、xss、ssrf、ssti、xxe、command-injection、path-traversal、file-upload、auth-bypass、idor、jwt、sensitive-info-exposure、directory-enum、open-redirect。每个 ≤ 12k 字符,结构统一:触发时机 / 探测矩阵(场景→起始 payload→判定依据)/ 指纹路由 / 验证步骤(证据标准)/ 负结果处理。
2. 写 `sql/postgresql/0002_vuln_playbook_skills.sql` 插桩(`status='published'`,manifest 用 `skill-seed-*` 的结构化格式,name 按漏洞类命名以便 `skill_list` 检索命中)。
3. 按 AGENTS.md 数据库工作流:应用到本地参考库(postgres 容器已在运行)→ `bash script/export-database-bootstrap.sh` 重导快照 → 删 0002 → `bash script/test-database-migrations.sh` → 更新 `crates/framework/database/tests/migrations.rs` bootstrap 断言。
4. 内容风格参照 `~/code/desredteam-v2/skills/` 但自行撰写,不整段复制。

涉及文件:`sql/postgresql/0001_initial.sql`(快照重导)、`migrations.rs` 断言;不改 Rust 代码。
验收:快照初始化空库通过;`skill_list query=sqli` 能命中新 playbook;bootstrap 断言测试过。
工作量:中(主要是内容撰写)。

## 不做 / 缓做

- 异步并发执行模型(工具 60 秒转后台):架构改动大,本期不动。
- plan-execute 重规划环:kairos 已有编排闸门,先观察项 1-4 效果。
- DesRedTeam 的 26 个缺口工具:按 parity 文档分级原则另行评估,不在本期。

## 执行顺序与验证

1. 项 3 → 项 2 → 项 1(均小改动,顺序提交)
2. 项 4(内容撰写 + 快照工作流)
3. 全量 `cargo test --workspace`
4. 靶场对比验证:同一 juice-shop 目标,改动前后各跑一个 run,对比 finding 数量与覆盖动作数
5. 提交并推送 origin(gitcode)

## 风险

- 项 2 校验收紧可能使存量自动投影路径产出失败,必须先适配再收紧,并有测试兜底。
- 项 4 快照重导会把本地参考库当前状态固化,导出前确认库内无临时脏数据(导出脚本已排除运行时表并做密钥扫描)。
