# 产品规格文档：LLM 自动化渗透测试平台（AutoPentest）

| 项目   | 内容                    |
| ---- | --------------------- |
| 文档版本 | v1.6（技术选型对齐落地栈：后端 Rust+Axum、前端 Vue3+AntDV+Vben、PG18+pgvector、Neo4j5.26+Graphiti、Redis8、NATS、MinIO、AiModelFactory/DeepSeek、chrome-devtools-mcp、mitmproxy、宿主机原生 prlimit 执行） |
| 创建日期 | 2026-08-06            |
| 文档状态 | 评审稿                   |
| 产品形态 | Web 应用（SaaS / 私有化可部署） |
| 文档作者 | 产品 + 研发               |

---

## 修订记录

| 版本   | 日期         | 说明                                                |
| ---- | ---------- | ------------------------------------------------- |
| v0.1 | 2026-08-06 | 初稿：核心 Agent Loop、本地/MCP 工具、基础合规模型                 |
| v0.2 | 2026-08-06 | 确认纳入多 LLM 供应商切换（F-11）、分布式扫描节点（F-12）               |
| v0.3 | 2026-08-06 | 合规模型改为「操作者责任制 + 高风险栅栏」，移除授权前置拦截                   |
| v1.0 | 2026-08-06 | 全面重写：按定稿范围重排；新增项目级流量记录（F-13）、长期记忆系统（F-14）；重绘全层架构图 |
| v1.1 | 2026-08-06 | 新增基础设施与中间件规划（消息队列 MQ、Redis、存储分层、密钥库）               |
| v1.2 | 2026-08-06 | 新增「方案设计阶段」：概要设计（技术选型/模块拆分/数据库设计）+ 详细设计（接口文档/时序图/子系统） |
| v1.3 | 2026-08-06 | 全文重写：统一术语与风格、消除累积编辑瑕疵，内容范围与 v1.2 一致               |
| v1.4 | 2026-08-06 | 设计校正：移除「工具执行层必须 Docker 沙箱」约束，改为主机本地进程级隔离 + 行为栅栏；Docker/K8s 仅用于平台组件部署 |
| v1.5 | 2026-08-06 | 文件被意外清空后恢复重建（内容同 v1.4）；新增 5.2.3.5「流量抓取机制」小节（HTTP 拦截代理 + eBPF/iptables + 按 cgroup 打 project_id） |
| v1.6 | 2026-08-06 | 技术选型对齐用户落地栈：后端 Rust+Axum、前端 Vue3+TS / Ant Design Vue / Vben Admin / Vite8+pnpm / 端口 5666；PG18+pgvector、Neo4j5.26+Graphiti、Redis8、NATS、MinIO；LLM 自研 AiModelFactory（兼容 OpenAI，接入 DeepSeek v4-pro）；MCP 用 chrome-devtools-mcp；HTTP 代理 mitmproxy；工具执行宿主机原生（prlimit）+ 60+ 安全工具；同步修正下游 FastAPI/Redis7/Neo4j5/PG15/Redis Streams 引用 |

---

## 目录

