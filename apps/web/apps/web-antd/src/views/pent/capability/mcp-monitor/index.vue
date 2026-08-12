<script lang="ts" setup>
import type { EchartsUIType } from '@vben/plugins/echarts';
import type {
  PentMonitorEvidence,
  PentMonitorExecution,
  PentMonitorSummary,
  PentMonitorTimelineBucket,
  PentMonitorTopTool,
} from '#/api/pent';

import { computed, nextTick, onActivated, onDeactivated, ref } from 'vue';
import { message } from 'ant-design-vue';

import { EchartsUI, useEcharts } from '@vben/plugins/echarts';
import {
  cancelPentRun,
  deletePentMonitorExecution,
  deletePentMonitorExecutions,
  getPentMonitor,
  getPentMonitorExecution,
  getPentMonitorTimeline,
} from '#/api/pent';

// ── utils ─────────────────────────────────────────────────────────────────
/** 监控页展示：内部 mcp::tool → 模型侧 mcp__tool（与原版一致） */
function formatToolName(name?: string) {
  return name && name.includes('::') ? name.replace('::', '__') : name || '';
}

function fmtDateTime(t?: string) {
  if (!t) return '';
  const d = new Date(t);
  return Number.isNaN(d.getTime())
    ? t
    : d.toLocaleString('zh-CN', { hour12: false });
}

function fmtTimelineLabel(t: string, range: string) {
  const d = new Date(t);
  if (Number.isNaN(d.getTime())) return '';
  if (range === '24h') {
    return d.toLocaleTimeString('zh-CN', {
      hour: '2-digit',
      minute: '2-digit',
      hour12: false,
    });
  }
  if (range === '30d') {
    return d.toLocaleDateString('zh-CN', { month: 'numeric', day: 'numeric' });
  }
  return d.toLocaleString('zh-CN', {
    month: 'numeric',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  });
}

/** 耗时：n 秒 / n 分 n 秒 / n 小时 n 分（与原版 formatExecutionDuration 一致） */
function fmtDuration(start?: string, end?: string) {
  if (!start) return '未知';
  const s = new Date(start).getTime();
  const e = end ? new Date(end).getTime() : Date.now();
  if (Number.isNaN(s) || Number.isNaN(e)) return '未知';
  const seconds = Math.max(0, Math.floor((e - s) / 1000));
  if (seconds < 60) return `${seconds} 秒`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) {
    const remain = seconds % 60;
    return remain > 0 ? `${minutes} 分 ${remain} 秒` : `${minutes} 分`;
  }
  const hours = Math.floor(minutes / 60);
  const remainMinutes = minutes % 60;
  return remainMinutes > 0 ? `${hours} 小时 ${remainMinutes} 分` : `${hours} 小时`;
}

// ── state ──────────────────────────────────────────────────────────────────
const loading = ref(false);
const executions = ref<PentMonitorExecution[]>([]);
const summary = ref<PentMonitorSummary>({
  total_calls: 0,
  success_calls: 0,
  failed_calls: 0,
  last_call_time: '',
  tool_count: 0,
});
const topTools = ref<PentMonitorTopTool[]>([]);
const total = ref(0);
const page = ref(1);

const PAGE_SIZE_KEY = 'monitorPageSize';
function getSavedPageSize() {
  try {
    const size = Number.parseInt(localStorage.getItem(PAGE_SIZE_KEY) || '');
    if ([10, 20, 50, 100].includes(size)) return size;
  } catch {
    /* 忽略 */
  }
  return 20;
}
const pageSize = ref(getSavedPageSize());

const toolFilter = ref('');
const statusFilter = ref('all');
const selectedRowKeys = ref<string[]>([]);
const lastRefresh = ref('');

const statsSubtitle = computed(
  () =>
    `最后刷新 ${lastRefresh.value || '—'} · 共 ${summary.value.tool_count || topTools.value.length} 个工具`,
);

