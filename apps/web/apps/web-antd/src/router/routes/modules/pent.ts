import type { RouteRecordRaw } from 'vue-router';

const routes: RouteRecordRaw[] = [
  {
    path: '/pent',
    name: 'Pent',
    meta: { title: '安全智能', icon: 'lucide:bot', order: 25 },
    children: [
      {
        path: '/pent/dashboard',
        component: () => import('#/views/pent/dashboard/index.vue'),
        name: 'PentDashboard',
        meta: { title: '仪表盘', icon: 'lucide:gauge' },
      },
      {
        path: '/pent/agent',
        component: () => import('#/views/pent/agent-start/index.vue'),
        name: 'PentAgentStart',
        meta: { title: '启动 Agent', icon: 'lucide:message-square-code' },
      },
      {
        path: '/pent/tasks/:id',
        component: () => import('#/views/pent/task-monitor/index.vue'),
        name: 'PentTaskMonitor',
        meta: {
          title: '任务监控',
          hideInMenu: true,
          activePath: '/pent/tasks',
        },
      },
      {
        path: '/pent/projects',
        component: () => import('#/views/pent/projects/index.vue'),
        name: 'PentProjects',
        meta: { title: '项目', icon: 'lucide:folder-git' },
      },
      {
        path: '/pent/tasks',
        component: () => import('#/views/pent/tasks/index.vue'),
        name: 'PentTasks',
        meta: { title: '任务', icon: 'lucide:list-checks' },
      },
      {
        path: '/pent/runs',
        component: () => import('#/views/pent/runs/index.vue'),
        name: 'PentRuns',
        meta: { title: '运行', icon: 'lucide:play-circle' },
      },
      {
        path: '/pent/findings',
        component: () => import('#/views/pent/findings/index.vue'),
        name: 'PentFindings',
        meta: { title: '漏洞', icon: 'lucide:alert-triangle' },
      },
      {
        path: '/pent/settings',
        component: () => import('#/views/pent/platform-settings/index.vue'),
        name: 'PentSettings',
        meta: { title: '平台设置', icon: 'lucide:settings' },
      },
      {
        path: '/pent/traffic',
        component: () => import('#/views/pent/traffic/index.vue'),
        name: 'PentTraffic',
        meta: { title: 'LLM 流量', icon: 'lucide:network' },
      },
      {
        path: '/pent/rmos',
        component: () => import('#/views/pent/rmos/index.vue'),
        name: 'PentRmos',
        meta: { title: 'RMOS 记忆', icon: 'lucide:brain-circuit' },
      },
      {
        path: '/pent/tools',
        component: () => import('#/views/pent/tools/index.vue'),
        name: 'PentTools',
        meta: { title: '工具生态', icon: 'lucide:wrench' },
      },
      {
        path: '/pent/guard',
        component: () => import('#/views/pent/guard/index.vue'),
        name: 'PentGuard',
        meta: { title: '拦截中心', icon: 'lucide:shield-ban', hideInMenu: true },
      },
      {
        path: '/pent/hitl',
        component: () => import('#/views/pent/hitl/index.vue'),
        name: 'PentHitl',
        meta: { title: 'AI 围栏', icon: 'lucide:shield-alert' },
      },
      // ── M2/M3 新增页面 ───────────────────────────────────────────
      {
        path: '/pent/nodes',
        component: () => import('#/views/pent/nodes/index.vue'),
        name: 'PentNodes',
        meta: { title: '扫描节点', icon: 'lucide:server' },
      },
      {
        path: '/pent/providers',
        component: () => import('#/views/pent/providers/index.vue'),
        name: 'PentProviders',
        meta: { title: 'LLM 供应商', icon: 'lucide:cpu' },
      },
      {
        path: '/pent/compliance',
        component: () => import('#/views/pent/compliance/index.vue'),
        name: 'PentCompliance',
        meta: { title: '合规栅栏', icon: 'lucide:shield-check' },
      },
      {
        path: '/pent/memory',
        component: () => import('#/views/pent/global-memory/index.vue'),
        name: 'PentGlobalMemory',
        meta: { title: '全局记忆', icon: 'lucide:database' },
      },
      {
        path: '/pent/graph',
        component: () => import('#/views/pent/attack-graph/index.vue'),
        name: 'PentAttackGraph',
        meta: { title: '攻击图谱', icon: 'lucide:git-graph' },
      },
      {
        path: '/pent/reports/:id',
        component: () => import('#/views/pent/report-detail/index.vue'),
        name: 'PentReportDetail',
        meta: { title: '报告详情', hideInMenu: true },
      },
      // ── 能力中心 ─────────────────────────────────────────────────
      {
        path: '/pent/capability',
        name: 'PentCapability',
        meta: { title: '能力中心', icon: 'lucide:boxes' },
        children: [
          {
            path: '/pent/capability/mcp-monitor',
            component: () => import('#/views/pent/capability/mcp-monitor/index.vue'),
            name: 'PentCapabilityMcpMonitor',
            meta: { title: 'MCP 监控', icon: 'lucide:activity' },
          },
          {
            path: '/pent/capability/mcp-management',
            component: () => import('#/views/pent/capability/mcp-management/index.vue'),
            name: 'PentCapabilityMcpManagement',
            meta: { title: 'MCP 管理', icon: 'lucide:plug-zap' },
          },
          {
            path: '/pent/capability/knowledge',
            component: () => import('#/views/pent/capability/knowledge/index.vue'),
            name: 'PentCapabilityKnowledge',
            meta: { title: '知识管理', icon: 'lucide:book-open' },
          },
          {
            path: '/pent/capability/knowledge-logs',
            component: () => import('#/views/pent/capability/knowledge-logs/index.vue'),
            name: 'PentCapabilityKnowledgeLogs',
            meta: { title: '检索历史', icon: 'lucide:history' },
          },
          {
            path: '/pent/capability/skills',
            component: () => import('#/views/pent/capability/skills/index.vue'),
            name: 'PentCapabilitySkills',
            meta: { title: '技能管理', icon: 'lucide:file-cog' },
          },
          {
            path: '/pent/capability/skills-monitor',
            component: () => import('#/views/pent/capability/skills-monitor/index.vue'),
            name: 'PentCapabilitySkillsMonitor',
            meta: { title: '技能状态', icon: 'lucide:bar-chart-3' },
          },
          {
            path: '/pent/capability/agents',
            component: () => import('#/views/pent/capability/agents/index.vue'),
            name: 'PentCapabilityAgents',
            meta: { title: '智能体管理', icon: 'lucide:bot' },
          },
          {
            path: '/pent/capability/roles',
            component: () => import('#/views/pent/capability/roles/index.vue'),
            name: 'PentCapabilityRoles',
            meta: { title: '角色管理', icon: 'lucide:users' },
          },
        ],
      },
      // ── 安全作业 ─────────────────────────────────────────────────
      {
        path: '/pent/ops',
        name: 'PentOps',
        meta: { title: '安全作业', icon: 'lucide:crosshair' },
        children: [
          {
            path: '/pent/ops/asset-overview',
            component: () => import('#/views/pent/ops/asset-overview/index.vue'),
            name: 'PentOpsAssetOverview',
            meta: { title: '资产概览', icon: 'lucide:layout-dashboard' },
          },
          {
            path: '/pent/ops/asset-library',
            component: () => import('#/views/pent/ops/asset-library/index.vue'),
            name: 'PentOpsAssetLibrary',
            meta: { title: '资产库', icon: 'lucide:database' },
          },
          {
            path: '/pent/ops/info-collect',
            component: () => import('#/views/pent/ops/info-collect/index.vue'),
            name: 'PentOpsInfoCollect',
            meta: { title: '信息收集', icon: 'lucide:radar' },
          },
          {
            path: '/pent/ops/supply-chain',
            component: () => import('#/views/pent/ops/supply-chain/index.vue'),
            name: 'PentOpsSupplyChain',
            meta: { title: '股权穿透', icon: 'lucide:network' },
          },
          {
            path: '/pent/ops/supply-network',
            component: () => import('#/views/pent/ops/supply-network/index.vue'),
            name: 'PentOpsSupplyNetwork',
            meta: { title: '供应链', icon: 'lucide:share-2' },
          },
          {
            path: '/pent/ops/workflows',
            component: () => import('#/views/pent/ops/workflows/index.vue'),
            name: 'PentOpsWorkflows',
            meta: { title: '工作流', icon: 'lucide:workflow' },
          },
          {
            path: '/pent/ops/chat-files',
            component: () => import('#/views/pent/ops/chat-files/index.vue'),
            name: 'PentOpsChatFiles',
            meta: { title: '文件管理', icon: 'lucide:folder-open' },
          },
        ],
      },
      // Also add /pent/mcp as a placeholder linking to existing MCP config
      {
        path: '/pent/mcp',
        component: () => import('#/views/pent/capability/mcp-management/index.vue'),
        name: 'PentMcp',
        meta: { title: 'MCP 管理', icon: 'lucide:plug-zap' },
      },
    ],
  },
];

export default routes;
