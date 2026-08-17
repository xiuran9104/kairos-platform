# Kairos Platform 现状盘点

本文是平台实现现状与已知缺陷的权威清单。以实际 Rust 路由、数据库迁移和前端菜单为准，替代原 `parity-roadmap.md` 与各历史实施任务书。

## 1. 已实现

- **System**：认证、用户、角色、权限、菜单、租户、审计、后台管理。
- **Infra**：配置、文件、任务、日志、数据源。
- **AI**：模型管理、聊天/SSE、工具调用、知识库、图片/Midjourney/音乐/语音/Embedding、写作。
- **Media**：素材上传与基础能力。
- **Pent**：项目/任务/Run、批量 Run、Agent 会话与自主循环、工具路由与动态披露、YAML 工具配方、外部 MCP（stdio/HTTP/SSE）、HITL 审批、多代理、角色策略、Workflow 图执行、视觉分析适配、Skills 生命周期、multi-query 知识检索与精排、证据/流量回看工具、报告生成、弹性执行（并发限制/熔断/取消）。
- **MITM 流量抓取**：进程内代理（CA 签发 + TLS 终止 + HTTP 转发 + 捕获），落库 `pent_http_traffic`；curl-backed 普通命令和结构化 recipe 均从执行上下文自动注入 `x-kairos-run-id`。
- **Scan**：扫描器、扫描任务、资产发现与归一化；独立 Scan Agent 支持注册/心跳、原子任务领取、租约恢复、CIDR 展开、并发 TCP 扫描、受限 Banner/HTTP 探测、协议与 TLS 指纹。Gateway 提供数据库驱动的 exact、版本范围和 CPE 产品规则匹配；目录命中保存版本证据并默认进入待复核。待复核风险可提交带授权声明的主动验证申请，经人工明确批准后由 Pent Agent 运行受控 Nuclei 验证器，只有结构化命中证据才会标记为已确认。
- **工具执行**：强制 bubblewrap 本地沙箱 + workspace/artifact 挂载 + mount/PID/user/IPC/UTS namespace + capdrop + seccomp-bpf + prlimit/cgroup v2 + 输出上限；按部署决策不启用 network namespace。

## 2. 未实现 / 部分实现

| 项 | 状态 | 位置/说明 |
| --- | --- | --- |
| GCP 云凭据注入 | 部分 | AWS/Azure/阿里云临时环境已接通；GCP 只读凭据文件注入仍保持拒绝 |
| 高权限工具 | 部分/明确受限 | `masscan`、`ssh_exec`、`sql_query` 已强制始终审批并路由到 `PENTA_WORKER_PROFILE=privileged` 专用队列；任意脚本执行、Metasploit 等仍不开放 |
| Scan 高级指纹与风险 | 部分 | 版本范围/CPE 和需授权主动验证已实现；完整 OS 指纹与商业漏洞情报仍未实现 |
| BPM / 社交登录 | 部分或未实现 | 前端保留兼容页面；Rust 后端没有完整 BPM 引擎，少数社交绑定兼容接口仍为固定成功响应 |

## 3. 已知缺陷

| 缺陷 | 说明 |
| --- | --- |
| 集成测试依赖外部服务 | AI E2E 与数据库空库初始化由专用脚本运行，不进入默认 `cargo test --workspace` 的外部依赖路径 |
| Scan warning / dead code | Scan Server 仍有未使用 import、旧内存态字段和未接线云适配辅助函数等编译告警 |
| HTTP 工具双路径 | 对话模式 `http_request` 有 reqwest 直连（开发/测试）与 curl 隔离执行两套，直连路径不回写流量记录 |

## 4. 明确不做

- 内置 C2、WebShell、DesShell、机器人接入（越权/绕过授权类能力）。
- 商业漏洞库全量订阅（可经 MCP 接入第三方）。
- OCI/runc 容器执行后端与工具镜像；统一使用宿主机本地 bubblewrap 沙箱。