// ── KPI（总调用 / 成功率 / 最近一次调用）────────────────────────────────────
const totals = computed(() => {
  const s = summary.value;
  return {
    total: s.total_calls || 0,
    success: s.success_calls || 0,
    failed: s.failed_calls || 0,
    neutral: Math.max(0, (s.total_calls || 0) - (s.success_calls || 0) - (s.failed_calls || 0)),
  };
});

const successRateInfo = computed(() => {
  const t = totals.value;
  const effective = t.success + t.failed;
  if (effective <= 0) {
    return {
      text: '-',
      tone: 'muted',
      sub: t.total > 0 ? '暂无有效完成' : '暂无调用',
    };
  }
  const rate = (t.success / effective) * 100;
  return {
    text: `${rate.toFixed(1)}%`,
    tone: rate >= 95 ? 'success' : rate >= 80 ? 'warning' : 'danger',
    sub: rate >= 95 ? '运行平稳' : rate >= 80 ? '存在失败调用' : '失败率偏高',
  };
});

const rateColors: Record<string, string> = {
  success: '#52c41a',
  warning: '#faad14',
  danger: '#f5222d',
  muted: 'var(--ant-color-text-tertiary)',
};

const lastCallText = computed(() =>
  summary.value.last_call_time
    ? fmtDateTime(summary.value.last_call_time)
    : '暂无调用',
);

// ── timeline ───────────────────────────────────────────────────────────────
const TIMELINE_KEY = 'mcpMonitorTimelineRange';
const timelineRange = ref<'24h' | '30d' | '7d'>(
  (localStorage.getItem(TIMELINE_KEY) as any) || '7d',
);
const buckets = ref<PentMonitorTimelineBucket[]>([]);
const timelineLoading = ref(false);

const rangeLabel = computed(() =>
  timelineRange.value === '24h'
    ? '24 小时'
    : timelineRange.value === '30d'
      ? '30 天'
      : '7 天',
);

const timelineTotal = computed(() =>
  buckets.value.reduce((s, b) => s + (b.total || 0), 0),
);
const timelinePeak = computed(() =>
  buckets.value.reduce((m, b) => Math.max(m, b.total || 0), 0),
);
const hasFailedPoints = computed(() =>
  buckets.value.some((b) => (b.failed || 0) > 0),
);

/** 活跃时段：按调用量取前 4 个非空桶 */
const activeMoments = computed(() =>
  buckets.value
    .filter((b) => (b.total || 0) > 0)
    .sort((a, b) => b.total - a.total)
    .slice(0, 4)
    .map((b) => ({
      time: fmtTimelineLabel(b.ts, timelineRange.value),
      total: b.total,
      failed: b.failed || 0,
    })),
);

const trendChart = ref<EchartsUIType>();
const { renderEcharts } = useEcharts(trendChart);

async function fetchTimeline() {
  timelineLoading.value = buckets.value.length === 0;
  try {
    const res = await getPentMonitorTimeline(timelineRange.value);
    buckets.value = res.buckets || [];
    await nextTick();
    renderTrend();
  } catch {
    /* 静默 */
  } finally {
    timelineLoading.value = false;
  }
}

function renderTrend() {
  if (!buckets.value.length) return;
  const labels = buckets.value.map((b) =>
    fmtTimelineLabel(b.ts, timelineRange.value),
  );
  renderEcharts({
    backgroundColor: 'transparent',
    grid: { left: 36, right: 12, top: 16, bottom: 24 },
    tooltip: { trigger: 'axis' },
    xAxis: {
      type: 'category',
      data: labels,
      axisLabel: { fontSize: 11 },
      boundaryGap: false,
    },
    yAxis: { type: 'value', minInterval: 1, axisLabel: { fontSize: 11 } },
    series: [
      {
        name: '总调用',
        type: 'line',
        smooth: true,
        showSymbol: false,
        data: buckets.value.map((b) => b.total || 0),
        lineStyle: { color: '#3b82f6', width: 2 },
        itemStyle: { color: '#3b82f6' },
        areaStyle: { color: '#3b82f6', opacity: 0.18 },
      },
      {
        name: '失败',
        type: 'line',
        smooth: true,
        showSymbol: false,
        data: buckets.value.map((b) => b.failed || 0),
        lineStyle: { color: '#f5222d', width: 1.5 },
        itemStyle: { color: '#f5222d' },
      },
    ],
  });
}

