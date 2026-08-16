<script lang="ts" setup>
import type { EchartsUIType } from '@vben/plugins/echarts';

import { computed, onMounted, ref, watch } from 'vue';
import { useRouter } from 'vue-router';

import { EchartsUI, useEcharts } from '@vben/plugins/echarts';

import { getChatRolePage } from '#/api/ai/model/chatRole';
import {
  getPentAgents,
  getPentMcpServers,
  getPentMonitor,
  getPentProjectFindings,
  getPentProjects,
  getPentRuns,
  getPentSessions,
  getPentSkills,
  getPentTools,
} from '#/api/pent';

const router = useRouter();

// ── state ──────────────────────────────────────────────────────────────────
const loading = ref(false);
const lastUpdated = ref('');

const sessions = ref<any[]>([]);
const projects = ref<any[]>([]);
const runs = ref<any[]>([]);
const findings = ref<any[]>([]);
const monitorSummary = ref<any>({});
const topTools = ref<any[]>([]);
const counts = ref({
  agents: 0,
  mcpServers: 0,
  roles: 0,
  skills: 0,
  tools: 0,
});

const sevStatusFilter = ref('');

// ── constants ────────────────────────────────────────────────────────────────
const SEVERITIES = ['critical', 'high', 'medium', 'low', 'info'] as const;
const sevLabel: Record<string, string> = {
  critical: '严重',
  high: '高危',
  medium: '中危',
  low: '低危',
  info: '信息',
};
const sevHex: Record<string, string> = {
  critical: '#dc2626',
  high: '#ea580c',
  medium: '#ca8a04',
  low: '#16a34a',
  info: '#6b7280',
};
const sevStatusOptions = [
  { label: '全部状态', value: '' },
  { label: '待处理', value: 'open' },
  { label: '已确认', value: 'confirmed' },
  { label: '已修复', value: 'fixed' },
  { label: '已忽略', value: 'ignored' },
  { label: '误报', value: 'false_positive' },
];

// ── data loading ───────────────────────────────────────────────────────────
async function fetchAll() {
  loading.value = true;
  try {
    const [sess, projs, runList, monitor, skills, agents, tools, mcpServers, roles] =
      await Promise.all([
        getPentSessions().catch(() => []),
        getPentProjects().catch(() => []),
        getPentRuns().catch(() => []),
        getPentMonitor({ page_size: 1 }).catch(() => null),
        getPentSkills().catch(() => []),
        getPentAgents().catch(() => []),
        getPentTools().catch(() => []),
        getPentMcpServers().catch(() => []),
        getChatRolePage({ pageNo: 1, pageSize: 1 }).catch(() => null),
      ]);
    sessions.value = (sess as any[]) || [];
    projects.value = (projs as any[]) || [];
    runs.value = (runList as any[]) || [];
    monitorSummary.value = (monitor as any)?.summary || {};
    topTools.value = (monitor as any)?.top_tools || [];
    counts.value = {
      agents: ((agents as any[]) || []).length,
      mcpServers: ((mcpServers as any[]) || []).length,
      roles: (roles as any)?.total || 0,
      skills: ((skills as any[]) || []).length,
      tools: ((tools as any[]) || []).length,
    };

    // 漏洞：逐项目合并（与漏洞管理页同一口径）
    const perProject = await Promise.all(
      projects.value.map(async (p) => {
        try {
          const resp: any = await getPentProjectFindings(p.id);
          return (resp?.items ?? []) as any[];
        } catch {
          return [];
        }
      }),
    );
    findings.value = perProject.flat();
    lastUpdated.value = new Date().toLocaleTimeString();
  } finally {
    loading.value = false;
  }
}

onMounted(fetchAll);

// ── KPI ──────────────────────────────────────────────────────────────────
const runningSessions = computed(
  () =>
    sessions.value.filter((s) => ['active', 'running'].includes(s.status)).length,
);
const vulnTotal = computed(() => findings.value.length);
const openFindings = computed(
  () => findings.value.filter((f) => f.status === 'open').length,
);
const criticalCount = computed(
  () => findings.value.filter((f) => f.severity === 'critical' && f.status !== 'false_positive').length,
);
const toolCalls = computed(() => monitorSummary.value.total_calls || 0);
const successRate = computed(() => {
  const { total_calls, success_calls } = monitorSummary.value;
  if (!total_calls) return null;
  return ((success_calls / total_calls) * 100).toFixed(1);
});
const rateHealthy = computed(() => Number(successRate.value ?? 100) >= 95);

