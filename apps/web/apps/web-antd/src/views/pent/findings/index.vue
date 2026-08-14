<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue';
import { message } from 'ant-design-vue';

import { getPentProjectFindings, getPentProjects, getPentRunEvidence } from '#/api/pent';
import { requestClient } from '#/api/request';

// ── constants ────────────────────────────────────────────────────────────────
const SEVERITIES = ['critical', 'high', 'medium', 'low', 'info'] as const;

const sevLabel: Record<string, string> = {
  critical: '严重',
  high: '高危',
  medium: '中危',
  low: '低危',
  info: '信息',
};
const sevColor: Record<string, string> = {
  critical: 'red',
  high: 'orange',
  medium: 'gold',
  low: 'green',
  info: 'default',
};
// 堆叠条 / 卡片左侧色条用的语义色（对齐原版）
const sevHex: Record<string, string> = {
  critical: '#dc2626',
  high: '#ea580c',
  medium: '#ca8a04',
  low: '#16a34a',
  info: '#6b7280',
};

const statusLabel: Record<string, string> = {
  open: '待处理',
  confirmed: '已确认',
  fixed: '已修复',
  ignored: '已忽略',
  false_positive: '误报',
};
const statusColor: Record<string, string> = {
  open: 'blue',
  confirmed: 'green',
  fixed: 'cyan',
  ignored: 'default',
  false_positive: 'default',
};
const statusOptions = [
  { label: '全部状态', value: 'all' },
  { label: '待处理', value: 'open' },
  { label: '已确认', value: 'confirmed' },
  { label: '已修复', value: 'fixed' },
  { label: '已忽略', value: 'ignored' },
  { label: '误报', value: 'false_positive' },
];

// ── state ──────────────────────────────────────────────────────────────────
const projects = ref<any[]>([]);
const findings = ref<any[]>([]);
const loading = ref(false);

const searchText = ref('');
const projectFilter = ref(''); // '' = 全部项目
const statusFilter = ref('all');
const sevFilter = ref(''); // '' = 全部
const exactIdFilter = ref('');

// ── data loading ───────────────────────────────────────────────────────────
async function loadProjects() {
  try {
    projects.value = ((await getPentProjects()) as any[]) || [];
  } catch {
    projects.value = [];
  }
}

async function fetchFindings() {
  loading.value = true;
  expandedId.value = null;
  evidenceCache.value = {};
  try {
    if (projectFilter.value) {
      const resp: any = await getPentProjectFindings(projectFilter.value);
      findings.value = resp?.items ?? [];
    } else {
      // 全部项目：逐项目拉取后合并
      const all = await Promise.all(
        projects.value.map(async (p) => {
          try {
            const resp: any = await getPentProjectFindings(p.id);
            return (resp?.items ?? []) as any[];
          } catch {
            return [];
          }
        }),
      );
      findings.value = all.flat();
    }
    findings.value.sort((a, b) => (b.created_at || '').localeCompare(a.created_at || ''));
  } catch {
    findings.value = [];
  } finally {
    loading.value = false;
  }
}

function refresh() {
  fetchFindings();
}

onMounted(async () => {
  await loadProjects();
  await fetchFindings();
});

const projectName = computed(() => {
  const m: Record<string, string> = {};
  for (const p of projects.value) m[p.id] = p.name || p.id;
  return m;
});

// ── stats ──────────────────────────────────────────────────────────────────
const total = computed(() => findings.value.length);

function sevCount(sev: string) {
  return findings.value.filter((f) => (f.severity || 'info') === sev).length;
}
function sevPct(sev: string) {
  if (!total.value) return '—';
  return `${Math.round((sevCount(sev) / total.value) * 100)}%`;
}
const stackedSegments = computed(() =>
  SEVERITIES.map((sev) => ({
    sev,
    width: total.value ? `${(sevCount(sev) / total.value) * 100}%` : '0%',
  })).filter((s) => s.width !== '0%'),
);