function onRangeChange() {
  localStorage.setItem(TIMELINE_KEY, timelineRange.value);
  fetchTimeline();
}

// ── 工具统计（堆叠占比条 + Top 排行）────────────────────────────────────────
const DIST_COLORS = ['#3b82f6', '#22c55e', '#f59e0b', '#8b5cf6', '#14b8a6', '#ec4899'];

const topToolStats = computed(() =>
  topTools.value.map((t, i) => {
    const effective = (t.successes || 0) + (t.failures || 0);
    const rate = effective > 0 ? ((t.successes || 0) / effective) * 100 : 0;
    return {
      raw: t.tool_name,
      name: formatToolName(t.tool_name) || '未知工具',
      calls: t.calls || 0,
      successes: t.successes || 0,
      failures: t.failures || 0,
      rate: rate.toFixed(1),
      rateTone: rate >= 95 ? 'success' : rate >= 80 ? 'warning' : 'danger',
      share: totals.value.total > 0 ? (((t.calls || 0) / totals.value.total) * 100).toFixed(1) : '0.0',
      color: DIST_COLORS[i % DIST_COLORS.length],
    };
  }),
);

const maxToolCalls = computed(() =>
  Math.max(1, ...topToolStats.value.map((t) => t.calls)),
);

const topSharePct = computed(() => {
  const topTotal = topToolStats.value.reduce((s, t) => s + t.calls, 0);
  return totals.value.total > 0
    ? ((topTotal / totals.value.total) * 100).toFixed(1)
    : '0.0';
});

/** 占比条分段：Top 工具 + 其他工具 */
const shareSegments = computed(() => {
  const segs = topToolStats.value.map((t) => ({
    name: t.name,
    raw: t.raw,
    pct: Number.parseFloat(t.share),
    calls: t.calls,
    color: t.color,
    isOthers: false,
  }));
  const other =
    totals.value.total - topToolStats.value.reduce((s, t) => s + t.calls, 0);
  if (other > 0 && totals.value.total > 0) {
    segs.push({
      name: '其他工具',
      raw: '',
      pct: (other / totals.value.total) * 100,
      calls: other,
      color: '#cbd5e1',
      isOthers: true,
    });
  }
  return segs.filter((s) => s.pct > 0);
});

function filterByTool(toolName: string) {
  toolFilter.value = toolName === toolFilter.value ? '' : toolName;
  page.value = 1;
  fetchMonitor();
}

function clearToolFilter() {
  toolFilter.value = '';
  page.value = 1;
  fetchMonitor();
}

// ── fetch ──────────────────────────────────────────────────────────────────
async function fetchMonitor() {
  loading.value = true;
  try {
    const params: Record<string, any> = { page: page.value, page_size: pageSize.value };
    if (toolFilter.value) params.tool = toolFilter.value;
    if (statusFilter.value !== 'all') params.status = statusFilter.value;
    const res = await getPentMonitor(params);
    executions.value = res.executions || [];
    summary.value = res.summary || summary.value;
    topTools.value = res.top_tools || [];
    total.value = res.total || 0;
    lastRefresh.value = new Date().toLocaleString('zh-CN', { hour12: false });
  } catch {
    /* 静默 */
  } finally {
    loading.value = false;
  }
}

function refreshAll() {
  fetchMonitor();
  fetchTimeline();
}

// 工具搜索防抖（与原版 500ms 一致）
let toolFilterTimer: ReturnType<typeof setTimeout> | null = null;
function onToolFilterInput() {
  if (toolFilterTimer) clearTimeout(toolFilterTimer);
  toolFilterTimer = setTimeout(() => {
    page.value = 1;
    fetchMonitor();
  }, 500);
}

// ── executions table ───────────────────────────────────────────────────────
const execColumns = [
  { title: '工具', dataIndex: 'tool_name', key: 'tool_name' },
  { title: '状态', dataIndex: 'status', key: 'status', width: 100 },
  { title: '开始时间', dataIndex: 'started_at', key: 'started_at', width: 190 },
  { title: '耗时', key: 'duration', width: 120 },
  { title: '操作', key: 'actions', width: 220 },
];

