# Pent 模块 UI 实现方案

## 1. 文档目标

本文只描述 Kairos Platform 中 Pent 模块的前端 UI 实现，不展开 Agent 内部推理、
数据库选型和微服务部署。

目标是让前端 AI 可以依据本文，在现有 Vben Admin、Vue 3、TypeScript 和
Ant Design Vue 技术栈中，逐页完成可交付的 Pent 安全智能平台。

本文中的“已搭建”表示页面、路由或 API 契约已经存在，不代表真实业务能力已经完成。
禁止使用硬编码统计数字或静态数组冒充接口实现。

## 2. UI 产品定位

Pent UI 不是普通扫描器控制台，也不是只有聊天框的 Agent 页面。它需要同时承担：

- 安全态势总览。
- Agent 任务发起、对话、干预和审批。
- 资产、漏洞、证据、端点和攻击路径归档。
- LLM、工具和网络流量审计。
- RMOS 记忆检索、反馈和路径回放。
- 工具注册、健康检查和调用质量分析。
- 工作区文件与报告交付。
- 模型、角色、预算和护栏策略配置。

UI 必须始终显示授权范围、运行状态和高风险操作状态，不能让用户误认为 Agent 可以绕过
平台策略自由执行。

## 3. 当前前端基础

当前目录：

```text
apps/web/apps/web-antd/src/
├─ api/pent/
│  ├─ index.ts
│  ├─ conversation.ts
│  ├─ assets.ts
│  ├─ traffic.ts
│  ├─ rmos.ts
│  ├─ tools.ts
│  ├─ guard.ts
│  ├─ workspace.ts
│  └─ settings.ts
└─ views/pent/
   ├─ dashboard/
   ├─ agent-start/
   ├─ agent-session/
   ├─ projects/
   ├─ tasks/
   ├─ runs/
   ├─ approvals/
   ├─ evidence/
   ├─ findings/
   ├─ assets/
   ├─ traffic/
   ├─ rmos/
   ├─ tools/
   ├─ guard/
   ├─ workspace/
   ├─ platform-settings/
   └─ components/
```

已经具备：

- Agent Session 创建和会话页面。
- 项目、任务、Run、审批、证据和漏洞基础页面。
- 资产列表/卡片双视图。
- 资产详情九页签骨架。
- LLM 流量、RMOS、工具、拦截、工作区和平台设置页面骨架。
- 动态数据库菜单。
- 对应的前端 API 文件。

尚未完成的页面使用“基础框架”提示，后续实现真实数据后应删除占位内容。

## 4. 菜单与信息架构

推荐最终菜单：

```text
安全智能
├─ 安全仪表盘
├─ Agent 工作台
│  ├─ 启动 Agent
│  ├─ Agent 会话
│  ├─ 任务
│  ├─ 运行
│  └─ 审批
├─ 资产中心
├─ 漏洞中心
├─ 证据中心
├─ LLM 流量
├─ RMOS 记忆
├─ 工具生态
├─ 拦截中心
├─ 工作区文件
└─ 平台设置
```

当前业务路由以数据库 `system_menu` 为事实来源。新增或修改菜单时必须同步：

1. Vue 页面组件。
2. `system_menu` 组件定位符。
3. 权限码。
4. 数据库升级 SQL。
5. `sql/postgresql/0001_initial.sql`。

本地 `router/routes/modules/pent.ts` 只用于开发结构参考，不能作为生产菜单的唯一来源。

## 5. 全局页面规范

### 5.1 页面布局

统一采用：

```text
页面标题区
├─ 标题、说明、状态
└─ 页面级操作

筛选/统计区
├─ 指标卡或筛选表单
└─ 快捷操作

主内容区
├─ 表格、卡片、图谱或时间线
└─ 分页

详情承载
├─ 独立详情页：复杂、需要分享 URL 的内容
└─ Drawer/Modal：快速查看、短表单和确认操作
```

### 5.2 通用状态

每个页面必须实现：

- 首次加载 Skeleton 或 Spin。
- 无数据 Empty。
- 请求失败 Result/Alert，并提供重试。
- 无权限 403。
- 分页、筛选和排序状态写入 URL Query。
- 删除、停止、清理等操作二次确认。
- 后台任务提交后显示任务 ID，不假装同步完成。

### 5.3 状态颜色

