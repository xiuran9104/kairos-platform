# Kairos Platform 现状盘点

本文是平台实现现状与已知缺陷的权威清单。以实际 Rust 路由、数据库迁移和前端菜单为准，替代原 `parity-roadmap.md` 与各历史实施任务书。

## 1. 已实现

- **System**：认证、用户、角色、权限、菜单、租户、审计、后台管理。
- **Infra**：配置、文件、任务、日志、数据源。
- **AI**：模型管理、聊天/SSE、工具调用、知识库、图片/Midjourney/音乐/语音/Embedding、写作。
- **Media**：素材上传与基础能力。
- **Pent**：项目/任务/Run、Agent 会话与自主循环、工具路由与动态披露、YAML 工具配方、MCP（stdio/HTTP/SSE + 内部 `kairos-local`）、HITL 审批、多代理、Skills 生命周期、证据/流量回看工具、报告生成、弹性执行（并发限制/熔断/取消）。
- **MITM 流量抓取**：进程内代理（CA 签发 + TLS 终止 + HTTP 转发 + 捕获），落库 `pent_http_traffic`。
- **Scan**：扫描器、扫描任务、资产发现与归一化。
- **工具执行**：白名单 + Scope 校验 + prlimit + cgroup v2 + 网络策略 + 输出上限。

## 2. 未实现 / 部分实现

| 项 | 状态 | 位置/说明 |
| --- | --- | --- |
| `ToolExecutionBackend` 执行后端抽象 | 未接线 | `pent-server/src/tooling/executors/` 的 `BuiltinExecutor`/`McpExecutor` 无调度器引用，属死代码 |
| MITM run_id 打标 | 部分 | 捕获时读 `x-kairos-run-id` 头，未从任务上下文自动注入，默认 `mitm-proxy` |
| MITM body 归档 | 部分 | body 直接存 PG，未分流 MinIO；受 `PENTA_MITM_BODY_LIMIT` 限制 |
| 角色系统 | 未实现 | DesRed 对齐项 D-03（角色 = 提示词 + 工具白名单 + HITL 策略） |
| Workflow 节点图 | 未实现 | DesRed 对齐项 D-09（`pent-core/flow` 是状态机，非可视化编排） |
| 视觉分析 | 未实现 | DesRed 对齐项 D-10（独立视觉模型读截图/验证码） |
| 批量任务编排 | 部分 | 有任务与 task-monitor，批量编排能力待核实（D-11） |
| 知识库 multi-query + 精排 | 部分 | `tooling/reranker/` 有骨架，链路未闭合（D-07） |
| 内部 MCP 网络策略 | 部分 | `registry.rs` 内部调用固定 `NetworkPolicy::Full`，未从 Run 策略推导（D-08） |
| 子代理 profile | 部分 | 3 个 profile，DesRed 有 15 个（D-02） |
| 工具配方数量 | 部分 | 现有约 60 个 CLI 工具已配方化，DesRed 95 个未批量转换（D-01） |
| BPM / OAuth2 / 社交登录 / 代码生成 | 未验证 | 原 parity-roadmap 条目，需按实际路由/迁移逐条验收 |

## 3. 已知缺陷

| 缺陷 | 说明 |
| --- | --- |
| 加密测试被跳过 | `pent-server/src/infrastructure/secrets.rs` 两个加解密测试 `#[ignore]`，注释「test env key derivation needs investigation」 |
| 集成测试依赖外部服务 | 8 个测试因需本地 PG/MinIO/nmap 而 `#[ignore]`（其中加密测试见上） |
| Dead code | pent-server 编译 59 个 warning（如 `CheckpointWriter::needs_flush`、`TurnTaskRegistry::remove` 未使用），部分能力已实现但未接线 |
| HTTP 工具双路径 | 对话模式 `http_request` 有 reqwest 直连（开发/测试）与 curl 隔离执行两套，直连路径不回写流量记录 |

## 4. 明确不做

- 内置 C2、WebShell、DesShell、机器人接入（越权/绕过授权类能力）。
- 商业漏洞库全量订阅（可经 MCP 接入第三方）。
- 工具执行强制 Docker 沙箱（宿主进程 + prlimit + cgroup 隔离）。