function toggleSev(sev: string) {
  sevFilter.value = sevFilter.value === sev ? '' : sev;
  page.value = 1;
}

// ── filters ────────────────────────────────────────────────────────────────
const filtered = computed(() => {
  let list = findings.value;
  if (sevFilter.value) {
    list = list.filter((f) => (f.severity || 'info') === sevFilter.value);
  }
  if (statusFilter.value !== 'all') {
    list = list.filter((f) => f.status === statusFilter.value);
  }
  if (exactIdFilter.value.trim()) {
    const id = exactIdFilter.value.trim();
    list = list.filter((f) => f.id === id);
  }
  if (searchText.value) {
    const q = searchText.value.toLowerCase();
    list = list.filter(
      (f) =>
        (f.title || '').toLowerCase().includes(q) ||
        (f.description || '').toLowerCase().includes(q) ||
        (f.vuln_type || '').toLowerCase().includes(q) ||
        (f.target || '').toLowerCase().includes(q),
    );
  }
  return list;
});

const hasActiveFilters = computed(
  () =>
    !!(
      searchText.value ||
      projectFilter.value ||
      statusFilter.value !== 'all' ||
      sevFilter.value ||
      exactIdFilter.value.trim()
    ),
);

function clearAllFilters() {
  searchText.value = '';
  projectFilter.value = '';
  statusFilter.value = 'all';
  sevFilter.value = '';
  exactIdFilter.value = '';
  page.value = 1;
  fetchFindings();
}

// ── pagination（前端切片，对齐原版分页栏）─────────────────────────────────────
const page = ref(1);
const pageSize = ref(Number(localStorage.getItem('pentVulnPageSize')) || 20);
const totalPages = computed(() => Math.max(1, Math.ceil(filtered.value.length / pageSize.value)));
const paged = computed(() => {
  const start = (page.value - 1) * pageSize.value;
  return filtered.value.slice(start, start + pageSize.value);
});
const rangeStart = computed(() =>
  filtered.value.length === 0 ? 0 : (page.value - 1) * pageSize.value + 1,
);
const rangeEnd = computed(() =>
  Math.min(page.value * pageSize.value, filtered.value.length),
);

function gotoPage(p: number) {
  page.value = Math.min(Math.max(1, p), totalPages.value);
}
function changePageSize(v: number) {
  localStorage.setItem('pentVulnPageSize', String(v));
  const currentStart = (page.value - 1) * pageSize.value + 1;
  pageSize.value = v;
  page.value = Math.max(1, Math.floor((currentStart - 1) / v) + 1);
}

// ── expand & evidence ──────────────────────────────────────────────────────
const expandedId = ref<null | string>(null);
const evidenceCache = ref<Record<string, any[]>>({});
const loadingEvidence = ref<null | string>(null);

async function toggleExpand(f: any) {
  if (expandedId.value === f.id) {
    expandedId.value = null;
    return;
  }
  expandedId.value = f.id;
  await loadEvidence(f);
}

async function loadEvidence(f: any) {
  if (evidenceCache.value[f.id]) return;
  loadingEvidence.value = f.id;
  try {
    const ev = ((await getPentRunEvidence(f.run_id)) as any[]) || [];
    const linked = ev.filter((e: any) =>
      f.evidence_ids?.some((eid: any) => {
        const id = typeof eid === 'string' ? eid : eid?.id;
        return id && (e.id === id || e.id?.includes(id?.slice(0, 12)));
      }),
    );
    evidenceCache.value[f.id] = linked.length ? linked : ev.slice(0, 3);
  } catch {
    evidenceCache.value[f.id] = [];
  } finally {
    loadingEvidence.value = null;
  }
}

// ── review（审核，仅支持 确认/误报）───────────────────────────────────────────
const reviewing = ref<null | string>(null);

