<script lang="ts" setup>
import type { EchartsUIType } from '@vben/plugins/echarts';
import { nextTick, onMounted, reactive, ref, watch } from 'vue';
import { EchartsUI, useEcharts } from '@vben/plugins/echarts';
import { message } from 'ant-design-vue';
import { getPentProjects, getPentRunEvents, getPentRunToolCalls, getPentRuns } from '#/api/pent';
import { getRmosAttackGraph, getRmosMemories, submitRmosFeedback } from '#/api/pent/rmos';
import PageHeader from '../components/PageHeader.vue';

const projects = ref<any[]>([]);
const runs = ref<any[]>([]);
const activeTab = ref('memories');

// ── Memories ──────────────────────────────────────────────────────────
const memories = ref<any[]>([]);
const memoryLoading = ref(false);
const memoryFilters = reactive<{ project_id?: string; run_id?: string; q?: string }>({});
const memoryPagination = reactive({ current: 1, pageSize: 20, total: 0, showTotal: (t: number) => `共 ${t} 条` });
const feedbackLoading = ref<string | null>(null);

async function loadProjects() {
  try {
    projects.value = (await getPentProjects()) as any[];
  } catch {
    projects.value = [];
  }
}
async function loadRuns() {
  try {
    runs.value = (await getPentRuns({ limit: 200 })) as any[];
  } catch {
    runs.value = [];
  }
}

async function fetchMemories() {
  memoryLoading.value = true;
  try {
    const params: Record<string, any> = { limit: 200 };
    if (memoryFilters.project_id) params.project_id = memoryFilters.project_id;
    if (memoryFilters.run_id) params.run_id = memoryFilters.run_id;
    if (memoryFilters.q) params.q = memoryFilters.q;
    memories.value = (await getRmosMemories(params)) as any[];
    memoryPagination.total = memories.value.length;
  } catch (error: any) {
    message.error(error?.message || '加载记忆失败');
  } finally {
    memoryLoading.value = false;
  }
}

async function submitFeedback(memory: any, feedback: 'useful' | 'useless') {
  const memoryId = memory.id;
  const previous = memory.feedback;
  memory.feedback = feedback;
  feedbackLoading.value = memoryId;
  try {
    await submitRmosFeedback(memoryId, { feedback });
  } catch (error: any) {
    memory.feedback = previous;
    message.error(error?.message || '反馈提交失败');
  } finally {
    feedbackLoading.value = null;
  }
}

// ── Attack graph ──────────────────────────────────────────────────────
const graphLoading = ref(false);
const graphProjectId = ref<string>();
const graphData = ref<any>({ nodes: [], edges: [], summary: {} });
const graphChart = ref<EchartsUIType>();
const { renderEcharts } = useEcharts(graphChart);

async function buildGraph() {
  graphLoading.value = true;
  try {
    const params: Record<string, any> = {};
    if (graphProjectId.value) params.project_id = graphProjectId.value;
    graphData.value = (await getRmosAttackGraph(params)) || { nodes: [], edges: [], summary: {} };
    if (graphData.value.nodes?.length) {
      await nextTick();
      renderEcharts({
        backgroundColor: 'transparent',
        tooltip: {
          formatter: (p: any) =>
            p.dataType === 'node'
              ? `<b>${p.name}</b><br/>${nodeLabel(p.data)}`
              : `${p.data.source} → ${p.data.target}`,
        },
        series: [
          {
            type: 'graph',
            layout: 'force',
            roam: true,
            draggable: true,
            label: {
              show: true,
              fontSize: 11,
              color: 'var(--ant-color-text)',
              formatter: (p: any) => (p.name?.length > 16 ? `${p.name.slice(0, 14)}..` : p.name),
            },
            force: { repulsion: 600, gravity: 0.1, edgeLength: [80, 180], layoutAnimation: true },
            categories: [
              { name: '严重', itemStyle: { color: 'var(--ant-color-error)' } },
              { name: '高危', itemStyle: { color: 'var(--ant-color-warning)' } },
              { name: '中低危', itemStyle: { color: 'var(--ant-color-primary)' } },
              { name: '目标', itemStyle: { color: '#3cb371' } },
            ],
            data: graphData.value.nodes,
            edges: (graphData.value.edges || []).map((e: any) => ({
              ...e,
              lineStyle: { color: '#bbb', curveness: 0.2, width: 0.6, opacity: 0.7 },
            })),
          },
        ],
      });
    }
  } catch (error: any) {
    message.error(error?.message || '加载攻击图谱失败');
  } finally {
    graphLoading.value = false;
  }
}

function nodeLabel(node: any) {
  if (node.kind === 'asset') return '目标资产';
  if (node.severity === 'critical') return '严重漏洞';
  if (node.severity === 'high') return '高危漏洞';
  return '中低危漏洞';
}