const rowSelection = computed(() => ({
  selectedRowKeys: selectedRowKeys.value,
  onChange: (keys: string[]) => {
    selectedRowKeys.value = keys;
  },
}));

const statusColor: Record<string, string> = {
  pending: 'default',
  queued: 'default',
  running: 'blue',
  completed: 'green',
  failed: 'red',
  hard_timeout: 'red',
  cancelled: 'orange',
  orphaned: 'default',
};
const statusLabel: Record<string, string> = {
  pending: '等待中',
  queued: '排队中',
  running: '执行中',
  completed: '已完成',
  failed: '失败',
  hard_timeout: '执行超时',
  cancelled: '已终止',
  orphaned: '孤儿任务',
};

const hasActiveFilter = computed(
  () => statusFilter.value !== 'all' || !!toolFilter.value.trim(),
);

function onPageChange(p: number, ps: number) {
  page.value = p;
  if (ps !== pageSize.value) {
    pageSize.value = ps;
    try {
      localStorage.setItem(PAGE_SIZE_KEY, String(ps));
    } catch {
      /* 忽略 */
    }
  }
  fetchMonitor();
}

async function doCancel(exec: PentMonitorExecution) {
  if (!exec.run_id) return;
  try {
    await cancelPentRun(exec.run_id);
    message.success('已发送终止请求，工具返回后状态将更新');
    fetchMonitor();
  } catch {
    message.error('终止失败');
  }
}

async function doDelete(exec: PentMonitorExecution) {
  try {
    await deletePentMonitorExecution(exec.id);
    selectedRowKeys.value = selectedRowKeys.value.filter((k) => k !== exec.id);
    message.success('执行记录已删除');
    fetchMonitor();
  } catch {
    message.error('删除执行记录失败');
  }
}

async function doBatchDelete() {
  const count = selectedRowKeys.value.length;
  try {
    await deletePentMonitorExecutions(selectedRowKeys.value);
    message.success(`成功删除 ${count} 条执行记录`);
    selectedRowKeys.value = [];
    fetchMonitor();
  } catch {
    message.error('批量删除执行记录失败');
  }
}

// ── detail modal ───────────────────────────────────────────────────────────
const showDetail = ref(false);
const detail = ref<PentMonitorExecution | null>(null);
const evidence = ref<PentMonitorEvidence[]>([]);
const detailLoading = ref(false);
const evidenceTab = ref('raw');

const detailRequestJson = computed(() => {
  if (!detail.value) return '';
  let args: any = detail.value.params_summary || '';
  try {
    args = JSON.parse(detail.value.params_summary || '""');
  } catch {
    /* 保留原文 */
  }
  return JSON.stringify({ tool: formatToolName(detail.value.tool_name), arguments: args }, null, 2);
});

function tryFormatJson(text?: string) {
  if (!text) return '';
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}

async function openDetail(exec: PentMonitorExecution) {
  showDetail.value = true;
  detailLoading.value = true;
  detail.value = exec;
  evidence.value = [];
  evidenceTab.value = 'raw';
  try {
    const res = await getPentMonitorExecution(exec.id);
    detail.value = res.execution || exec;
    evidence.value = res.evidence || [];
  } catch {
    /* 保留已有数据 */
  } finally {
    detailLoading.value = false;
  }
}

// ── polling ────────────────────────────────────────────────────────────────
let timer: ReturnType<typeof setInterval> | null = null;

function startPolling() {
  refreshAll();
  if (timer) clearInterval(timer);
  timer = setInterval(refreshAll, 3000);
}