| 类型 | 颜色 |
|---|---|
| 正常、完成、已验证 | Green |
| 运行中、处理中 | Blue |
| 等待确认、待审批 | Orange |
| 失败、高危、已阻断 | Red |
| 已取消、未知、不可用 | Gray |

漏洞严重度固定使用：严重、 高危、 中危、 低危、 信息，不允许各页面自行创造另一套颜色。

### 5.4 实时数据

- Run、Agent 消息和执行事件优先使用 SSE。
- 仪表盘低频指标可以 15～30 秒轮询。
- 拦截记录可以 15 秒轮询，后续切换 SSE。
- 页面隐藏后降低刷新频率。
- SSE 重连必须使用最后事件 ID，避免重复消息。

## 6. 页面实现方案

## 6.1 安全仪表盘

路径：`/pent/dashboard`

页面结构：

1. 顶部六张指标卡：
   - 总任务
   - 漏洞总数
   - 资产数量
   - 工具调用量
   - LLM 调用量
   - Token 消耗
2. 第二行图表：
   - 漏洞等级分布饼图
   - 漏洞类型 TOP8 柱状图
   - 高频工具 TOP6
   - 模型角色调用分布
3. 第三行：
   - 高风险资产排行
   - 系统组件健康
4. 底部：
   - 实时任务活动流

组件拆分：

```text
dashboard/
├─ index.vue
├─ components/
│  ├─ MetricCards.vue
│  ├─ FindingSeverityChart.vue
│  ├─ FindingTypeChart.vue
│  ├─ ToolRankingChart.vue
│  ├─ ModelRoleChart.vue
│  ├─ AssetRiskRanking.vue
│  ├─ ComponentHealth.vue
│  └─ ActivityStream.vue
└─ composables/useDashboard.ts
```

验收：

- 不允许通过加载所有 Run 后在浏览器临时统计。
- 指标时间范围可切换。
- 图表空数据不会报错。
- 活动流断线后可以恢复。

## 6.2 Agent 启动页

路径：`/pent/agent`

表单分为四步：

1. 选择项目。
2. 输入自然语言目标和目标资产。
3. 配置时间、轮次、并发、工具和风险预算。
4. 展示 Scope Proposal，并要求用户明确确认。

UI 必须展示：

- 允许目标。
- 禁止目标。
- 允许工具范围。
- 高风险操作审批策略。
- 最大轮次、并发和执行时间。

提交后进入 Agent Session，不直接在浏览器执行扫描。

## 6.3 Agent Session

路径：

- `/pent/agent-session`
- `/pent/sessions/:id`

推荐三栏布局：

```text
左侧 240px          中间自适应             右侧 320px
Session 列表        对话与事件流           当前计划
搜索/状态筛选       消息/工具卡/审批卡     Scope
新建 Session        输入与快捷指令         预算和运行状态
```

消息类型必须有独立组件：

- 用户消息。
- Agent 文本。
- 规划 DAG/步骤。
- 工具调用。
- 工具结果。
- 审批请求。
- 范围变更请求。
- 错误和重试。
- 最终总结。

禁止把所有 payload 直接输出为 JSON 文本。JSON Viewer 仅用于“查看原始数据”。

运行中用户可执行：

- 追加只读指令。
- 请求复核。
- 暂停。
- 恢复。
- 停止。
- 申请扩大范围。

范围扩大必须重新确认，聊天消息不能直接改变执行策略。

## 6.4 资产中心

路径：

- `/pent/assets`
- `/pent/assets/:id`

列表页支持列表/卡片双视图。

字段：

- 资产名称。
- IP/URL。
- 内网、外网、DMZ。
- 开发语言。
- 技术栈摘要。
- 风险评分。
- 漏洞数量。
- 端口指纹。
- 业务标签。
- 最后扫描时间。

筛选：

- 关键字。
- 网络边界。
- 风险区间。
- 漏洞等级。
- 技术栈。
- 业务标签。
- 最近扫描时间。

卡片操作：

- 查看档案。
- 发起扫描。
- 发起 Agent 任务。
- 生成报告。

风险评分使用后端返回的 `risk_score`，现有人工 `weight` 只能作为业务权重展示。

### 资产详情九页签

#### 概览

- 百分制风险仪表盘。
- 风险构成。
- 基础属性。
- 网络暴露面。
- 技术栈标签云。
- 漏洞和端点摘要。
- 最近活动。

#### 报告

- 报告历史。
- 模板和语言选择。
- HTML 预览。
- DOCX/HTML 下载。
- 生成状态和失败原因。