// ── Step replay ───────────────────────────────────────────────────────
const replayRunId = ref<string>();
const replayEvents = ref<any[]>([]);
const replayToolCalls = ref<any[]>([]);
const replayLoading = ref(false);

async function loadReplay() {
  if (!replayRunId.value) {
    replayEvents.value = [];
    replayToolCalls.value = [];
    return;
  }
  replayLoading.value = true;
  try {
    [replayEvents.value, replayToolCalls.value] = (await Promise.all([
      getPentRunEvents(replayRunId.value),
      getPentRunToolCalls(replayRunId.value),
    ])) as [any[], any[]];
  } catch (error: any) {
    message.error(error?.message || '加载回放失败');
  } finally {
    replayLoading.value = false;
  }
}

function fmt(value?: string) {
  return value ? new Date(value).toLocaleString() : '-';
}
function eventTitle(event: any) {
  return event.event_type?.replaceAll('.', ' / ') || '事件';
}
function replayTone(type = '') {
  if (type.includes('failed') || type.includes('error')) return 'red';
  if (type.includes('completed') || type.includes('result')) return 'green';
  if (type.includes('tool')) return 'blue';
  return 'gray';
}

const memoryColumns = [
  { title: '摘要', dataIndex: 'title', key: 'title', ellipsis: true },
  { title: '类型', dataIndex: 'memory_type', key: 'type', width: 110 },
  { title: '工具', dataIndex: 'tool_name', key: 'tool', width: 130, ellipsis: true },
  { title: '来源 Run', dataIndex: 'run_id', key: 'run', width: 140, ellipsis: true },
  { title: '时间', dataIndex: 'created_at', key: 'time', width: 170 },
  { title: '反馈', key: 'feedback', width: 150 },
];

watch(activeTab, (tab) => {
  if (tab === 'graph') buildGraph();
});

onMounted(async () => {
  await Promise.all([loadProjects(), loadRuns()]);
  await fetchMemories();
});
</script>

