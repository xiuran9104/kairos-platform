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
        path: '/pent/sessions/:id',
        component: () => import('#/views/pent/agent-session/index.vue'),
        name: 'PentAgentSession',
        meta: {
          title: 'Agent Session',
          hideInMenu: true,
          activePath: '/pent/agent',
        },
      },
      {
        path: '/pent/assets',
        component: () => import('#/views/pent/assets/index.vue'),
        name: 'PentAssets',
        meta: { title: '资产中心', icon: 'lucide:server' },
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
    ],
  },
];

export default routes;

// Agent Session route
export const agentSessionRoute = {
  path: '/pent/agent-session',
  component: () => import('#/views/pent/agent-session/index.vue'),
  name: 'PentAgentConversation',
  meta: { title: 'Agent 对话', icon: 'lucide:message-circle' },
};