#### 漏洞

- 漏洞等级、状态、置信度。
- 生命周期。
- 关联证据。
- 复测入口。

#### 端点

- Method、URL、参数、认证要求、来源。
- 去重前来源数量。
- 风险标签。
- 服务端分页和全文检索。

#### 指纹

- 服务、语言、框架、中间件、版本。
- 来源工具和置信度。
- 冲突指纹对比。

#### 原始发现

- 原始工具输出。
- 标准化结果。
- 解析警告。
- 原始文件下载。

#### 时间线

- 发现、扫描、验证、处置、复测和报告事件。
- 按 Run、工具和事件类型筛选。

#### 攻击图谱

- 资产、端点、漏洞、凭据、工具和证据节点。
- 缩放、聚焦、路径高亮和节点详情。
- 默认只读，不允许在图谱中直接执行工具。

#### 历史路径

- RMOS 历史路径列表。
- 侦察→扫描→验证步骤回放。
- 成功/失败步骤。
- 工具输入摘要、结果和证据引用。

组件目录：

```text
assets/
├─ index.vue
├─ detail.vue
├─ components/
│  ├─ AssetFilters.vue
│  ├─ AssetTable.vue
│  ├─ AssetCardGrid.vue
│  ├─ RiskScorePanel.vue
│  ├─ TechStackSummary.vue
│  └─ AssetActions.vue
└─ tabs/
   ├─ OverviewTab.vue
   ├─ ReportsTab.vue
   ├─ FindingsTab.vue
   ├─ EndpointsTab.vue
   ├─ FingerprintsTab.vue
   ├─ DiscoveriesTab.vue
   ├─ TimelineTab.vue
   ├─ AttackGraphTab.vue
   └─ HistoryPathsTab.vue
```

## 6.5 漏洞中心

路径：`/pent/findings`

列表顶部：

- 五级漏洞统计。
- 状态统计。
- 待人工复核数量。

筛选：

- 等级。
- 状态。
- 资产。
- 漏洞类型。
- 置信度。
- 来源工具。
- 时间范围。

批量操作：

- 标记误报。
- 忽略。
- 发起复测。
- 调整等级。
- 创建工单。

详情使用 Drawer 或独立页面，包含：

- 请求包和响应包。
- PoC。
- 漏洞描述、影响和修复建议。
- 生命周期四阶段。
- LLM 研判记录。
- 扫描器原始输出。
- Run、Toolcall、Evidence 关联。

## 6.6 证据中心

路径：`/pent/evidence`

证据卡片/表格字段：

- 类型。
- 关联资产和漏洞。
- Run/Toolcall。
- 内容摘要。
- 哈希。
- 存储位置。
- 采集时间。

支持安全预览文本、JSON、HTTP 报文和图片。二进制文件只提供元数据和下载，不在浏览器
中直接执行或渲染不可信 HTML。

## 6.7 LLM 流量日志

路径：`/pent/traffic`

筛选栏：

- Method。
- URL。
- 状态码。
- 来源。
- 耗时区间。
- RunID。
- 时间范围。
- 全文关键字。

表格字段：

- 时间。
- Method。
- URL。
- 状态码。
- 来源。
- 耗时。
- 请求/响应大小。
- RunID。
- 流量类型。

详情 Drawer：

- 基本信息。
- 请求头。
- 请求体。
- 响应头。
- 响应体。
- 脱敏字段说明。
- 专属 UA。
- 关联 Session/Run/Toolcall。

长报文使用虚拟滚动或代码编辑器，默认折叠，禁止一次性渲染超大响应。

## 6.8 RMOS 记忆

路径：`/pent/rmos`

页面使用四个页签：

1. 记忆管理。
2. 攻击路径图谱。
3. 步骤回放。
4. 记忆配置。

记忆列表字段：

- 摘要。
- 记忆类型。
- 置信度。
- 验证状态。
- 来源 Run。
- 召回次数。
- 最近使用时间。
- 有用/无用反馈。

置信度：

- 已验证。
- 待确认。
- 理论。

反馈提交后使用乐观更新，但失败必须回滚并提示。

攻击路径图谱和资产详情中的图谱共用基础图组件，不复制两套图引擎。

## 6.9 工具生态

路径：`/pent/tools`

顶部指标：

- 总工具数量。
- 内置工具数量。
- 健康工具数量。
- 累计调用次数。

页面页签：

1. 工具目录。
2. 健康状态。
3. 调用分析。
4. 版本管理。