async function doReview(f: any, decision: string) {
  if (reviewing.value) return;
  reviewing.value = f.id;
  try {
    const updated: any = await requestClient.post(`/pent/findings/${f.id}`, { decision });
    f.status = updated?.status || decision;
    message.success(decision === 'confirmed' ? '已确认为漏洞' : '已标记为误报');
  } catch {
    message.error('状态更新失败（可能需要先关联证据）');
  } finally {
    reviewing.value = null;
  }
}

// ── markdown 下载（前端生成，无需后端接口）────────────────────────────────────
function downloadMarkdown(f: any) {
  const lines = [
    `# ${f.title || '未命名漏洞'}`,
    '',
    `- 漏洞 ID：${f.id}`,
    `- 严重度：${sevLabel[f.severity] || f.severity || 'info'}`,
    `- 状态：${statusLabel[f.status] || f.status || '—'}`,
    `- 项目：${projectName.value[f.project_id] || '未绑定项目'}`,
    `- 目标：${f.target || '—'}`,
    `- 类型：${f.vuln_type || f.category || '—'}`,
    `- 创建时间：${fmtTime(f.created_at)}`,
    '',
    '## 描述',
    '',
    f.description || '（无描述）',
    '',
  ];
  const blob = new Blob([lines.join('\n')], { type: 'text/markdown;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `vulnerability-${f.id}.md`;
  a.click();
  URL.revokeObjectURL(url);
}

function fmtTime(t?: string) {
  return t ? t.slice(0, 16).replace('T', ' ') : '—';
}
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <!-- 页头：标题 + 操作按钮（对齐原版顺序） -->
    <a-page-header title="漏洞管理">
      <template #extra>
        <a-tooltip title="暂未接入（后端暂不支持批量导出）">
          <a-button disabled>批量导出</a-button>
        </a-tooltip>
        <a-tooltip title="暂未接入（后端暂不支持删除）">
          <a-button disabled danger>批量删除</a-button>
        </a-tooltip>
        <a-button :loading="loading" @click="refresh">刷新</a-button>
        <a-tooltip title="暂未接入（后端暂不支持手工添加）">
          <a-button disabled type="primary">添加漏洞</a-button>
        </a-tooltip>
      </template>
    </a-page-header>

    <!-- 统计看板：点击卡片筛选严重度 -->
    <a-row :gutter="[12, 12]" style="margin-top: 16px">
      <a-col :xs="12" :sm="8" :lg="4">
        <a-tooltip title="查看全部（清除严重度筛选）">
          <a-card
            hoverable
            size="small"
            :style="!sevFilter ? { borderColor: 'var(--ant-color-primary)' } : {}"
            @click="sevFilter = ''"
          >
            <div style="font-size: 13px; color: var(--ant-color-text-secondary)">总漏洞数</div>
            <div style="font-size: 28px; font-weight: 700">{{ total }}</div>
            <div
              style="
                display: flex;
                height: 4px;
                border-radius: 2px;
                overflow: hidden;
                margin-top: 8px;
                background: var(--ant-color-fill-secondary);
              "
            >
              <span
                v-for="s in stackedSegments"
                :key="s.sev"
                :style="{ width: s.width, background: sevHex[s.sev] }"
              />
            </div>
          </a-card>
        </a-tooltip>
      </a-col>
      <a-col v-for="sev in SEVERITIES" :key="sev" :xs="12" :sm="8" :lg="4">
        <a-tooltip title="点击按此严重度筛选；再次点击清除">
          <a-card
            hoverable
            size="small"
            :style="{
              borderLeft: `3px solid ${sevHex[sev]}`,
              ...(sevFilter === sev ? { borderColor: sevHex[sev] } : {}),
            }"
            @click="toggleSev(sev)"
          >
            <div style="font-size: 13px; color: var(--ant-color-text-secondary)">
              {{ sevLabel[sev] }}
            </div>
            <div style="font-size: 28px; font-weight: 700">{{ sevCount(sev) }}</div>
            <div style="font-size: 12px; color: var(--ant-color-text-tertiary)">
              {{ sevPct(sev) }}
            </div>
          </a-card>
        </a-tooltip>
      </a-col>
    </a-row>

    <!-- 筛选区 -->
    <a-card size="small" style="margin-top: 16px">
      <div style="display: flex; gap: 12px; flex-wrap: wrap">
        <a-input
          v-model:value="searchText"
          placeholder="搜索标题、描述、类型、目标…"
          allow-clear
          style="flex: 1; min-width: 220px"
          @change="page = 1"
        />
        <a-select
          v-model:value="projectFilter"
          style="width: 180px"
          :options="[
            { label: '全部项目', value: '' },
            ...projects.map((p) => ({ label: p.name, value: p.id })),
          ]"
          title="按项目筛选"
          @change="(page = 1, fetchFindings())"
        />
        <a-select
          v-model:value="statusFilter"
          :options="statusOptions"
          style="width: 130px"
          title="状态"
          @change="page = 1"
        />
        <a-popover trigger="click" placement="bottomRight">
          <template #content>
            <div style="width: 420px">
              <div style="font-weight: 600; margin-bottom: 12px">更多筛选</div>
              <a-form layout="vertical" size="small">
                <a-form-item label="漏洞 ID">
                  <a-input
                    v-model:value="exactIdFilter"
                    placeholder="精确匹配漏洞 ID"
                    allow-clear
                    @change="page = 1"
                  />
                </a-form-item>
                <a-form-item label="会话 ID">
                  <a-tooltip title="暂未接入（数据无会话字段）">
                    <a-input disabled placeholder="筛选特定会话" />
                  </a-tooltip>
                </a-form-item>
                <a-form-item label="任务 / 队列 ID">
                  <a-tooltip title="暂未接入（数据无任务字段）">
                    <a-input disabled placeholder="筛选任务或队列 ID" />
                  </a-tooltip>
                </a-form-item>
                <a-form-item label="对话标签">
                  <a-tooltip title="暂未接入（数据无标签字段）">
                    <a-input disabled placeholder="筛选对话标签" />
                  </a-tooltip>
                </a-form-item>
                <a-form-item label="任务标签" style="margin-bottom: 0">
                  <a-tooltip title="暂未接入（数据无标签字段）">
                    <a-input disabled placeholder="筛选任务标签" />
                  </a-tooltip>
                </a-form-item>
              </a-form>
              <div style="display: flex; justify-content: flex-end; gap: 8px; margin-top: 12px">
                <a-button size="small" @click="((exactIdFilter = ''), (page = 1))">清空</a-button>
              </div>
            </div>
          </template>
          <a-button>
            更多筛选
            <a-badge v-if="exactIdFilter.trim()" :count="1" size="small" />
          </a-button>
        </a-popover>
        <a-button v-if="hasActiveFilters" type="link" @click="clearAllFilters">重置全部</a-button>
      </div>

      <!-- 已选条件 chips -->
      <div
        v-if="hasActiveFilters"
        style="
          margin-top: 12px;
          padding-top: 12px;
          border-top: 1px solid var(--ant-color-border-secondary);
          display: flex;
          gap: 8px;
          align-items: center;
          flex-wrap: wrap;
        "
      >
        <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">已选条件</span>
        <a-tag v-if="sevFilter" :color="sevColor[sevFilter]" closable @close="sevFilter = ''">
          严重度：{{ sevLabel[sevFilter] }}
        </a-tag>
        <a-tag
          v-if="statusFilter !== 'all'"
          closable
          @close="statusFilter = 'all'"
        >
          状态：{{ statusLabel[statusFilter] }}
        </a-tag>
        <a-tag
          v-if="projectFilter"
          closable
          @close="((projectFilter = ''), fetchFindings())"
        >
          项目：{{ projectName[projectFilter] || projectFilter }}
        </a-tag>
        <a-tag v-if="searchText" closable @close="searchText = ''">
          关键词：{{ searchText }}
        </a-tag>
        <a-tag v-if="exactIdFilter.trim()" closable @close="exactIdFilter = ''">
          漏洞 ID：{{ exactIdFilter.trim() }}
        </a-tag>
      </div>
    </a-card>

    <!-- 漏洞卡片列表 -->
    <a-spin :spinning="loading">
      <div style="margin-top: 16px; display: flex; flex-direction: column; gap: 12px">
        <a-card
          v-for="f in paged"
          :key="f.id"
          size="small"
          hoverable
          :style="{ borderLeft: `3px solid ${sevHex[f.severity] || sevHex.info}` }"
        >
          <!-- 卡片头 -->
          <div
            style="display: flex; align-items: center; gap: 8px; cursor: pointer; flex-wrap: wrap"
            @click="toggleExpand(f)"
          >
            <span
              :style="{
                display: 'inline-block',
                transition: 'transform 0.2s',
                transform: expandedId === f.id ? 'rotate(90deg)' : 'none',
                color: 'var(--ant-color-text-tertiary)',
              }"
            >▶</span>
            <span style="font-weight: 600; font-size: 14px">{{ f.title || '未命名漏洞' }}</span>
            <a-tag :color="sevColor[f.severity] || 'default'">
              {{ sevLabel[f.severity] || f.severity || 'info' }}
            </a-tag>
            <a-tag :color="statusColor[f.status] || 'default'">
              {{ statusLabel[f.status] || f.status || '—' }}
            </a-tag>
            <a-tag v-if="f.project_id" style="font-size: 12px">
              项目：{{ projectName[f.project_id] || f.project_id }}
            </a-tag>
            <a-tag v-else style="font-size: 12px">未绑定项目</a-tag>
            <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">
              {{ fmtTime(f.created_at) }}
            </span>
            <span style="margin-left: auto; display: flex; gap: 4px" @click.stop>
              <a-tooltip title="下载 Markdown">
                <a-button size="small" type="text" @click="downloadMarkdown(f)">⬇</a-button>
              </a-tooltip>
              <a-tooltip title="暂未接入（后端暂不支持编辑）">
                <a-button size="small" type="text" disabled>✏️</a-button>
              </a-tooltip>
              <a-tooltip title="暂未接入（后端暂不支持删除）">
                <a-button size="small" type="text" disabled danger>🗑</a-button>
              </a-tooltip>
              <a-select
                :value="['confirmed', 'false_positive'].includes(f.status) ? f.status : undefined"
                placeholder="审核状态"
                size="small"
                style="width: 130px"
                :loading="reviewing === f.id"
                :options="[
                  { label: '确认为漏洞', value: 'confirmed' },
                  { label: '标记误报', value: 'false_positive' },
                ]"
                @change="(v: string) => doReview(f, v)"
              />
            </span>
          </div>

          <!-- 展开详情 -->
          <div
            v-if="expandedId === f.id"
            style="
              margin-top: 12px;
              border-top: 1px solid var(--ant-color-border-secondary);
              padding-top: 12px;
            "
          >
            <a-typography-paragraph
              v-if="f.description"
              :ellipsis="{ rows: 4, expandable: true, symbol: '展开' }"
              style="white-space: pre-wrap"
            >
              {{ f.description }}
            </a-typography-paragraph>

            <a-descriptions bordered size="small" :column="2">
              <a-descriptions-item label="漏洞 ID">
                <span style="font-family: monospace; font-size: 12px">{{ f.id }}</span>
              </a-descriptions-item>
              <a-descriptions-item label="项目">
                {{ projectName[f.project_id] || '未绑定项目' }}
              </a-descriptions-item>
              <a-descriptions-item label="类型">
                {{ f.vuln_type || f.category || '—' }}
              </a-descriptions-item>
              <a-descriptions-item label="目标">{{ f.target || '—' }}</a-descriptions-item>
              <a-descriptions-item label="置信度">
                {{ typeof f.confidence === 'number' ? f.confidence.toFixed(2) : '—' }}
              </a-descriptions-item>
              <a-descriptions-item label="Run ID">
                <span style="font-family: monospace; font-size: 12px">{{ f.run_id || '—' }}</span>
              </a-descriptions-item>
              <a-descriptions-item label="审核信息" :span="2">
                <template v-if="f.reviewed_by">
                  {{ f.reviewed_by }} · {{ fmtTime(f.reviewed_at) }}
                  <span v-if="f.review_comment"> · {{ f.review_comment }}</span>
                </template>
                <template v-else>未审核</template>
              </a-descriptions-item>
            </a-descriptions>

            <!-- 关联证据 -->
            <div style="margin-top: 12px">
              <strong>关联证据：</strong>
              <a-spin :spinning="loadingEvidence === f.id" size="small">
                <div v-if="evidenceCache[f.id]?.length" style="margin-top: 8px">
                  <div
                    v-for="(ev, idx) in evidenceCache[f.id]"
                    :key="ev.id || idx"
                    style="
                      padding: 8px 0;
                      border-bottom: 1px dashed var(--ant-color-border-secondary);
                    "
                  >
                    <div style="display: flex; gap: 6px; margin-bottom: 4px">
                      <a-tag v-if="ev.content_hash" color="blue" style="font-family: monospace">
                        {{ ev.content_hash?.slice(0, 20) }}
                      </a-tag>
                      <a-tag>{{ ev.content_type || 'text/plain' }}</a-tag>
                    </div>
                    <pre
                      style="
                        background: var(--ant-color-bg-layout);
                        padding: 8px;
                        border-radius: 4px;
                        font-size: 11px;
                        max-height: 120px;
                        overflow: auto;
                        white-space: pre-wrap;
                        word-break: break-all;
                        margin: 0;
                      "
                    >{{ ev.source || ev.content || '(空)' }}</pre>
                  </div>
                </div>
                <div
                  v-else
                  style="margin-top: 8px; color: var(--ant-color-text-tertiary); font-size: 12px"
                >
                  暂无关联证据
                </div>
              </a-spin>
            </div>
          </div>
        </a-card>
        <a-empty
          v-if="!filtered.length && !loading"
          description="暂无漏洞记录"
          style="margin-top: 48px"
        />
      </div>
    </a-spin>

    <!-- 分页栏（对齐原版：左侧范围+每页数量，右侧首/上/页码/下/末） -->
    <a-card size="small" style="margin-top: 16px">
      <div style="display: flex; align-items: center; flex-wrap: wrap; gap: 12px">
        <span style="font-size: 13px; color: var(--ant-color-text-secondary)">
          显示 {{ rangeStart }}-{{ rangeEnd }} / 共 {{ filtered.length }} 条
        </span>
        <span style="font-size: 13px; color: var(--ant-color-text-secondary)">
          每页
          <a-select
            :value="pageSize"
            size="small"
            style="width: 70px; margin-left: 4px"
            :options="[10, 20, 50, 100].map((n) => ({ label: String(n), value: n }))"
            @change="(v: number) => changePageSize(v)"
          />
        </span>
        <span style="margin-left: auto; display: flex; gap: 8px; align-items: center">
          <a-button size="small" :disabled="page <= 1 || !filtered.length" @click="gotoPage(1)">
            首页
          </a-button>
          <a-button
            size="small"
            :disabled="page <= 1 || !filtered.length"
            @click="gotoPage(page - 1)"
          >
            上一页
          </a-button>
          <span style="font-size: 13px">第 {{ page }} / {{ totalPages }} 页</span>
          <a-button
            size="small"
            :disabled="page >= totalPages || !filtered.length"
            @click="gotoPage(page + 1)"
          >
            下一页
          </a-button>
          <a-button
            size="small"
            :disabled="page >= totalPages || !filtered.length"
            @click="gotoPage(totalPages)"
          >
            末页
          </a-button>
        </span>
      </div>
    </a-card>
  </div>
</template>
