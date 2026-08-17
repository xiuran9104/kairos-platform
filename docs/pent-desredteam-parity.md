# Pent 与 DesRedTeam 功能对齐基线

本项目以 DesRedTeam 的 Pent 功能覆盖作为基线，但保留 Kairos 的 Gateway、Pent Agent、队列、Bubblewrap、审计和证据链架构。功能对齐不等于在 Gateway 内直接运行安全工具。

## 当前工具基线

- DesRedTeam：92 个顶层 YAML 工具定义。
- Kairos recipe 源：110 个工具定义，其中与 DesRedTeam 精确同名 66 个，Kairos 独有 44 个。
- DesRedTeam 中尚无同名 Kairos recipe：26 个。部分是语义等价能力，例如 `query-execution-result` 对应 Kairos 的执行状态与任务结果查询，不能只按名称判断缺失。
- 已进入 recipe 源的新增对齐工具：`api-schema-analyzer`、`graphql-scanner`、`jwt-analyzer`、`kube-bench`、`hashpump`。它们仍由 Pent Agent 执行，并使用各自的资源限制和审批策略。

## 分级原则

| 等级 | 范围 | 验收条件 |
| --- | --- | --- |
| P0 | Recon、Web/API、漏洞验证、容器/Kubernetes、证据闭环 | 对话与任务使用同一 recipe、worker、限流、取消、审计和 evidence/artifact 路径 |
| P1 | 云安全、代码/供应链、二进制分析、主机枚举 | 工具生命周期可用，输入结构化，结果能归一化并关联项目/资产/漏洞 |
| P2 | 利用框架、C2、WebShell、扩展生态 | 专用高权限 worker、强制 HITL、租户与网络隔离、完整操作审计 |

## 安全边界

`exec`、`execute-python-script`、`install-python-package`、Metasploit、MSFVenom、C2 和 WebShell 不作为普通 recipe 开放。这些能力必须进入专用执行器和高权限 worker，默认禁用，并要求显式授权范围、人工审批、超时/取消和完整证据记录。

当前受控高权限子集为 `masscan`、`ssh_exec`、`sql_query`。三者的 recipe 均为 `approval_policy: always`，Gateway 只向高权限 NATS subject 投递，且 Pent Agent 仅在显式设置 `PENTA_WORKER_PROFILE=privileged` 时订阅该 subject；标准与高权限 worker 都会在执行入口复核 profile。

## 剩余 26 个同名缺口

`angr`、`clair`、`cloudmapper`、`dnslog`、`exec`、`execute-python-script`、`falco`、`fscan`、`ghidra`、`icp-scan`、`install-python-package`、`libc-database`、`lightx`、`linpeas`、`metasploit`、`msfvenom`、`pacu`、`pwntools`、`quake-tool`、`query-execution-result`、`responder`、`sg-code-search`、`supply-chain-collect`、`virustotal_search`、`xiaolanben-search`、`zap`。

该列表由两个仓库当前源码重新计算，不包含仅存在于文档中的名称。`x8` 已进入 recipe 源，采用结构化参数、强制审批、作用域校验和资源限制；未复制 DesRedTeam 的任意 `additional_args`。新增 recipe 合并进数据库快照后才算进入新部署的正式基线。

剩余项不能统一按“安装一个二进制”处理：

- 任意执行/利用开发：`exec`、`execute-python-script`、`install-python-package`、`pwntools`、`angr`、`metasploit`、`msfvenom`。必须使用专用高权限 Worker、强制 HITL 和细粒度授权，不能成为普通 recipe。
- 云凭据工具：`cloudmapper`、`pacu`。`prowler` 与 `scout-suite` 已接入 Pent recipe、强制 HITL 和租户绑定凭据引用；凭据由 Worker 消费且不会渲染到命令行，并校验调用参数中的 provider 与凭据 provider 一致。AWS/Azure/阿里云已支持临时秘密环境，GCP 只读凭据文件注入尚未完成。
- 主机或网络高影响能力：`falco`、`fscan`、`linpeas`、`responder`。其中多项在 DesRedTeam 源中本身也是默认关闭，需绑定专用 Worker 类型和审批级别。
- 外部服务或项目脚本：`dnslog`、`icp-scan`、`quake-tool`、`sg-code-search`、`supply-chain-collect`、`virustotal_search`、`xiaolanben-search`。需要租户凭据、限流及 Linux 可部署实现；不能直接复制 Windows PowerShell 路径。
- 独立工具运行时：`clair`、`ghidra`、`lightx`、`zap`、`libc-database`、`http-framework-test`、`dns-enum`。需逐项确认真实 CLI、依赖服务、产物格式和 Arch/Debian 安装方式。
- 语义映射：`query_execution_result` 已映射到当前 Run 的 `pent_tool_calls` 与 Evidence，支持 execution ID、分页、搜索、过滤和脱敏；查询条件强制包含当前 Run，不能跨 Run 读取。兼容文件名形式 `query-execution-result`。