工具目录按 namespace 和 source 分组：

- scan
- http
- exec
- memory
- asset
- report
- builtin
- container
- mcp
- http

工具详情：

- 名称、描述、标签。
- Function Schema。
- 风险等级。
- 执行后端。
- 超时和并发限制。
- 当前版本。
- 成功率和延迟。
- 最近健康检查。

高风险工具的启停、版本切换必须二次确认并显示权限要求。

## 6.10 拦截中心

路径：`/pent/guard`

上半部分：

- 静态规则。
- LLM 语义审计规则。
- 默认动作。
- 规则版本。

下半部分：

- 拦截记录表格。
- 15 秒自动刷新开关。
- 命中详情 Drawer。

详情展示：

- 原始请求或命令。
- 脱敏后参数。
- 命中规则。
- 语义模型判定。
- 最终确定性裁决。
- Run、工具和操作者。

规则编辑页面必须区分“保存草稿”和“发布版本”。

## 6.11 工作区文件

路径：`/pent/workspace`

三栏结构：

```text
项目/Run 树 | 文件列表 | 文件预览/元数据
```

支持：

- 按项目和 Run 查看。
- 文件类型、大小、时间和来源筛选。
- 文本、JSON、图片和报告预览。
- 下载。
- 归档。
- 受控清理。
- 存储配额和占用趋势。

“清空空间”必须改为范围明确的清理向导，不提供无范围的一键删除。

## 6.12 平台设置

路径：`/pent/platform-settings`

使用四个页签：

1. 模型服务商。
2. 模型角色绑定。
3. Agent 运行策略。
4. 成本与配额。

模型服务商表单：

- 服务商类型。
- Base URL。
- API Key。
- 可用模型。
- 连接测试。
- 启用状态。

API Key 只允许输入和覆盖，不允许接口回显明文。

角色绑定：

- Meta Planner。
- Stage Planner。
- Step Planner。
- PreFlect。
- Reflector。
- 元认知控制器。
- 感知/归一化。
- 摘要与压缩。

运行策略：

- 最大轮次。
- 最大并发。
- 阶段超时。
- 最大 Token。
- 最大费用。
- 重规划阈值。
- 连续失败停止条件。

## 7. 前端 API 组织

每个业务域使用独立 API 文件：

| 业务域 | 文件 |
|---|---|
| Session/Run | `api/pent/index.ts`、`conversation.ts` |
| 资产档案 | `api/pent/assets.ts` |
| LLM 流量 | `api/pent/traffic.ts` |
| RMOS | `api/pent/rmos.ts` |
| 工具生态 | `api/pent/tools.ts` |
| 拦截中心 | `api/pent/guard.ts` |
| 工作区 | `api/pent/workspace.ts` |
| 平台设置 | `api/pent/settings.ts` |

规则：

- 禁止页面中直接拼接 `fetch`。
- 禁止在多个页面重复定义同一个 DTO。
- `any` 只允许用于尚未冻结的过渡接口，完成任务时必须替换。
- 分页统一返回 `list`、`total`、`page`、`page_size`。
- 时间统一使用 ISO 8601。
- 枚举值在 API 层定义，页面只负责映射显示。

## 8. 状态管理

优先使用页面级 composable，不建立一个包含全部 Pent 数据的巨型 Store。

建议：

```text
composables/
├─ usePentSession.ts
├─ useRunEventStream.ts
├─ useAssetProfile.ts
├─ useTrafficQuery.ts
├─ useRmosGraph.ts
└─ useToolHealth.ts
```

Pinia 只存放跨页面且需要长期保留的状态：

- 当前项目。
- 最近访问 Session。
- 用户选择的默认时间范围。
- 页面展示偏好。

服务端数据优先在页面/composable 中加载，不能把所有列表永久缓存到全局 Store。

## 9. 权限与可见性

页面权限：

- `pent:dashboard:read`
- `pent:agent:chat`
- `pent:asset:read`
- `pent:finding:read`
- `pent:evidence:read`
- `pent:traffic:read`
- `pent:rmos:read`
- `pent:tool:read`
- `pent:guard:read`
- `pent:workspace:read`
- `pent:settings:read`

按钮需要更细的写权限，例如：

- `pent:asset:scan`
- `pent:finding:update`
- `pent:tool:manage`
- `pent:guard:manage`
- `pent:workspace:delete`
- `pent:settings:update`