onActivated(startPolling);
onDeactivated(() => {
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
});
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <!-- 页头：标题 + 副标题 + 刷新 -->
    <a-page-header title="MCP 状态监控">
      <template #sub-title>{{ statsSubtitle }}</template>
      <template #extra>
        <a-button :loading="loading" @click="refreshAll">刷新</a-button>
      </template>
    </a-page-header>

    <!-- KPI 条：总调用 / 成功率 / 最近一次调用 -->
    <a-card style="margin-top: 16px" :body-style="{ padding: '16px 24px' }">
      <a-row :gutter="24">
        <a-col :span="8">
          <div style="border-left: 3px solid #3b82f6; padding-left: 16px">
            <div style="font-size: 12px; letter-spacing: 1px; color: var(--ant-color-text-tertiary)">
              总调用次数
            </div>
            <div style="font-size: 28px; font-weight: 600; line-height: 1.4">
              {{ totals.total }}
            </div>
            <div style="margin-top: 4px">
              <a-tag color="green">成功 {{ totals.success }}</a-tag>
              <a-tag color="red">失败 {{ totals.failed }}</a-tag>
              <a-tag v-if="totals.neutral > 0">终止 {{ totals.neutral }}</a-tag>
            </div>
          </div>
        </a-col>
        <a-col :span="8">
          <div style="border-left: 3px solid #52c41a; padding-left: 16px">
            <div style="font-size: 12px; letter-spacing: 1px; color: var(--ant-color-text-tertiary)">
              成功率
            </div>
            <div
              style="font-size: 28px; font-weight: 600; line-height: 1.4"
              :style="{ color: rateColors[successRateInfo.tone] }"
            >
              {{ successRateInfo.text }}
            </div>
            <div style="margin-top: 4px; font-size: 12px" :style="{ color: rateColors[successRateInfo.tone] }">
              {{ successRateInfo.sub }}
            </div>
          </div>
        </a-col>
        <a-col :span="8">
          <div style="border-left: 3px solid #8b5cf6; padding-left: 16px">
            <div style="font-size: 12px; letter-spacing: 1px; color: var(--ant-color-text-tertiary)">
              最近一次调用
            </div>
            <div style="font-size: 18px; font-weight: 600; line-height: 2">
              {{ lastCallText }}
            </div>
          </div>
        </a-col>
      </a-row>
    </a-card>

    <!-- 工具统计 · 调用趋势 合并面板 -->
    <a-card size="small" style="margin-top: 16px">
      <template #title>工具统计 · 调用趋势</template>
      <template #extra>
        <a-space>
          <a-tag v-if="toolFilter" closable color="blue" @close="clearToolFilter">
            已筛选：{{ formatToolName(toolFilter) }}
          </a-tag>
          <a-radio-group v-model:value="timelineRange" size="small" @change="onRangeChange">
            <a-radio-button value="24h">24h</a-radio-button>
            <a-radio-button value="7d">7d</a-radio-button>
            <a-radio-button value="30d">30d</a-radio-button>
          </a-radio-group>
        </a-space>
      </template>
      <div style="margin-bottom: 12px; font-size: 12px; color: var(--ant-color-text-tertiary)">
        <a-tag size="small" color="blue">累计</a-tag>
        <a-tag size="small" color="purple">{{ rangeLabel }}</a-tag>
        点击色条或列表行筛选下方执行记录；悬停联动高亮
      </div>
      <a-row :gutter="24">
        <!-- 左：占比条 + Top 工具排行 -->
        <a-col :xs="24" :lg="12">
          <template v-if="topToolStats.length">
            <div style="display: flex; height: 12px; border-radius: 6px; overflow: hidden; gap: 2px">
              <div
                v-for="seg in shareSegments"
                :key="seg.name"
                :title="`${seg.name} · ${seg.pct.toFixed(1)}% · ${seg.calls}`"
                :style="{
                  flex: `${seg.pct} 1 0`,
                  background: seg.color,
                  cursor: seg.isOthers ? 'default' : 'pointer',
                  opacity: toolFilter && !seg.isOthers && seg.raw !== toolFilter ? 0.35 : 1,
                }"
                @click="!seg.isOthers && filterByTool(seg.raw)"
              />
            </div>
            <div style="margin: 8px 0 12px; font-size: 12px; color: var(--ant-color-text-tertiary)">
              <a-tag size="small" color="blue">累计</a-tag>
              Top {{ topToolStats.length }} 占 {{ topSharePct }}% · 共 {{ totals.total }} 次调用
            </div>
            <div
              v-for="(t, i) in topToolStats"
              :key="t.raw"
              :style="{
                border: '1px solid var(--ant-color-border-secondary)',
                borderRadius: '8px',
                padding: '12px 16px',
                marginBottom: '12px',
                cursor: 'pointer',
                background: toolFilter === t.raw ? 'var(--ant-color-fill-quaternary)' : 'transparent',
              }"
              :title="`${t.name}，${t.calls} 次调用，成功率 ${t.rate}%，点击查看执行记录`"
              @click="filterByTool(t.raw)"
            >
              <div style="display: flex; align-items: center; gap: 8px">
                <a-avatar :size="22" :style="{ background: i < 3 ? t.color : 'var(--ant-color-text-quaternary)', fontSize: '12px' }">
                  {{ i + 1 }}
                </a-avatar>
                <span
                  :style="{ display: 'inline-block', width: '8px', height: '8px', borderRadius: '50%', background: t.color }"
                />
                <span style="font-weight: 600; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap">
                  {{ t.name }}
                </span>
                <a-tag size="small">{{ t.share }}%</a-tag>
              </div>
              <div style="display: flex; align-items: center; gap: 8px; margin-top: 8px">
                <span style="font-size: 22px; font-weight: 700">{{ t.calls }}</span>
                <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">调用</span>
                <span style="flex: 1; height: 6px; border-radius: 3px; background: var(--ant-color-fill-secondary)">
                  <span
                    :style="{
                      display: 'block',
                      height: '100%',
                      borderRadius: '3px',
                      background: t.color,
                      width: `${((t.calls / maxToolCalls) * 100).toFixed(1)}%`,
                    }"
                  />
                </span>
              </div>
              <div style="display: flex; align-items: center; gap: 8px; margin-top: 8px">
                <a-tag color="green" size="small">成功 {{ t.successes }}</a-tag>
                <a-tag :color="t.failures > 0 ? 'red' : 'default'" size="small">
                  失败 {{ t.failures }}
                </a-tag>
                <span style="flex: 1" />
                <span :style="{ color: rateColors[t.rateTone], fontWeight: 600 }">{{ t.rate }}%</span>
                <span v-if="t.failures > 0" style="font-size: 12px; color: #faad14">
                  失败 {{ t.failures }}
                </span>
              </div>
            </div>
          </template>
          <a-empty v-else-if="!loading" description="暂无统计数据" />
        </a-col>
        <!-- 右：调用趋势 -->
        <a-col :xs="24" :lg="12">
          <div style="font-size: 12px; letter-spacing: 1px; color: var(--ant-color-text-tertiary)">
            调用趋势
          </div>
          <template v-if="buckets.length && timelineTotal > 0">
            <div style="margin-top: 4px; font-size: 12px; color: var(--ant-color-text-tertiary)">
              全部工具合计，不按工具拆分 · 区间内 {{ timelineTotal }} 次 · 峰值 {{ timelinePeak }}
            </div>
            <div v-if="activeMoments.length" style="display: flex; align-items: center; flex-wrap: wrap; gap: 6px; margin-top: 8px">
              <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">活跃时段</span>
              <a-tag v-for="m in activeMoments" :key="m.time" size="small" :title="m.time">
                {{ m.time }}
                <a-badge :count="m.total" size="small" color="#3b82f6" />
                <span v-if="m.failed > 0" style="color: #f5222d">失败 {{ m.failed }}</span>
              </a-tag>
            </div>
            <EchartsUI ref="trendChart" height="220px" />
            <div style="display: flex; gap: 16px; font-size: 12px; color: var(--ant-color-text-secondary)">
              <span>
                <span :style="{ display: 'inline-block', width: '12px', height: '3px', background: '#3b82f6', verticalAlign: 'middle', marginRight: '4px' }" />
                总调用
              </span>
              <span v-if="hasFailedPoints">
                <span :style="{ display: 'inline-block', width: '12px', height: '3px', background: '#f5222d', verticalAlign: 'middle', marginRight: '4px' }" />
                失败
              </span>
            </div>
          </template>
          <a-empty
            v-else-if="!timelineLoading"
            description="该时段暂无调用"
            style="margin-top: 48px"
          >
            <template #description>
              <div>该时段暂无调用</div>
              <div style="font-size: 12px; color: var(--ant-color-text-tertiary)">
                切换时间范围查看其他时段，或在对话/任务中调用 MCP 工具
              </div>
            </template>
          </a-empty>
          <a-spin v-else style="display: block; text-align: center; padding: 60px 0" tip="趋势加载中…" />
        </a-col>
      </a-row>
    </a-card>

    <!-- 最新执行记录 -->
    <a-card title="最新执行记录" size="small" style="margin-top: 16px">
      <template #extra>
        <a-space>
          <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">工具搜索</span>
          <a-input
            v-model:value="toolFilter"
            placeholder="输入工具名称..."
            style="width: 200px"
            allow-clear
            @input="onToolFilterInput"
            @keydown.enter="page = 1; fetchMonitor()"
          />
          <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">状态筛选</span>
          <a-select
            v-model:value="statusFilter"
            style="width: 120px"
            :options="[
              { label: '全部', value: 'all' },
              { label: '已完成', value: 'completed' },
              { label: '执行中', value: 'running' },
              { label: '失败', value: 'failed' },
              { label: '已终止', value: 'cancelled' },
            ]"
            @change="page = 1; fetchMonitor()"
          />
        </a-space>
      </template>

      <!-- 批量操作条 -->
      <div
        v-if="selectedRowKeys.length"
        style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; padding: 8px 12px; border: 1px solid var(--ant-color-border-secondary); border-radius: 6px"
      >
        <span>已选择 {{ selectedRowKeys.length }} 项</span>
        <a-space>
          <a-button
            size="small"
            @click="selectedRowKeys = executions.map((e: PentMonitorExecution) => e.id)"
          >
            全选
          </a-button>
          <a-button size="small" @click="selectedRowKeys = []">取消全选</a-button>
          <a-popconfirm
            :title="`确定要删除选中的 ${selectedRowKeys.length} 条执行记录吗？此操作不可恢复。`"
            @confirm="doBatchDelete"
          >
            <a-button danger size="small">批量删除</a-button>
          </a-popconfirm>
        </a-space>
      </div>

      <a-table
        :columns="execColumns"
        :data-source="executions"
        :loading="loading"
        row-key="id"
        size="middle"
        :row-selection="rowSelection"
        :pagination="{
          current: page,
          pageSize,
          total,
          pageSizeOptions: ['10', '20', '50', '100'],
          showSizeChanger: true,
          showTotal: (t: number) => {
            const start = t === 0 ? 0 : (page - 1) * pageSize + 1;
            const end = Math.min(page * pageSize, t);
            return `显示 ${start}-${end} / 共 ${t} 条记录`;
          },
          onChange: onPageChange,
        }"
      >
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'tool_name'">
            {{ formatToolName(record.tool_name) || '未知工具' }}
          </template>
          <template v-else-if="column.key === 'status'">
            <a-tag :color="statusColor[record.status] || 'default'">
              {{ statusLabel[record.status] || record.status }}
            </a-tag>
          </template>
          <template v-else-if="column.key === 'started_at'">
            {{ fmtDateTime(record.started_at) || '未知' }}
          </template>
          <template v-else-if="column.key === 'duration'">
            {{ fmtDuration(record.started_at, record.completed_at) }}
          </template>
          <template v-else-if="column.key === 'actions'">
            <a-button size="small" type="link" @click="openDetail(record)">查看详情</a-button>
            <a-popconfirm
              v-if="record.status === 'running'"
              title="确定终止此次工具调用？整条对话或迭代任务不会因此停止。"
              @confirm="doCancel(record)"
            >
              <a-button size="small" type="link">终止</a-button>
            </a-popconfirm>
            <a-popconfirm title="确定要删除此执行记录吗？此操作不可恢复。" @confirm="doDelete(record)">
              <a-button size="small" type="link" danger>删除</a-button>
            </a-popconfirm>
          </template>
        </template>
      </a-table>
      <a-empty v-if="!executions.length && !loading">
        <template #description>
          <div>{{ hasActiveFilter ? '当前筛选条件下暂无记录' : '暂无执行记录' }}</div>
          <div v-if="!hasActiveFilter" style="font-size: 12px; color: var(--ant-color-text-tertiary)">
            在对话或任务中调用 MCP 工具后，执行记录将显示在此处
          </div>
        </template>
      </a-empty>
    </a-card>

    <!-- 工具调用详情弹窗 -->
    <a-modal v-model:open="showDetail" title="工具调用详情" width="760px" :footer="null">
      <a-spin :spinning="detailLoading">
        <template v-if="detail">
          <a-descriptions title="执行信息" bordered size="small" :column="2">
            <a-descriptions-item label="工具">
              {{ formatToolName(detail.tool_name) || '未知' }}
            </a-descriptions-item>
            <a-descriptions-item label="执行 ID">{{ detail.id || '—' }}</a-descriptions-item>
            <a-descriptions-item label="状态">
              <a-tag :color="statusColor[detail.status] || 'default'">
                {{ statusLabel[detail.status] || detail.status }}
              </a-tag>
            </a-descriptions-item>
            <a-descriptions-item label="时间">
              {{ fmtDateTime(detail.started_at) || '—' }}
            </a-descriptions-item>
            <a-descriptions-item label="耗时">
              {{ fmtDuration(detail.started_at, detail.completed_at) }}
            </a-descriptions-item>
            <a-descriptions-item label="执行后端">{{ detail.executor_backend || '—' }}</a-descriptions-item>
          </a-descriptions>

          <div style="margin-top: 16px">
            <strong>请求参数</strong>
            <pre style="background: var(--ant-color-bg-layout); padding: 12px; border-radius: 4px; max-height: 200px; overflow: auto; margin-top: 4px; font-size: 12px; white-space: pre-wrap">{{ detailRequestJson || '—' }}</pre>
          </div>

          <div style="margin-top: 16px">
            <strong>响应结果</strong>
            <template v-if="evidence.length">
              <div
                v-for="(ev, i) in evidence"
                :key="ev.id || i"
                style="margin-top: 8px; border: 1px solid var(--ant-color-border-secondary); border-radius: 4px; padding: 8px"
              >
                <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-bottom: 4px">
                  {{ ev.content_type }}
                </div>
                <a-tabs v-model:activeKey="evidenceTab" size="small">
                  <a-tab-pane key="raw" tab="原始响应">
                    <pre style="background: var(--ant-color-bg-layout); padding: 12px; border-radius: 4px; max-height: 260px; overflow: auto; font-size: 12px; white-space: pre-wrap">{{ ev.source }}</pre>
                  </a-tab-pane>
                  <a-tab-pane key="formatted" tab="正确信息">
                    <pre style="background: var(--ant-color-bg-layout); padding: 12px; border-radius: 4px; max-height: 260px; overflow: auto; font-size: 12px; white-space: pre-wrap">{{ tryFormatJson(ev.source) }}</pre>
                  </a-tab-pane>
                </a-tabs>
              </div>
            </template>
            <div v-else style="margin-top: 8px; font-size: 13px; color: var(--ant-color-text-tertiary)">
              {{
                detail.status === 'running'
                  ? '尚无返回，工具可能仍在执行。若长时间无响应，可使用下方「终止工具」结束本次调用。'
                  : '暂无响应数据'
              }}
            </div>
          </div>

          <div v-if="detail.status === 'running' && detail.run_id" style="margin-top: 16px">
            <strong>运行控制</strong>
            <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin: 4px 0 8px">
              仅中断当前这一次工具调用；对话与多步迭代任务会继续，不会等同于「停止任务」。
            </div>
            <a-popconfirm
              title="确定终止此次工具调用？整条对话或迭代任务不会因此停止。"
              @confirm="doCancel(detail)"
            >
              <a-button danger size="small">终止工具</a-button>
            </a-popconfirm>
          </div>
        </template>
      </a-spin>
    </a-modal>
  </div>
</template>