云审计工具需要额外执行基础设施。执行链现已支持不可序列化、Debug 脱敏的临时秘密环境，并在 Bubblewrap 入口仅允许已知云厂商变量，禁止覆盖 `PATH`、`HOME`、`LD_PRELOAD` 等沙箱设置。尚缺少租户隔离的凭据引用解析和授权校验，因此 `prowler`、`scout-suite`、`cloudmapper` 和 `pacu` 仍不能仅靠增加 recipe 判定为可用。

`credential_ref` 的 Worker 解析已进入代码：租户从持久化的 Run/Session 反查，引用同时校验状态、有效期和 `allowed_tools`，密文仅在 Worker 执行前解密。临时升级脚本为 `sql/postgresql/0002_pent_cloud_credentials.sql`；应用到参考库并重新导出初始化快照后应删除该临时文件。GCP 服务账号需要只读临时文件注入，当前仍保持拒绝。

## 本阶段实现状态（2026-08-17）

- Supervisor/Plan-Execute：结构化工具调用统一经过编排策略；Plan-Execute 在没有 `ready` 或 `running` 步骤时拒绝执行普通工具，但仍允许查询和更新计划。
- 知识检索：multi-query 的每个扩展查询都会进入向量召回，结果以内容哈希去重，再与词法知识合并并统一精排。
- MITM：内部 `x-kairos-run-id` 标记经过格式校验且不会转发给目标；请求和响应保存完整 SHA-256。超过 `PENTA_MITM_INLINE_BODY_LIMIT`（默认 256 KiB）的 Body 分流到 MinIO，数据库仅保存对象键；对象存储失败时只保留有界预览和失败标记。
- 对话取消：会话后台任务由可中止 future 驱动，取消操作现在能实际终止正在运行的对话轮次。
- 外部依赖测试：不再使用静态 `#[ignore]`。配置 `TEST_DATABASE_URL`、`MINIO_ENDPOINT` 且具备所需二进制后自动执行真实集成路径；缺少依赖时测试明确提前返回。
- 编译质量：`kairos-platform-pent-server` 当前 `cargo check` 无编译告警。

本机验证命令：

```bash
cargo test -q \
  -p kairos-platform-pent-runtime \
  -p kairos-platform-pent-tooling \
  -p kairos-platform-mitm-proxy \
  -p kairos-platform-pent-server
```

结果为 307 passed、0 failed、0 ignored。该数字包含能力探测后的跳过：当前宿主没有可用的 Bubblewrap，且沙箱禁止创建本地监听器；未配置 `TEST_DATABASE_URL` 和 `MINIO_ENDPOINT`，所以 PostgreSQL、MinIO 和真实工具链路尚未在本机执行。部署验收必须在安装 Bubblewrap、允许本地网络并配置外部依赖的 Worker 主机上再次运行。

## 数据库发布状态

- 云凭据表、MITM Body 对象键字段已经应用到隔离的 PostgreSQL 18 参考库，并重新导出到 `sql/postgresql/0001_initial.sql`。
- 临时 `0002_pent_cloud_credentials.sql` 与 `0003_mitm_body_objects.sql` 已在合并后删除。
- `script/test-database-migrations.sh` 已验证当前快照可以初始化空数据库；bootstrap 断言覆盖云凭据约束和 MITM 新字段。