前端权限只控制显示和交互，后端必须再次鉴权。

## 10. UI 实施任务

### P0：形成可用闭环

| 编号 | 页面任务 | 验收 |
|---|---|---|
| UI-P0-01 | 重构安全仪表盘 | 六类指标和四类图表均来自聚合 API |
| UI-P0-02 | 完善资产列表 | 双视图、完整字段、服务端筛选分页 |
| UI-P0-03 | 实现资产概览 | 风险评分、技术栈、暴露面和活动 |
| UI-P0-04 | 实现漏洞页 | 筛选、批量处置、生命周期和证据链 |
| UI-P0-05 | 实现报告页 | HTML 预览、生成状态和下载 |
| UI-P0-06 | 实现模型设置 | 服务商、角色绑定、连接测试和脱敏 |

### P1：自动化和审计

| 编号 | 页面任务 | 验收 |
|---|---|---|
| UI-P1-01 | LLM 流量日志 | 服务端分页、多条件检索、报文详情 |
| UI-P1-02 | 工具生态 | 分类、健康、调用指标和版本操作 |
| UI-P1-03 | RMOS 记忆列表 | 置信度、来源、反馈和检索 |
| UI-P1-04 | 攻击图谱 | 公共图组件、节点详情和路径高亮 |
| UI-P1-05 | 历史路径回放 | 步骤、工具、结果和证据同步回放 |
| UI-P1-06 | Agent Session 重构 | 三栏工作台、结构化消息和断线恢复 |

### P2：安全管控和交付

| 编号 | 页面任务 | 验收 |
|---|---|---|
| UI-P2-01 | 拦截中心 | 双规则配置、版本发布、记录详情 |
| UI-P2-02 | 工作区文件 | 隔离文件树、预览、配额和受控清理 |
| UI-P2-03 | DOCX 报告交付 | 报告历史、下载、失败重试 |
| UI-P2-04 | 实时活动流 | SSE 重连、去重和页面隐藏降频 |
| UI-P2-05 | 全模块权限 | 菜单、按钮和无权限状态完整 |

## 11. 单任务实现要求

其他 AI 每次只领取一个 UI 任务，并完成：

1. 检查对应 API 是否真实存在。
2. 将页面拆为清晰的组件和 composable。
3. 补齐 Loading、Empty、Error、Forbidden 状态。
4. 实现分页、筛选、排序和 URL 状态同步。
5. 删除该页面的占位统计和“基础框架”提示。
6. 检查菜单组件路径和权限。
7. 运行类型检查和生产构建。
8. 在交付说明中列出仍依赖的后端接口。

不得：

- 在页面中硬编码成功率、Token、费用和工具数量。
- 用前端静态数组模拟生产数据并宣布完成。
- 将多个业务域写入一个 `index.vue`。
- 在浏览器直接执行 Shell 或扫描器。
- 绕过 Agent Scope、审批或护栏接口。
- 在日志、错误提示或页面状态中显示 API Key。

## 12. UI 验收标准

Pent UI 首版完成必须满足：

1. 所有菜单均可正常导航且刷新后不 404。
2. 资产列表和九页签详情使用真实接口。
3. 任一漏洞可跳转到资产、Run、工具调用和证据。
4. Agent Session 可以重连、恢复消息和运行事件。
5. 高风险工具请求显示审批状态，未审批不能显示为已执行。
6. LLM 流量支持服务端分页和完整报文详情。
7. RMOS 记忆可以检索、反馈和回放历史路径。
8. 工具生态显示真实健康状态和调用指标。
9. 工作区文件符合项目和租户隔离。
10. 模型密钥不回显，设置变更有成功/失败反馈。
11. 所有页面具备加载、空、错和无权限状态。
12. 通过：

```bash
pnpm --dir apps/web --filter @vben/web-antd run typecheck
pnpm --dir apps/web --filter @vben/web-antd run build
```

## 13. 相关文档

- [Pent Agent Web 微服务迁移方案](./pent-agent-service-plan.md)
- [Pent 智能渗透平台实现方案](./Pent智能渗透平台实现方案.md)
- [Pent 成熟工具路由框架实施任务书](./Pent成熟工具路由框架实施任务书.md)
- [Pent 成熟 LLM 对话运行时实施任务书](./Pent成熟LLM对话运行时实施任务书.md)
- [Pent H-PER-M 认知运行时实施任务书](./Pent-H-PER-M认知运行时实施任务书.md)