1. [产品概述](#1-产品概述)
2. [用户角色与场景](#2-用户角色与场景)
3. [功能需求](#3-功能需求)
4. [交互与 UI 设计](#4-交互与-ui-设计)
5. [方案设计阶段（概要设计 + 详细设计）](#5-方案设计阶段概要设计--详细设计)
6. [验收标准与埋点](#6-验收标准与埋点)
7. [风险与合规](#7-风险与合规)
8. [里程碑](#8-里程碑)
9. [附录](#9-附录)

---

## 1. 产品概述

### 1.1 背景与问题

传统渗透测试高度依赖安全专家的人力投入，存在三大痛点：

- **成本高、周期长**：一次完整的渗透测试需要数天至数周，专家资源稀缺且昂贵。
- **覆盖不稳定**：人工测试依赖个人经验，容易遗漏资产、忽略非典型攻击路径。
- **知识难沉淀**：每次测试的经验（打法、指纹、路径）随人员流动而流失，无法跨项目复用。

与此同时，大语言模型（LLM）已具备较强的推理、规划与工具调用（Function Calling）能力。将 LLM 编排为 Agent，结合成熟的本地安全工具、可扩展的 MCP 工具生态，以及**项目级流量归档**与**长期记忆系统**，有机会把渗透测试从「纯人工」推进到「人机协同 + 自动化循环 + 跨任务自学习」的新形态。

### 1.2 产品目标

- **自动化驱动**：LLM Agent 在「规划 → 执行 → 观察 → 判断」循环中自主推进测试，最小化人工干预。
- **可编排**：同时支持本地安全工具（宿主机原生执行，不设 Docker 沙箱，含 60+ 安全工具）与 MCP（当前 chrome-devtools-mcp）远程/第三方工具；LLM 供应商经自研 AiModelFactory 兼容 OpenAI 协议可切换（当前接入 DeepSeek v4-pro，另可选 OpenAI / Claude / 自建）。
- **可追溯**：每一步推理、工具调用、流量与返回结果全程留痕，可按项目检索、回放与审计。
- **可进化**：跨任务的攻击图谱与记忆库持续累积，越测越准。
- **合规性**：默认信任授权目标，以操作者责任制 + 高风险栅栏约束危险动作，不做前置拦截。

### 1.3 核心概念与能力一览

**核心概念**

| 概念            | 说明                                                                         |
| ------------- | -------------------------------------------------------------------------- |
| **Agent Orchestrator（编排核心）** | 驱动 ReAct 循环的状态机：Plan→检索记忆注入→Act→Observe→Judge→回写记忆；统一编排本地工具、MCP、扫描节点与 LLM。 |
| **本地工具（Local Tools）**   | 在平台执行环境内直接运行的安全工具（端口扫描、漏洞探测、Web 指纹等），以主机本地进程运行，通过命令白名单与超时控制约束。                           |
| **MCP 工具**     | 通过 Model Context Protocol 接入的远程/第三方工具（stdio / SSE 传输），扩展平台能力而不改核心代码。              |
| **分布式扫描节点（Scan Node）** | 独立的执行节点池，承接高并发任务，由调度器弹性扩缩容、统一管理出口与配额。                                |
| **攻击图谱（Attack Graph）** | 以图结构记录资产、服务、漏洞、凭据节点及可达/利用关系，支撑路径推理与可视化。                              |
| **攻击路径（Attack Path）** | 由图谱推导出的可达利用链路，标注置信度，作为报告与下一步规划的依据。                                     |
| **记忆库（Memory）**  | 向量化（embedding + RAG）的长期经验库：TTP、指纹、有效打法，跨任务检索复用。                           |
| **项目（Project）**         | 顶层组织单元，聚合目标、扫描任务、流量记录与长期记忆；流量与记忆均按项目隔离与归档。                                        |

**能力一览（功能 → 章节）**

| 能力域        | 关键功能                  | 对应章节        |
| ----------- | --------------------- | ----------- |
| 编排核心        | Agent Loop、实时流式         | 5.2.3.1 / 5.2.2 |
| 工具生态        | 本地工具、MCP 接入           | F-07 / F-08   |
| 输出与报告       | 漏洞报告生成与导出            | F-06         |
| 项目与数据       | 项目级流量记录、历史任务与回放       | F-13 / F-09   |
| 智能记忆        | 长期记忆系统（图谱/路径/记忆库）     | F-14         |
| 管理合规        | 多 LLM 切换、分布式节点、合规栅栏   | F-11 / F-12 / 5.2.3.7 |

### 1.4 非目标（明确不做）

- 不做授权目标的前置拦截与白名单审批（默认信任授权，仅高风险栅栏约束）。
- 不提供漏洞利用的「一键自动化入侵」对外服务；平台定位为企业内部/授权场景的测试助手。
- 不在 v1 阶段内置商业漏洞库的全量订阅（可通过 MCP 接入第三方情报）。
- 不绑定单一云厂商，私有化部署为一级目标。

---

## 2. 用户角色与场景

| 角色          | 职责                              | 核心诉求                |
| ----------- | ------------------------------- | ------------------- |
| 渗透测试工程师     | 发起任务、复核结果、导出报告                 | 自动化减负、流量/证据可追溯       |
| 安全负责人       | 配置合规栅栏、审计全部操作、管理 LLM 预算       | 合规可控、责任可追溯           |
| 平台管理员       | 管理扫描节点池、MCP 服务、LLM 供应商凭据      | 弹性扩容、统一出口与配额         |
| （可选）红队经理    | 跨项目统筹、复用攻击图谱与记忆             | 跨任务知识沉淀             |

**典型场景**：工程师新建项目 → 录入授权目标范围 → 选择 LLM 供应商与策略 → 启动任务 → 实时观察 Agent 自主探测 → 暂停复核高风险动作 → 收敛后生成带流量证据的报告 → 下次同类型项目自动复用历史记忆提升覆盖。

---

## 3. 功能需求

### 3.1 功能清单（F-01 ~ F-14）

**编排核心**

| 编号   | 功能          | 优先级 | 说明                                                           |
| ---- | ----------- | --- | ------------------------------------------------------------ |
| F-01 | Agent 自动化循环 | P0  | 基于 ReAct 的 Plan→Act→Observe→Judge 循环，自主推进测试至收敛；支持暂停/继续/停止。      |
| F-02 | 实时步骤流式展示    | P0  | 每一步推理、工具调用、观察结果经 SSE/WebSocket 实时推送前端，1s 内呈现。                |
| F-03 | 任务编排与调度     | P0  | 创建/配置任务（目标、策略、LLM 选择），提交后由 Orchestrator 调度执行。                |
| F-04 | 实时监控面板      | P1  | 仪表盘展示任务进度、当前阶段、命中漏洞、LLM 成本等。                                 |
| F-05 | 历史任务与回放     | P1  | 任务列表、详情、步骤回放。                                                |

**工具生态**

| 编号   | 功能          | 优先级 | 说明                                                           |
| ---- | ----------- | --- | ------------------------------------------------------------ |
| F-07 | 本地工具管理      | P1  | 工具注册、版本、命令白名单维护                                              |
| F-08 | MCP 连接管理    | P1  | MCP Server 配置、健康检查、工具发现                                      |
| F-11 | 多 LLM 供应商切换 | P1  | 支持 OpenAI / Claude / 自建模型，按任务或单步策略选择 Provider，记录 token 消耗与成本 |
| F-12 | 分布式扫描节点     | P1  | 多目标并发时动态扩容隔离执行节点，统一调度、网络出口与配额管理                              |

**输出与报告**

| 编号   | 功能        | 优先级 | 说明                       |
| ---- | --------- | --- | ------------------------ |
| F-06 | 漏洞报告生成与导出 | P1  | 按 CVSS 分级，支持 PDF/JSON 导出 |

**项目与数据**

| 编号   | 功能        | 优先级 | 说明                                                |
| ---- | --------- | --- | ------------------------------------------------- |
| F-09 | 项目管理      | P0  | 项目的创建、目标范围录入、成员与权限。                                |
| F-13 | 项目级流量记录   | P1  | 按项目归档所有测试流量（HTTP/网络流），支持按项目检索、回放与漏洞证据关联            |

**智能记忆**

| 编号   | 功能       | 优先级 | 说明                                                  |
| ---- | -------- | --- | --------------------------------------------------- |
| F-14 | 长期记忆系统   | P2  | 攻击图谱（图库）+ 攻击路径 + 记忆库（向量+RAG），跨任务自增强、越测越准。            |

**管理与合规**

| 编号   | 功能        | 优先级 | 说明                              |
| ---- | --------- | --- | ------------------------------- |
| F-10 | 审计与合规管理   | P1  | 全量操作/步骤/工具调用审计日志；AUP 免责声明前置确认；高风险栅栏配置。 |

### 3.2 关键用户故事

- **US-1（自动化）**：作为渗透工程师，我希望启动任务后 Agent 能自主完成「发现→验证→收敛」，让我只在高风险动作时介入。
- **US-2（可追溯）**：作为安全负责人，我希望每一步工具调用的请求/响应都能按项目回放，并作为漏洞证据链。
- **US-3（合规）**：作为安全负责人，我希望启动前必须确认 AUP 免责声明，且全网段扫描/破坏性动作被栅栏拦截或二次确认。
- **US-4（多供应商）**：作为平台管理员，我希望在任务配置里切换 LLM 供应商并准确记录成本，避免被单一厂商绑定。
- **US-5（弹性）**：作为平台管理员，我希望多目标并发时节点自动扩容、单节点故障自动重调度，任务不中断。
- **US-6（自学习）**：作为红队经理，我希望同一项目的多任务持续累积攻击图谱，新任务能检索历史记忆提升覆盖。

---

## 4. 交互与 UI 设计

### 4.1 信息架构（前端页面划分）

| 路由                | 页面            | 核心内容                                       |
| ----------------- | ------------- | ------------------------------------------ |
| `/`               | 仪表盘           | 任务总览、近期活动、LLM 成本趋势                         |
| `/projects`       | 项目列表          | 项目卡片、创建入口                                   |
| `/projects/:id`    | 项目详情（Tab 容器）  | 目标目标&范围 / 攻击图谱 / 攻击路径 / 记忆库 / 流量记录 五个子页范围 / 攻击图谱 / 攻击路径 / 记忆库 / 流量记录 / 漏洞 六个子页          |
| `/tasks/new`       | 新建任务向导        | 选目标、选策略、选 LLM、确认 AUP                        |
| `/tasks/:id`       | 任务实时监控        | 步骤流（SSE）、当前阶段、命中漏洞、暂停/继续/停止               |
| `/reports/:id`     | 报告详情页         | CVSS 严重度热力图、漏洞卡片（描述/PoC/证据截图/关联流量/修复建议）、一键导出 |
| `/tools`          | 本地工具管理        | 工具注册、白名单、版本                                 |
| `/mcp`            | MCP 连接管理      | Server 配置、健康检查、工具发现                         |
| `/nodes`          | 扫描节点池         | 节点状态、扩缩容、配额、心跳                             |
| `/providers`      | LLM 供应商管理     | Provider 配置、凭据托管（api_key_ref）、成本统计           |
| `/memory`         | 全局记忆/图谱浏览     | 跨项目攻击图谱、记忆条目检索                              |
| `/settings/compliance` | 合规与栅栏配置    | AUP 文本、高风险动作分级、限速/出口白名单                     |

### 4.2 交互要点

- **实时流**：任务详情页以时间线呈现 AgentStep，新步骤 1s 内到达；工具调用可展开查看请求/响应摘要。
- **高风险介入**：当 Agent 拟执行需确认的高风险动作，前端弹出二次确认卡（动作描述 + 目标 + 影响），确认前该步暂停。
- **证据关联**：漏洞卡片一键跳转至对应 `TrafficRecord`；报告自动附带流量证据链。
- **图谱可视化**：项目详情的攻击图谱视图以力导向图展示资产/服务/漏洞/凭据节点与可达、利用关系，点击节点查看证据步骤与关联流量。

---

## 5. 方案设计阶段（概要设计 + 详细设计）

### 5.1 概要设计（架构设计）

#### 5.1.1 技术选型

| 类别        | 选型                                              | 说明                                          |
| --------- | ----------------------------------------------- | ------------------------------------------- |
| 后端语言/框架   | **Rust + Axum（Tokio 异步运行时）**                  | 高性能、内存安全；Axum 基于 Tokio 原生异步，适合长连接 SSE/WebSocket 与高并发工具调度 |
| 前端        | **Vue 3 + TypeScript**                           | SPA，实时流用原生 EventSource / WebSocket                 |
| 前端 UI 组件库  | **Ant Design Vue**                               | 企业级组件体系，与 Vben Admin 配套                        |
| 前端脚手架     | **Vben Admin（基于 Vite）**                         | 中后台开箱即用骨架，内置路由 / 权限 / 主题                    |
| 前端构建工具    | **Vite 8 + pnpm 11+**                             | 极速冷启动与 HMR                                    |
| 前端端口      | **5666**                                         | 本地开发 / 部署端口                                 |
| 主数据库      | **PostgreSQL 18（含 pgvector 扩展）**                 | 结构化实体（Project/Task/Step/Vuln/Report/Traffic/Audit/Provider/Node），见 5.1.6；向量检索由 pgvector 在主库直接实现 |
| 图数据库      | **Neo4j 5.26 + Graphiti**                         | 知识图谱 / 攻击链（攻击图谱节点与关系边，F-14）；Graphiti 提供时间感知的图谱记忆 |
| 缓存/限流     | **Redis 8**                                       | 全局限速栅栏（滑动窗口/令牌桶）、实时流 Pub/Sub、缓存、分布式锁、会话，见 5.1.3     |
| 消息队列      | **NATS**                                          | 工具执行结果推送、SSE 事件总线、任务分发/结果回传、优先级与背压、DLQ，见 5.1.3        |
| 对象存储      | **MinIO**                                         | 流量全量请求/响应体、报告导出文件、大体积证据（Evidence/报告/附件），冷热分层归档   |
| LLM 调用    | **自研 AiModelFactory（兼容 OpenAI 协议，当前接入 DeepSeek v4-pro）** | 统一 `chat(messages, tools)`；多供应商可切换（OpenAI/Claude/自建/DeepSeek），凭据托管不落明文 |
| MCP 工具    | **chrome-devtools-mcp（Headless Chrome CDP 浏览器自动化）** | 网页交互 / 爬取 / 动态测试类 MCP 工具，见 5.2.3.3              |
| HTTP 代理   | **mitmproxy**                                     | 流量抓取 / 重放，详见 5.2.3.5 流量抓取机制                      |
| 密钥库       | **HashiCorp Vault** 或 K8s Secret + 信封加密              | LLM Provider 凭据（`api_key_ref`）托管，不落明文              |
| 编排/部署     | **Docker + Kubernetes**（生产）/ **docker-compose**（一键私有化） | CI/CD 用 GitHub Actions / GitLab CI                  |
| 可观测       | **OpenTelemetry + Prometheus + Grafana**；日志 Loki/ELK  | 链路追踪、指标、日志统一采集                              |
| 工具执行隔离    | 宿主机原生执行（**prlimit** 资源隔离），含 nmap/nuclei/sqlmap/dalfox 等 **60+ 安全工具** | 本地工具与扫描节点**不设 Docker 沙箱**——主机原生进程直连授权目标出网更快、逃逸面更小；prlimit 做 CPU/内存/文件描述符限额；Docker/K8s 仅用于平台自身组件部署 |

#### 5.1.2 系统架构设计

```
┌──────────────────────────────────────────────────────────┐
│                        Web 前端 (SPA)                       │
│  仪表盘 · 新建向导 · 实时监控 · 报告 · 项目 · 图谱/记忆 · 工具  │
└───────────────────────────────┬──────────────────────────┘
                                 │ HTTPS · SSE / WebSocket
┌───────────────────────────────▼──────────────────────────┐
│                       API 网关                              │
│              鉴权 · 限流 · AUP 确认 · 审计                    │
└───────────────────────────────┬──────────────────────────┘
                                 │
┌───────────────────────────────▼──────────────────────────────────────┐
│                           Agent Orchestrator                           │
│  ┌────────────────────────────────────────────────────────────┐     │
│  │  Loop Controller（状态机）                                    │     │
│  │   Plan → 检索长期记忆注入 │ Act → 工具路由 │ Observe → 结构化   │     │
│  │   Judge → 继续/收敛/升级  │ 回写 → 图谱 + 记忆库                │     │
│  └────────────────────────────────────────────────────────────┘     │
│     ├───────────┬───────────┬────────────┬──────────────────┐        │
│     │           │           │            │                  │        │
│  ┌──▼──┐  ┌─────▼────┐ ┌────▼────────┐ ┌─▼─────────────┐            │
│  │Local│  │ MCP      │ │ Scan Node   │ │ LLM Provider  │            │
│  │Tool │  │ Client   │ │ Pool(分布式) │ │ Abstraction   │            │
│  │Exec │  │ stdio/SSE│ │ 弹性扩缩容   │ │OpenAI/Claude/ │            │
│  │     │  └──────────┘ └────────────┘ │ Self-hosted   │            │
│  └──┬──┘                             └────────────────┘            │
│     │ 所有对外请求/响应                                           │
│     └──────────────► Traffic Recorder（按 project_id 归档）──────┘
└───────────────────────────────┬──────────────────────────────────────┘
                                 │
     ┌───────────────────────────▼────────────────┐   ┌──────────────────────┐
     │              Memory Store                    │   │       数据层           │
     │  攻击图谱(图库) · 记忆库(向量+RAG)             │   │ Project/Task/Step/    │
     │  攻击路径 · TTP/指纹/打法                     │   │ Vuln/Report/Traffic/  │
     └─────────────────────────────────────────────┘   │ Audit/LLMProvider/    │
                                                        │ ScanNode             │
                                                        └──────────────────────┘
```

> 注：消息队列（NATS）、Redis、存储分层（主库 PostgreSQL 18 + pgvector / 图库 Neo4j / 对象存储 MinIO）与密钥库等中间件组件的选型与拓扑，详见 5.1.3 节。

#### 5.1.3 基础设施与中间件（Infrastructure & Middleware）

前文各功能（分布式扫描节点 F-12、实时可视化 F-04、全局限速栅栏、流量归档 F-13、长期记忆 F-14）都隐性依赖一组共享基础设施。为避免各模块各自选型造成割裂，本节集中规划消息队列、缓存/共享状态、存储分层与密钥管理。

**消息队列（Message Queue）**
- **任务分发与结果回传**：Orchestrator 将扫描/工具执行任务投递到队列，Scan Node Pool 异步消费执行并回传结果，解耦调度与执行，避免同步阻塞（节点池调度细节见 5.2.3.4）。
- **事件总线**：每一步 `AgentStep` 经 MQ 广播，供实时流（SSE/WebSocket）、审计（`AuditLog`）、流量记录（`TrafficRecord`）等多消费者订阅，实现关注点分离。
- **优先级与背压**：多目标并发时按优先级排队；超过全局并发上限时拒绝或延迟入队，配合 5.2.3.7 限速栅栏防止对目标造成 DoS。
- **可靠性**：至少一次投递 + 死信队列（DLQ），失败任务可重调度而不丢失（呼应 F-12 单节点故障重调度）。
- **选型建议**：**NATS**（轻量、原生支持 JetStream 持久化与多消费者扇出，契合工具执行结果推送 / SSE 事件总线场景）。

**Redis（缓存 · 共享状态 · 限速 · Pub/Sub）**
- **全局限速栅栏（5.2.3.7）**：以滑动窗口 / 令牌桶实现全局 QPS 与并发上限，是「高风险栅栏」限速能力的落地介质。
- **实时流 Pub/Sub**：`AgentStep` 经 Redis channel 广播，网关经 SSE/WebSocket 转推前端，支撑 F-04 实时可视化（1s 内呈现新步骤）。
- **缓存**：LLM 响应、工具结果指纹、攻击图谱热节点、报告渲染中间结果，降低重复计算与 LLM 成本。
- **共享状态 / 分布式锁**：扫描节点调度锁、任务状态机、幂等控制。
- **会话（可选）**：用户会话与临时状态。

**存储分层（Storage Tiers）**
- **主数据库（PostgreSQL 18）**：`Project` / `ScanTask` / `AgentStep` / `ToolCall` / `Vulnerability` / `Report` / `AuditLog` / `LLMProvider` / `ScanNode` 等结构化实体（见 5.1.6 数据库设计）；通过 **pgvector** 扩展在同一主库内承载记忆库向量检索，无需独立向量库。
- **图数据库（Neo4j 5.26 + Graphiti）**：攻击图谱节点与关系边（F-14）；Graphiti 提供时间感知的图谱记忆与实体解析。
- **向量检索（pgvector）**：记忆库 embedding 语义检索（RAG，F-14）直接在 PostgreSQL 18 主库完成。
- **对象存储（S3 / MinIO）**：流量全量请求/响应体、报告导出文件、大体积证据，冷热分层归档（呼应 F-13 容量策略）。

**密钥库（Secret Vault）**
- LLM Provider 凭据（`api_key_ref`）统一托管，不在任务或日志中明文落库（呼应 5.2.3.4 LLM 抽象层的凭据托管）。

**部署形态**：平台自身组件（编排核心、各微服务、MQ、Redis、存储等）容器化部署，支持 Kubernetes 编排；私有化提供 docker-compose 一键起栈（Postgres + Redis + MQ + 对象存储）。工具执行层**不强制容器化**——本地工具与扫描节点以主机本地进程运行（隔离方案见 5.1.1「工具执行隔离」），仅 Docker/K8s 用于承载平台服务。

**基础设施拓扑（Infra Topology）**

```
                          ┌─────────────────────────┐
                          │      API 网关 / 前端       │
                          └───────────┬─────────────┘
                                      │
                          ┌───────────▼─────────────┐
                          │   Agent Orchestrator     │
                          │  (Loop Controller)       │
                          └───┬───────────┬─────────┘
              ┌───────────────┘           └────────────────┐
              ▼                                            ▼
      ┌───────────────┐                          ┌──────────────────┐
      │  消息队列 MQ   │◄──── 任务分发 / 结果回传 ───►│  Scan Node Pool  │
      │    (NATS)    │                          │ (主机本地进程执行)  │
      └──────┬────────┘                          └──────────────────┘
             │ AgentStep 事件总线
             ▼
      ┌───────────────┐   限速/锁/PubSub   ┌──────────────┐
      │    Redis 8    │◄──────────────────►│  各微服务共享态  │
      └───────────────┘                    └──────────────┘
             │                                      │
   ┌─────────┴──────────┐              ┌─────────────┴──────────────┐
   │     存储分层         │              │      密钥库 (Vault)         │
   │ PG / 图库 / 向量 / 对象 │              │   LLM Provider 凭据托管     │
   └────────────────────┘              └────────────────────────────┘
```

#### 5.1.4 后端模块拆分

| 服务/模块             | 职责                                                                         | 主要依赖            |
| ---------------- | -------------------------------------------------------------------------- | --------------- |
| `api-gateway`     | 统一入口：鉴权、限流、AUP 确认、审计埋点；SSE/WebSocket 转推。                                      | Redis（限流/PubSub）  |
| `orchestrator`    | Agent Loop 状态机：Plan/Act/Observe/Judge/回写；记忆检索注入；工具路由。                                | Memory、Provider、MQ |
| `tool-runtime`    | 本地工具执行器：命令白名单校验、主机本地受限执行（资源限额 + 出口约束）、超时熔断、结果结构化；统一 `execute_tool(name, args)`        | Redis（锁）、Traffic     |
| `mcp-gateway`     | MCP Server 接入：连接管理、健康检查、`tools/list` 发现、`tools/call` 调用。                        | Traffic         |
| `node-scheduler`  | 扫描节点池调度：任务入队、节点扩缩容、心跳健康、失败重调度（DLQ）。                                        | MQ、Redis         |
| `memory-service`  | 长期记忆：攻击图谱（图库）读写、记忆库（pgvector 向量+RAG）写入/检索、攻击路径推导。                          | 图库、PostgreSQL(pgvector) |
| `traffic-service` | 流量统一埋点（req/resp 摘要 + 全量归档对象存储）、按 project 检索/回放、证据关联                       | 对象存储、PostgreSQL      |
| `report-service`  | 报告生成：CVSS 分级、证据链编排、PDF/JSON 导出。                                             | PostgreSQL、对象存储    |
| `provider-service`| 多 LLM 供应商抽象层：`chat(messages, tools)`，凭据托管（`api_key_ref`），token/成本计量。              | Vault、PostgreSQL   |
| `audit-service`   | 审计日志：全量操作/步骤/工具调用落库，不可篡改，责任追溯。                                              | PostgreSQL        |

#### 5.1.5 前端页面划分

前端路由与 4.1 信息架构一一对应：`/`（仪表盘）、`/projects` 与 `/projects/:id`（项目列表/详情五 Tab）、`/tasks/new` 与 `/tasks/:id`（新建向导/实时监控）、`/reports/:id`（报告详情）、`/tools`、`/mcp`、`/nodes`、`/providers`、`/memory`、`/settings/compliance`。实时流组件统一封装 `EventSource` 订阅任务 SSE 端点；图谱视图用力导向图库（如 Cytoscape / D3）渲染。

#### 5.1.6 数据库设计

**ER 图（实体关系）**

```
Project 1──* Target                (项目包含多个目标)
Project 1──* ScanTask              (项目包含多个扫描任务)
ScanTask 1──* AgentStep            (任务包含多轮循环步骤)
AgentStep 1──* ToolCall            (每步可调用多个工具)
AgentStep *──1 ScanTask
ScanTask 1──* Vulnerability        (任务发现多个漏洞)
Vulnerability *──* TrafficRecord   (漏洞关联流量证据，多对多)
ScanTask 1──* TrafficRecord        (任务产生多条流量)
Project 1──* TrafficRecord         (流量按项目归档，主索引)
Project 1──* AttackGraphNode       (项目级攻击图谱节点)
AttackGraphNode *──* AttackGraphEdge (节点间关系边)
Project 1──* AttackPath            (项目攻击路径)
AttackPath 1──* AttackGraphNode    (路径由节点组成)
Project 1──* MemoryEntry           (项目级记忆条目)
LLMProvider 1──* ScanTask          (任务选用某供应商)
ScanNode 1──* ScanTask             (节点执行任务)
AuditLog *──1 Project (可选)        (审计按项目归集)
```

**核心表结构（字段摘要）**

| 表                | 关键字段                                                                                     |
| ---------------- | ---------------------------------------------------------------------------------------- |
| `projects`       | id, name, owner_id, scope_cidr(出口白名单), created_at, status                                |
| `targets`        | id, project_id(FK), host, port, proto, discovered_at                                        |
| `scan_tasks`     | id, project_id(FK), strategy, llm_provider_id(FK), node_id(FK), status, aup_confirmed, created_at |
| `agent_steps`    | id, task_id(FK), idx, decision, tool_kind, model, tokens_in, tokens_out, created_at           |
| `tool_calls`     | id, step_id(FK), name, kind, args_json, duration_ms, status, output_summary                  |
| `vulnerabilities`| id, task_id(FK), target_id(FK), severity(CVSS), type, poc, evidence_ref, created_at           |
| `traffic_records`| id, project_id(FK), task_id(FK), step_id(FK), proto, method, endpoint, status_code, req_resp_ref(对象存储), created_at |
| `attack_graph_nodes` | id, project_id(FK), kind(asset/svc/vuln/cred), label, props_json                        |
| `attack_graph_edges` | id, project_id(FK), from_node, to_node, relation(可达/利用), confidence                  |
| `attack_paths`   | id, project_id(FK), node_ids_json, confidence, created_at                                   |
| `memory_entries` | id, project_id(FK), type(ttp/fingerprint/tactic), embedding(vector), content, confidence, created_at |
| `llm_providers`  | id, name, kind(openai/claude/self), api_key_ref(Vault), base_url, created_at                 |
| `scan_nodes`     | id, host, status, capacity, quota, last_heartbeat                                           |
| `audit_logs`     | id, project_id(FK), actor, action, detail_json, created_at                                  |

**索引策略**

| 表 / 对象            | 索引                                                                           | 说明                          |
| ----------------- | ---------------------------------------------------------------------------- | --------------------------- |
| `traffic_records` | `project_id` + `created_at`（复合，部分索引 `WHERE step_id IS NOT NULL`）                 | 项目级时间窗检索主路径；仅带证据关联的流量建部分索引   |
| `agent_steps`     | `(task_id, idx)`                                                              | 按任务顺序回放                     |
| `vulnerabilities` | `(task_id)`, `(project_id, severity)`, `(severity)` GIN（CVSS 向量） | 项目级漏洞分级聚合 + 报告分级聚合 |
| `memory_entries`  | `project_id` + `embedding` 向量索引（ivfflat / hnsw）                               | RAG 语义检索                    |
| `attack_graph_*`  | `(project_id)`；复杂遍历交图库（Neo4j Cypher / Graphiti）                                    | 图谱查询                        |
| `scan_tasks`      | `(project_id, status)`, `(llm_provider_id)`                                     | 项目任务列表与供应商成本聚合              |
| 大表分区             | `traffic_records` / `agent_steps` 按 `created_at` 按月分区                          | 控制单表体积，呼应 F-13 容量策略          |

> 图关系（AttackGraph）的复杂遍历交由图库（Neo4j 5.26 / Graphiti）承担；结构化实体与向量检索统一在 PostgreSQL 18（+ pgvector）完成。流量全量体量大，仅元数据入 PostgreSQL，原始 req/resp 体存对象存储并以 `vuln_id`/`step_id` 关联。

### 5.2 详细设计

#### 5.2.1 接口文档（API）

**通用错误码表**

| code    | HTTP | 含义              | 处理建议                   |
| ------- | ---- | --------------- | ---------------------- |
| 40001   | 400  | 请求参数校验失败        | 检查请求体字段与类型             |
| 40101   | 401  | 未认证 / Token 失效  | 重新登录                   |
| 40301   | 403  | 无权限（项目/操作）      | 检查成员角色与 AUP 确认状态        |
| 40901   | 409  | 任务状态冲突（如重复启动）   | 检查当前任务状态               |
| 42201   | 422  | 工具调用被栅栏拦截        | 查看 `guardrail` 字段，按需二次确认  |
| 42901   | 429  | 触发全局限速 / 并发上限   | 降级重试，配合令牌桶            |
| 50001   | 500  | 内部服务错误          | 查看 trace_id，联系平台管理员     |
| 50301   | 503  | 节点池无可用节点 / 过载   | 等待扩容或降低并发              |
| 50401   | 504  | 工具执行超时（>120s）   | 拆分目标或调高超时（需审批）        |

**重点端点**

| 方法 + 路径                  | 说明           | 关键请求体                                            | 返回 / 错误码                        |
| ------------------------ | ------------ | ------------------------------------------------- | ------------------------------ |
| `POST /api/tasks`        | 创建扫描任务       | `{project_id, targets[], strategy, llm_provider_id, aup_token}` | `201 {task_id}` / 40001,40301    |
| `GET /api/tasks/:id`     | 任务状态详情       | —                                                 | `{status, progress, cost}` / 404 |
| `GET /api/tasks/:id/stream` | SSE 实时步骤流  | —                                                 | `text/event-stream` AgentStep / 429 |
| `POST /api/tasks/:id/pause` `.../resume` `.../stop` | 暂停/继续/停止 | `{reason?}`                                        | `200 {ok}` / 40901               |
| `GET /api/reports/:id`   | 报告详情         | —                                                 | 报告对象 / 404                     |
| `GET /api/projects/:id/graph` | 攻击图谱       | `?min_confidence=`                                | `{nodes, edges}` / 40301         |
| `POST /api/memory/retrieve` | 记忆检索（RAG） | `{project_id, query, k}`                           | `{entries[]}` / 40001            |
| `GET /api/projects/:id/traffic` | 流量检索/回放  | `?task_id=&proto=&from=&to=`                       | `{records[], next}` / 40301      |

其余管理接口（工具注册、MCP 配置、节点管理、Provider 配置、合规栅栏配置、审计查询）以 REST 资源风格列出，由 `provider-service` 基于 Rust/utoipa 自动生成的 OpenAPI 文档补全。

#### 5.2.2 核心业务时序图

**(A) Agent Loop 闭环（Plan → Act → Observe → Judge → 回写）**

```
Web前端      API网关      Orchestrator      MemoryStore     LLM        ToolExec/MCP/Node   TrafficRecorder
  │ 启动任务    │            │                 │              │              │                  │
  │──────────►│───────────►│                 │              │              │                  │
  │            │            │─retrieve(mem)──►│              │              │                  │
  │            │            │◄─相关记忆/图谱────│              │              │                  │
  │            │            │─chat(plan+mem)────────────────►│              │                  │
  │            │            │◄─plan+tool_calls──│              │              │                  │
  │            │            │─route/execute─────────────────────────────────►│                  │
  │            │            │                 │              │              │─record traffic─►│
  │            │            │◄─result──────────│              │              │                  │
  │            │            │─chat(judge)────────────────────►│              │                  │
  │            │            │◄─continue/converge│              │              │                  │
  │            │            │─write(graph/mem)───────────────►│              │                  │
  │◄─SSE step─│◄──────────│                 │              │              │                  │
  │            │            │   … 循环直至 converge / max_iter / 用户停止 / 熔断 …    │
  │<══SSE done═══│<══════════│                  │              │              │                  │
```

**(B) 分布式扫描节点调度时序图（经 MQ 解耦）**

```
Orchestrator        NATS            ScanNode         Redis(锁/限速)      TrafficRecorder
   │                │                 │                    │                  │
   │─enqueue(task,target,permits)──>│                    │                  │
   │                │─XREAD──────────>│                    │                  │
   │                │                 │─acquire permit/lock────────────────>│
   │                │                 │<─ok─────────────────│                  │
   │                │                 │─exec tool (host-local)│                │
   │                │                 │─record traffic────────────────────────>│
   │                │<─XADD result────│                    │                  │
   │<─callback─────│                 │                    │                  │
   │  (失败) ── 经 DLQ 重调度到其他空闲节点，已完成的 AgentStep 不丢弃 ──│                  │
```

**(C) 长期记忆读写闭环（跨任务自增强）**

```
Plan 阶段：  Orchestrator ──retrieve(project_id, query_embedding, k)──► MemoryStore
            ◄── 相关 TTP/指纹/攻击路径/图谱热节点 注入 LLM 上下文 ──

Act/Observe/Judge：  执行工具、结构化观察、LLM 判断继续/收敛

回写阶段：  Orchestrator ──write(graph node/edge, attack_path, memory_entry+embedding)──► MemoryStore（持久化）

下一任务 Plan 阶段即可检索到本次写入  →  跨任务覆盖提升、越测越准（F-14）
```

#### 5.2.3 子系统详细设计

##### 5.2.3.1 Agent Loop（循环判断）设计

采用 ReAct 风格状态机，每轮迭代：

1. **Plan**：检索项目长期记忆（相似资产/已知路径/历史 TTP）注入上下文，LLM 产出下一步计划（含拟调用的工具与参数）。
2. **Act**：Loop Controller 校验工具是否在范围内 → 路由到 Local Executor / MCP Client / Scan Node 执行。
3. **Observe**：将工具返回结果结构化（截断/脱敏超长输出）。
4. **Judge**：LLM 判断——`continue` / `converge` / `escalate`（需用户确认高风险动作）；达到 `max_iterations` 或用户中止则强制退出。
5. **回写**：将新发现（图谱节点/边、攻击路径、有效打法）写入 Memory Store，供后续任务复用。

> 约束：`max_iterations` 默认 50，单步工具执行超时 120s，超限自动熔断并记录。

##### 5.2.3.2 本地工具调用

- 在受限执行环境下执行（低权限用户 + 资源限额 + 命令白名单 + 参数校验），禁用高危系统命令（rm -rf、:(){}/ 等）。工具以主机本地进程运行，不设 Docker 沙箱。
- 统一接口：`execute_tool(name, args) -> {status, output, meta}`。
- 预置工具示例：端口扫描、HTTP 指纹、目录探测、常见漏洞模板检测等。

##### 5.2.3.3 MCP 工具调用

- 支持 stdio（本地进程）与 SSE（远程服务）两种传输。
- 启动时执行 `tools/list` 发现可用工具并注册到工具目录；调用走 `tools/call`。
- 示例 MCP 服务：资产发现、漏洞情报查询、DNS/WHOIS、报告渲染。

##### 5.2.3.4 扩展设计要点

- **多 LLM 供应商抽象层（LLM Provider Abstraction）**
  - 在 Orchestrator 与具体模型之间引入统一接口 `chat(messages, tools) -> {content, tool_calls}`，屏蔽 OpenAI / Claude / 自建模型的协议差异。
  - 支持按**任务级别**或**单步策略**选择 Provider；完整记录每次调用的模型、token 消耗与成本，支撑预算告警。
  - 凭据统一托管于密钥库（`api_key_ref`），不在任务或日志中明文落库。
- **分布式扫描节点池（Scan Node Pool）**
  - Local Executor 调度到一组隔离执行节点（独立主机 / 轻量 VM），工具以**主机本地进程**运行、**不设 Docker 沙箱**；调度器按目标并发度弹性扩容、缩容。
  - 节点间通过消息队列异步接收任务、回传结果（消息队列选型与通信模型见 5.1.3）；平台统一管理网络出口白名单、资源配额与心跳健康检测。
  - 单节点故障不影响整体任务：任务可重调度到其他空闲节点，已完成的 AgentStep 不丢弃。

##### 5.2.3.5 项目级流量记录（Project-scoped Traffic Recording）

- **统一埋点**：在工具执行层（Local Executor / Scan Node / MCP Client）拦截每一次对外请求与响应，捕获协议、方法、端点、状态码、请求/响应摘要，并关联 `task_id` / `step_id` / `project_id`。
- **按项目归档**：写入 `TrafficRecord`，以 `project_id` 为主索引；支持按项目、任务、时间窗、协议多维检索与流量回放。
- **证据关联**：命中漏洞的步骤可一键跳转至对应流量记录，报告自动附带流量证据链。

**流量抓取机制（Traffic Capture Mechanism）**

文档早期版本仅声明「在工具执行层拦截每一次对外请求与响应」，未明确拦截的具体技术。本节补全落地方案——采用**两层混合抓取**，并与 v1.4 的「主机本地进程级隔离」天然咬合：

1. **主层 · HTTP(S) 拦截代理（mitmproxy 思路，自托管）**
   - 执行器在启动工具进程时，按任务注入 `http_proxy` / `https_proxy` 环境变量 + 平台自有 CA 证书（让 HTTPS 可解密）。
   - 这是 Web 渗透的绝对主力流量（HTTP/S 占绝大多数），可获取全量请求/响应体、做应用层解析；每条流直接用代理元数据打上 `project_id` / `task_id`。
   - 安全/审计人员习惯 Burp/ZAP 式代理视角，体验一致。

2. **兜底层 · eBPF / iptables REDIRECT 捕获**
   - 上述代理仅覆盖「愿意走代理」的流量；nmap 裸套接字（`-sS`）、自写脚本、不读 proxy 环境变量的工具会漏。
   - 用 eBPF（sockops / tc）或 iptables REDIRECT 将所有出站 TCP/UDP 重定向到本地捕获点，覆盖非代理感知流量与原始套接字。
   - **关键点**：v1.4 已把每个项目的工具跑在独立 cgroup / 网络 namespace 下，eBPF 可直接**按 cgroup 给流量打 `project_id` 标签**——隔离边界与抓取键是同一机制，无需 Docker 即可精确分级。

3. **可选 · pcap / tcpdump**：全量包级取证，默认关闭（体积爆炸、TLS 不透明），仅在需要司法级证据时开启。

```
工具进程(按 project 落入 cgroup/namespace)
   ├─ 走代理的 HTTP/S ──► 本地拦截代理(http_proxy+CA) ──► 打 project_id/task_id ──┐
   └─ 裸套接字/非代理流量 ──► eBPF/iptables REDIRECT ──► 本地捕获点(按 cgroup 打标) ─┤
                                                                                  ▼
                                             TrafficRecorder ──► TrafficRecord(元数据入PG) + 对象存储(全量req/resp)
```

> 抓取层与 5.2.3.7 的网络出口白名单共用同一套出口管控：出口白名单决定「能访问哪些授权目标」，抓取层负责「记录所有出网流量」，二者叠加构成可追溯边界。

##### 5.2.3.6 长期记忆系统（攻击图谱 + 记忆库）

- **攻击图谱（图库）**：节点类型 `asset / service / vulnerability / credential`；边关系 `可达 / 利用 / 属于`。复杂路径推理交由图库（Neo4j 5.26 Cypher；Graphiti 提供时间感知的实体解析与记忆编排）。
- **记忆库（向量 + RAG）**：将 TTP、指纹、有效打法向量化存储；Plan 阶段按 `query_embedding` 检索 top-k 注入 LLM 上下文。
- **攻击路径**：由图谱推导可达利用链路，标注置信度，作为报告与下一步规划依据。
- **作用域**：默认按 `project_id` 隔离；可选组织级共享，跨项目复用攻击模式（需权限控制）。
- **自增强闭环**：每次任务回写新节点/边/路径/记忆条目，下一任务 Plan 阶段即可检索复用，跨任务覆盖提升（F-14）。

##### 5.2.3.7 安全与合规（硬性要求）

- **操作者责任制（AUP）**：平台默认目标已获合法授权，不前置拦截；以可接受使用政策（AUP）明确授权责任归属，并在每次启动前展示免责声明供操作者确认（经法务审核）。
- **高风险操作栅栏（Guardrails）**：① 大规模/全网段扫描、写类/破坏性动作默认禁用或需二次确认；② 全局并发与速率上限（经 Redis 滑动窗口/令牌桶实现），防止对目标造成 DoS；③ 网络出口白名单，限制可触及的资产范围；④ 单步超时与自动熔断。
- **执行环境隔离（非 Docker 沙箱）**：本地工具与扫描节点以主机本地进程运行，不强制 Docker 容器。隔离由「主机本地受限执行」承载——低权限 OS 用户、cgroups/ulimit 资源限额、命令白名单、网络出口白名单（仅放行授权目标）；真正的安全边界是行为栅栏与出口管控，而非容器逃逸防护。
- **操作审计**：所有 Agent 步骤、工具调用、用户操作落审计日志，不可篡改，作为责任追溯依据。
- **数据脱敏**：报告中涉及敏感凭证、内网拓扑时提供脱敏选项。

---

## 6. 验收标准与埋点

### 6.1 功能验收标准

- [ ] 任务启动不做授权前置拦截，但启动前必须展示 AUP 免责声明且操作者需确认。
- [ ] 启动后 Agent Loop 可自主完成至少「发现→验证→收敛」基本路径，无需人工干预。
- [ ] 实时面板可在 1s 内呈现新步骤（流式延迟）。
- [ ] 暂停/继续/停止功能对任意进行中任务即时生效。
- [ ] 报告按 CVSS 正确分级，支持 PDF/JSON 导出且内容完整。
- [ ] 本地工具与至少一个 MCP 服务可在 UI 中注册、发现并成功调用。
- [ ] F-11：可在任务配置中切换不同 LLM 供应商并成功完成测试，token 消耗与成本被准确记录。
- [ ] F-12：提交多目标并发任务时，调度器能自动扩容扫描节点并在单节点故障时重调度，任务不中断。
- [ ] F-13：流量按项目归档，可按项目/任务/时间窗检索并回放，且能与对应漏洞证据关联。
- [ ] F-14：同一项目多任务间攻击图谱持续累积，新任务 Plan 阶段能检索到历史记忆并提升覆盖；记忆跨任务不丢失。

### 6.2 数据埋点（事件表）

| 事件                  | 触发时机        | 关键属性                                  |
| ------------------- | ----------- | ------------------------------------- |
| `project_created`   | 创建项目        | owner, name                           |
| `scan_created`      | 创建任务        | strategy, target_count, llm_provider  |
| `scan_started`      | 启动          | task_id                               |
| `agent_step`        | 每轮循环        | idx, decision, tool_kind              |
| `tool_called`        | 工具调用        | name, kind, duration_ms, status       |
| `vuln_found`        | 发现漏洞        | severity, type                        |
| `traffic_recorded`  | 记录一条流量      | project_id, protocol                  |
| `memory_written`    | 写入记忆        | project_id, type                      |
| `graph_updated`     | 图谱更新        | project_id, nodes, edges              |
| `attack_path_found` | 发现攻击路径      | project_id, confidence                |
| `scan_stopped`      | 停止          | reason(pause/user/auto)               |
| `report_exported`   | 导出          | format                                |
| `mcp_connected`     | MCP 连接成功    | server, tool_count                    |
| `llm_switched`      | 切换 LLM 供应商  | provider, task_id                     |
| `llm_cost`          | 单次 LLM 调用计费 | provider, tokens_in, tokens_out, cost |
| `node_scaled`       | 扫描节点扩缩容     | node_id, action, reason               |
| `node_heartbeat`    | 节点心跳        | node_id, status                       |

### 6.3 上线标准（Definition of Done）

- 核心链路（F-01~F-06、F-11、F-12、F-13、F-14）全部通过验收；
- 端到端压测：并发 5 个任务无崩溃，单任务平均收敛轮次可控；分布式节点在 10 目标并发下可自动扩容且单节点故障可重调度；
- 安全评审通过（高风险栅栏、执行环境隔离、审计 + AUP 免责声明）；
- 文档齐备（用户手册 + 部署手册 + API 文档）。

---

## 7. 风险与合规

| 风险             | 等级 | 应对                                                           |
| -------------- | -- | ------------------------------------------------------------ |
| 未授权测试的法律风险     | 高  | 操作者责任制 + AUP 免责声明前置确认 + 高风险栅栏 + 全程审计（责任可追溯）；免责声明与 AUP 需经法务审核 |
| 执行环境隔离不足导致主机受影响 | 高  | 低权限 OS 用户 + cgroups/ulimit 资源限额 + 网络出口强管控 + 主机网络分段（执行节点与编排核心网段隔离）；不依赖容器逃逸防护 |
| 分布式节点被攻陷成为跳板   | 高  | 节点最小权限 + 镜像签名 + 网络出口强管控 + 定期回收重建                             |
| LLM 误判导致漏报/误报  | 中  | 关键结论需人工复核；置信度标注                                              |
| LLM Token 成本失控 | 中  | 轮次上限、上下文压缩、预算告警                                              |
| MCP 第三方工具不可信   | 中  | 工具来源审批、调用隔离与输出校验                                            |
| 多供应商 LLM 行为不一致 | 中  | 统一接口契约测试、输出 schema 校验、降级到默认 Provider                         |
| 长期记忆数据膨胀 / 污染  | 中  | 记忆容量配额 + 置信度衰减 + 人工可校正 / 归档                                  |

---

## 8. 里程碑

| 阶段        | 范围                                              | 目标          |
| --------- | ----------------------------------------------- | ----------- |
| M1 内核     | F-01~F-05 + F-09 + F-10 基础（Agent Loop、单 LLM、本地工具、审计） | 可跑通单任务 Demo |
| M2 增强     | F-06~F-08 + MCP 接入 + 报告 + F-11 多 LLM 切换 + F-12 分布式节点 + F-13 项目流量记录 + F-14 长期记忆系统 | 内部可用版本      |
| M3 生产化    | 基础设施中间件（MQ/Redis/存储分层/Vault）、合规栅栏强化、可观测、私有化部署        | 可交付客户 / 私有化 |

---

## 9. 附录

### 9.1 术语表

- **ReAct**：Reason + Act 的 Agent 范式，本平台 Loop 的具体实现。
- **MCP**：Model Context Protocol，工具接入标准协议（stdio / SSE）。
- **AUP**：Acceptable Use Policy，可接受使用政策，明确授权与责任归属。
- **RAG**：Retrieval-Augmented Generation，检索增强生成，用于记忆库注入。

### 9.2 参考

- 架构图（5.1.2）、基础设施拓扑（5.1.3）、ER 图（5.1.6）、时序图（5.2.2）为本文档核心图示。
- 接口契约可由 `provider-service` 基于 Rust/utoipa 自动生成的 OpenAPI 文档补全。

### 9.3 关键决策记录

- **合规模型（v0.3）**：默认信任授权目标，不做前置拦截；以操作者责任制 + 高风险栅栏约束危险动作。
- **基础设施（v1.1）**：集中规划 MQ / Redis / 存储分层 / 密钥库，避免模块各自选型割裂。
- **方案设计（v1.2）**：补全概要设计（技术选型/模块拆分/数据库设计）与详细设计（接口文档/时序图/子系统）。
- **执行隔离（v1.4）**：移除工具执行层 Docker 沙箱约束，改为主机本地进程级隔离 + 行为栅栏；Docker/K8s 仅用于平台组件部署。
- **流量抓取（v1.5）**：明确流量抓取采用「HTTP 拦截代理为主 + eBPF/iptables 兜底、按 cgroup 打 project_id」的两层混合机制。
- **技术选型对齐（v1.6）**：技术选型对齐落地栈——后端 Rust+Axum、前端 Vue3+AntDV+Vben、PG18(+pgvector)、Neo4j5.26+Graphiti、Redis8、NATS、MinIO、自研 AiModelFactory（兼容 OpenAI，接入 DeepSeek v4-pro）、chrome-devtools-mcp、mitmproxy、宿主机原生 prlimit 执行（60+ 安全工具）；同步修正下游 FastAPI/Redis7/Neo4j5/PG15/Redis Streams 等过期引用。