// ── 严重度分布（带状态筛选）──────────────────────────────────────────────────
const filteredFindings = computed(() =>
  sevStatusFilter.value
    ? findings.value.filter((f) => f.status === sevStatusFilter.value)
    : findings.value,
);
const filteredTotal = computed(() => filteredFindings.value.length);
function sevCount(sev: string) {
  return filteredFindings.value.filter((f) => (f.severity || 'info') === sev).length;
}
function sevPct(sev: string) {
  if (!filteredTotal.value) return '0%';
  return `${Math.round((sevCount(sev) / filteredTotal.value) * 100)}%`;
}
const latestFindingTime = computed(() => {
  const times = filteredFindings.value.map((f) => f.created_at).filter(Boolean).sort();
  const t = times.at(-1);
  return t ? t.slice(0, 16).replace('T', ' ') : '暂无';
});
const fixedRate = computed(() => {
  if (!filteredTotal.value) return null;
  const fixed = filteredFindings.value.filter((f) => f.status === 'fixed').length;
  return { pct: Math.round((fixed / filteredTotal.value) * 100), fixed };
});

// ── 图表 ──────────────────────────────────────────────────────────────────
const donutRef = ref<EchartsUIType>();
const { renderEcharts: renderDonutChart } = useEcharts(donutRef);
const barRef = ref<EchartsUIType>();
const { renderEcharts: renderBarChart } = useEcharts(barRef);

function renderDonut() {
  const data = SEVERITIES.map((sev) => ({
    itemStyle: { color: sevHex[sev] },
    name: sevLabel[sev],
    value: sevCount(sev),
  })).filter((d) => d.value > 0);
  renderDonutChart({
    series: [
      {
        avoidLabelOverlap: true,
        data,
        label: { color: 'var(--ant-color-text-secondary)', fontSize: 12 },
        radius: ['55%', '80%'],
        type: 'pie',
      },
    ],
    tooltip: { trigger: 'item' },
  });
}

function renderBar() {
  const list = topTools.value.slice(0, 10);
  renderBarChart({
    grid: { bottom: 24, left: 48, right: 16, top: 16 },
    series: [
      {
        data: list.map((t) => t.calls),
        itemStyle: { color: '#3b82f6' },
        type: 'bar',
      },
    ],
    tooltip: {},
    xAxis: {
      axisLabel: { color: 'var(--ant-color-text-tertiary)', rotate: 30 },
      data: list.map((t) => t.tool_name),
      type: 'category',
    },
    yAxis: {
      axisLabel: { color: 'var(--ant-color-text-tertiary)' },
      splitLine: { lineStyle: { color: 'var(--ant-color-border-secondary)' } },
      type: 'value',
    },
  });
}

watch([filteredFindings], () => renderDonut());
watch([topTools], () => renderBar());

// ── 推荐操作（客户端规则）─────────────────────────────────────────────────────
const recommendations = computed(() => {
  const list: { action: string; path: string; title: string }[] = [];
  if (openFindings.value > 0) {
    list.push({
      action: '去审核',
      path: '/pent/findings',
      title: `${openFindings.value} 个漏洞待审核`,
    });
  }
  if (criticalCount.value > 0) {
    list.push({
      action: '查看严重漏洞',
      path: '/pent/findings',
      title: `${criticalCount.value} 个严重漏洞待处置`,
    });
  }
  if (!rateHealthy.value && toolCalls.value > 0) {
    list.push({
      action: '查看 MCP 监控',
      path: '/pent/capability/mcp-monitor',
      title: '工具执行成功率偏低',
    });
  }
  if (projects.value.length === 0) {
    list.push({ action: '新建项目', path: '/pent/projects', title: '还没有项目，先创建一个' });
  }
  if (sessions.value.length === 0) {
    list.push({ action: '发起对话', path: '/pent/agent', title: '还没有对话，发起一次渗透测试对话' });
  }
  return list;
});

// ── 最近事件（取 runs 最近状态）───────────────────────────────────────────────
const recentEvents = computed(() =>
  [...runs.value]
    .sort((a, b) => (b.updated_at || b.created_at || '').localeCompare(a.updated_at || a.created_at || ''))
    .slice(0, 8)
    .map((r) => ({
      color:
        r.status === 'completed'
          ? 'green'
          : r.status === 'failed'
            ? 'red'
            : r.status === 'running'
              ? 'blue'
              : 'gray',
      text: `运行 ${r.id?.slice(0, 12)} · ${r.status || '未知'}`,
      time: (r.updated_at || r.created_at || '').slice(0, 16).replace('T', ' '),
    })),
);