<template>
  <div style="padding: 24px; min-height: 100%; background-color: var(--ant-color-bg-layout)">
    <PageHeader title="RMOS 记忆" subtitle="记忆检索、反馈与攻击路径回放" />

    <a-tabs v-model:activeKey="activeTab" size="small">
      <a-tab-pane key="memories" tab="记忆">
        <a-card size="small" style="border-radius: 8px; margin-bottom: 12px">
          <a-space wrap>
            <a-select
              v-model:value="memoryFilters.project_id"
              allow-clear
              show-search
              option-filter-prop="label"
              placeholder="全部项目"
              style="width: 180px"
              :options="projects.map((p: any) => ({ label: p.name, value: p.id }))"
            />
            <a-select
              v-model:value="memoryFilters.run_id"
              allow-clear
              show-search
              option-filter-prop="label"
              placeholder="全部 Run"
              style="width: 200px"
              :options="runs.map((r: any) => ({ label: r.id?.slice(0, 16), value: r.id }))"
            />
            <a-input
              v-model:value="memoryFilters.q"
              allow-clear
              placeholder="搜索摘要 / 内容"
              style="width: 200px"
              @press-enter="fetchMemories"
            />
            <a-button type="primary" @click="fetchMemories">查询</a-button>
            <a-button
              @click="
                memoryFilters.project_id = undefined;
                memoryFilters.run_id = undefined;
                memoryFilters.q = undefined;
                fetchMemories();
              "
            >
              重置
            </a-button>
          </a-space>
        </a-card>

        <a-card size="small" style="border-radius: 8px">
          <a-table
            :columns="memoryColumns"
            :data-source="memories"
            row-key="id"
            size="middle"
            :loading="memoryLoading"
            :pagination="memoryPagination"
            :scroll="{ x: 900 }"
          >
            <template #bodyCell="{ column, record }">
              <template v-if="column.key === 'title'">
                <a-tooltip :title="record.summary">
                  <span>{{ record.title || record.summary?.slice(0, 60) || '(无标题)' }}</span>
                </a-tooltip>
              </template>
              <template v-if="column.key === 'type'">
                <a-tag :color="record.memory_type === 'tool_result' ? 'green' : 'blue'">
                  {{ record.memory_type || 'memory' }}
                </a-tag>
              </template>
              <template v-if="column.key === 'run'">
                <span style="font-family: monospace; font-size: 12px">{{ record.run_id?.slice(0, 16) || '-' }}</span>
              </template>
              <template v-if="column.key === 'time'">{{ fmt(record.created_at) }}</template>
              <template v-if="column.key === 'feedback'">
                <a-space>
                  <a-button
                    size="small"
                    :type="record.feedback === 'useful' ? 'primary' : 'default'"
                    :loading="feedbackLoading === record.id"
                    @click="submitFeedback(record, 'useful')"
                  >
                    👍 有用
                  </a-button>
                  <a-button
                    size="small"
                    :type="record.feedback === 'useless' ? 'primary' : 'default'"
                    :loading="feedbackLoading === record.id"
                    @click="submitFeedback(record, 'useless')"
                  >
                    👎 无用
                  </a-button>
                </a-space>
              </template>
            </template>
          </a-table>
          <a-empty v-if="!memories.length && !memoryLoading" description="暂无记忆 · Worker 执行后自动积累" />
        </a-card>
      </a-tab-pane>

      <a-tab-pane key="graph" tab="攻击图谱">
        <a-space style="margin-bottom: 12px" wrap>
          <a-select
            v-model:value="graphProjectId"
            allow-clear
            show-search
            option-filter-prop="label"
            placeholder="全部项目"
            style="width: 200px"
            :options="projects.map((p: any) => ({ label: p.name, value: p.id }))"
          />
          <a-button type="primary" :loading="graphLoading" @click="buildGraph">生成图谱</a-button>
        </a-space>

        <a-row :gutter="[12, 12]" style="margin-bottom: 12px">
          <a-col v-for="c in [
            { label: '目标资产', key: 'assets', color: '#3cb371' },
            { label: '漏洞总数', key: 'vulnerabilities', color: 'var(--ant-color-primary)' },
            { label: '严重', key: 'critical', color: 'var(--ant-color-error)' },
            { label: '高危', key: 'high', color: 'var(--ant-color-warning)' },
          ]" :key="c.key" :xs="12" :md="6">
            <div
              :style="{
                padding: '16px',
                borderRadius: '8px',
                textAlign: 'center',
                border: `1px solid ${c.color}40`,
                background: `${c.color}10`,
              }"
            >
              <div :style="{ fontSize: '28px', fontWeight: 700, color: c.color }">
                {{ graphData.summary?.[c.key] ?? 0 }}
              </div>
              <div style="font-size: 13px; color: var(--ant-color-text-tertiary)">{{ c.label }}</div>
            </div>
          </a-col>
        </a-row>

        <a-card size="small" :loading="graphLoading" style="border-radius: 8px">
          <a-empty v-if="!graphData.nodes?.length" description="暂无图谱数据 · 执行扫描后自动生成" />
          <EchartsUI v-else ref="graphChart" height="60vh" />
        </a-card>
      </a-tab-pane>

      <a-tab-pane key="replay" tab="步骤回放">
        <a-space style="margin-bottom: 12px" wrap>
          <a-select
            v-model:value="replayRunId"
            show-search
            option-filter-prop="label"
            placeholder="选择一个 Run 回放步骤"
            style="width: 280px"
            :options="runs.map((r: any) => ({ label: `${r.id?.slice(0, 16)} (${r.status})`, value: r.id }))"
            @change="loadReplay"
          />
        </a-space>

        <a-spin :spinning="replayLoading">
          <a-empty
            v-if="!replayRunId"
            description="选择 Run 后回放执行步骤"
            style="padding: 60px 0"
          />
          <a-empty
            v-else-if="!replayEvents.length && !replayToolCalls.length"
            description="该 Run 暂无执行步骤"
          />
          <a-timeline v-else>
            <a-timeline-item
              v-for="evt in replayEvents"
              :key="evt.event_id"
              :color="replayTone(evt.event_type)"
            >
              <div style="font-weight: 600">{{ eventTitle(evt) }}</div>
              <div style="color: var(--ant-color-text-secondary); font-size: 12px; word-break: break-all">
                {{ JSON.stringify(evt.payload ?? {}).slice(0, 200) }}
              </div>
              <div style="color: var(--ant-color-text-tertiary); font-size: 11px">{{ fmt(evt.occurred_at) }}</div>
            </a-timeline-item>
            <a-timeline-item
              v-for="call in replayToolCalls"
              :key="call.id"
              :color="call.exit_code === 0 ? 'green' : 'red'"
            >
              <div style="font-weight: 600">
                🔧 {{ call.tool_name }}
                <a-tag :color="call.exit_code === 0 ? 'success' : 'error'">
                  {{ call.exit_code === 0 ? '成功' : `退出码 ${call.exit_code ?? '-'}` }}
                </a-tag>
              </div>
              <div style="color: var(--ant-color-text-secondary); font-size: 12px">
                {{ JSON.stringify(call.params_summary ?? {}).slice(0, 200) }}
              </div>
            </a-timeline-item>
          </a-timeline>
        </a-spin>
      </a-tab-pane>
    </a-tabs>
  </div>
</template>