// ── 能力总览 ────────────────────────────────────────────────────────────────
const capabilityItems = computed(() => [
  { label: '智能体', path: '/pent/capability/agents', value: counts.value.agents },
  { label: '技能', path: '/pent/capability/skills', value: counts.value.skills },
  { label: '角色', path: '/pent/capability/roles', value: counts.value.roles },
  { label: 'MCP 工具', path: '/pent/capability/mcp-management', value: counts.value.tools },
  { label: '外部 MCP', path: '/pent/capability/mcp-management', value: counts.value.mcpServers },
  { label: '会话', path: '/pent/agent', value: sessions.value.length },
]);

function go(path: string) {
  router.push(path);
}
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <!-- 页头 -->
    <a-page-header title="仪表盘">
      <template #extra>
        <span style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-right: 8px">
          上次更新 {{ lastUpdated || '-' }}
        </span>
        <a-button :loading="loading" @click="fetchAll">刷新</a-button>
      </template>
    </a-page-header>

    <!-- 提醒横幅 -->
    <a-alert
      v-if="openFindings > 0"
      type="warning"
      show-icon
      style="margin-top: 16px"
      :message="`需要关注：${openFindings} 个漏洞待审核`"
    >
      <template #action>
        <a-button size="small" @click="go('/pent/findings')">去审核</a-button>
      </template>
    </a-alert>

    <!-- KPI 行 -->
    <a-row :gutter="[12, 12]" style="margin-top: 16px">
      <a-col :xs="12" :lg="6">
        <a-tooltip title="点击查看对话"><a-card hoverable size="small" @click="go('/pent/agent')">
          <div style="font-size: 13px; color: var(--ant-color-text-secondary)">运行中对话</div>
          <div style="font-size: 28px; font-weight: 700">{{ runningSessions }}</div>
          <div style="font-size: 12px; color: var(--ant-color-text-tertiary)">
            共 {{ sessions.length }} 个会话
          </div>
        </a-card></a-tooltip>
      </a-col>
      <a-col :xs="12" :lg="6">
        <a-tooltip title="点击查看漏洞管理"><a-card hoverable size="small" @click="go('/pent/findings')">
          <div style="font-size: 13px; color: var(--ant-color-text-secondary)">漏洞总数</div>
          <div style="font-size: 28px; font-weight: 700">{{ vulnTotal }}</div>
          <div style="font-size: 12px">
            <a-tag v-if="criticalCount" color="red" style="margin-inline-end: 4px">
              严重 {{ criticalCount }}
            </a-tag>
            <span v-else style="color: var(--ant-color-text-tertiary)">暂无新增风险</span>
          </div>
        </a-card></a-tooltip>
      </a-col>
      <a-col :xs="12" :lg="6">
        <a-tooltip title="点击查看 MCP 监控"><a-card
          hoverable
          size="small"
          @click="go('/pent/capability/mcp-monitor')"
        >
          <div style="font-size: 13px; color: var(--ant-color-text-secondary)">工具调用次数</div>
          <div style="font-size: 28px; font-weight: 700">{{ toolCalls }}</div>
          <div style="font-size: 12px; color: var(--ant-color-text-tertiary)">
            工具 {{ monitorSummary.tool_count || 0 }} 个
          </div>
        </a-card></a-tooltip>
      </a-col>
      <a-col :xs="12" :lg="6">
        <a-tooltip title="点击查看 MCP 监控"><a-card
          hoverable
          size="small"
          @click="go('/pent/capability/mcp-monitor')"
        >
          <div style="font-size: 13px; color: var(--ant-color-text-secondary)">工具执行成功率</div>
          <div
            style="font-size: 28px; font-weight: 700"
            :style="{ color: rateHealthy ? 'var(--ant-color-success)' : 'var(--ant-color-warning)' }"
          >
            {{ successRate === null ? '—' : `${successRate}%` }}
          </div>
          <div style="font-size: 12px; color: var(--ant-color-text-tertiary)">
            {{ toolCalls ? (rateHealthy ? '运行平稳' : '存在失败调用') : '暂无调用' }}
          </div>
        </a-card></a-tooltip>
      </a-col>
    </a-row>

    <!-- 漏洞严重程度分布 -->
    <a-card style="margin-top: 16px" size="small">
      <template #title>漏洞严重程度分布</template>
      <template #extra>
        <a-select
          v-model:value="sevStatusFilter"
          :options="sevStatusOptions"
          size="small"
          style="width: 130px"
          title="状态筛选"
        />
      </template>
      <a-empty v-if="!filteredTotal" description="暂无漏洞记录" />
      <div v-else style="display: flex; gap: 24px; flex-wrap: wrap">
        <div style="position: relative; width: 320px; height: 240px">
          <EchartsUI ref="donutRef" style="width: 100%; height: 100%" />
          <div
            style="
              position: absolute;
              inset: 0;
              display: flex;
              flex-direction: column;
              align-items: center;
              justify-content: center;
              pointer-events: none;
            "
          >
            <div style="font-size: 26px; font-weight: 700">{{ filteredTotal }}</div>
            <div style="font-size: 12px; color: var(--ant-color-text-tertiary)">总漏洞数</div>
          </div>
        </div>
        <div style="flex: 1; min-width: 260px">
          <div
            v-for="sev in SEVERITIES"
            :key="sev"
            style="display: flex; align-items: center; gap: 8px; padding: 6px 0"
          >
            <span
              :style="{ background: sevHex[sev] }"
              style="width: 10px; height: 10px; border-radius: 2px; display: inline-block"
            />
            <span style="width: 40px">{{ sevLabel[sev] }}</span>
            <span style="font-weight: 600">{{ sevCount(sev) }}</span>
            <span style="color: var(--ant-color-text-tertiary); font-size: 12px">
              {{ sevPct(sev) }}
            </span>
          </div>
          <div
            style="
              margin-top: 12px;
              padding-top: 12px;
              border-top: 1px solid var(--ant-color-border-secondary);
              font-size: 13px;
            "
          >
            <div style="display: flex; justify-content: space-between; margin-bottom: 6px">
              <span>修复率</span>
              <span>
                {{ fixedRate ? `${fixedRate.pct}%` : '—' }}
                <span style="color: var(--ant-color-text-tertiary); font-size: 12px">
                  ({{ fixedRate?.fixed || 0 }} / {{ filteredTotal }})
                </span>
              </span>
            </div>
            <a-progress
              :percent="fixedRate?.pct || 0"
              :show-info="false"
              stroke-color="var(--ant-color-success)"
              size="small"
            />
            <div style="margin-top: 8px; color: var(--ant-color-text-tertiary); font-size: 12px">
              最新发现：{{ latestFindingTime }}
            </div>
          </div>
        </div>
      </div>
    </a-card>

    <!-- 推荐操作 -->
    <a-card v-if="recommendations.length" title="推荐操作" size="small" style="margin-top: 16px">
      <div style="display: flex; gap: 12px; flex-wrap: wrap">
        <a-card
          v-for="r in recommendations"
          :key="r.title"
          size="small"
          hoverable
          style="min-width: 220px; flex: 1"
          @click="go(r.path)"
        >
          <div style="font-size: 13px">{{ r.title }}</div>
          <div style="margin-top: 8px; color: var(--ant-color-primary); font-size: 12px">
            {{ r.action }} →
          </div>
        </a-card>
      </div>
    </a-card>

    <!-- 最近事件 + 能力总览 -->
    <a-row :gutter="[16, 16]" style="margin-top: 16px">
      <a-col :xs="24" :lg="14">
        <a-card title="最近事件" size="small" style="height: 100%">
          <a-empty v-if="!recentEvents.length" description="暂无事件" />
          <a-timeline v-else>
            <a-timeline-item v-for="(e, i) in recentEvents" :key="i" :color="e.color">
              <span style="font-size: 13px">{{ e.text }}</span>
              <span style="margin-left: 8px; font-size: 12px; color: var(--ant-color-text-tertiary)">
                {{ e.time }}
              </span>
            </a-timeline-item>
          </a-timeline>
        </a-card>
      </a-col>
      <a-col :xs="24" :lg="10">
        <a-card title="能力总览" size="small" style="height: 100%">
          <div style="display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px">
            <div
              v-for="c in capabilityItems"
              :key="c.label"
              style="
                text-align: center;
                padding: 12px 4px;
                border: 1px solid var(--ant-color-border-secondary);
                border-radius: 8px;
                cursor: pointer;
              "
              @click="go(c.path)"
            >
              <div style="font-size: 22px; font-weight: 700">{{ c.value }}</div>
              <div style="font-size: 12px; color: var(--ant-color-text-tertiary)">{{ c.label }}</div>
            </div>
          </div>
        </a-card>
      </a-col>
    </a-row>

    <!-- 工具执行次数 -->
    <a-card title="工具执行次数" size="small" style="margin-top: 16px">
      <a-empty v-if="!topTools.length" description="暂无工具调用" />
      <EchartsUI v-else ref="barRef" style="width: 100%; height: 260px" />
    </a-card>
  </div>
</template>
