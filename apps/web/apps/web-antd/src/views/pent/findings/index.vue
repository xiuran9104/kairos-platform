<script lang="ts" setup>
import { computed, onMounted, ref, watch } from 'vue';
import { message } from 'ant-design-vue';

import { getPentProjectFindings, getPentProjects, getPentRunEvidence } from '#/api/pent';
import { requestClient } from '#/api/request';

// ── state ──────────────────────────────────────────────────────────────────
const projects = ref<any[]>([]);
const selectedProjectId = ref<string>();
const findings = ref<any[]>([]);
const summary = ref<Record<string, number>>({});
const loading = ref(false);

const sevFilter = ref('all');
const statusFilter = ref('all');
const searchText = ref('');

const sevColor: Record<string, string> = {
  critical: 'red',
  high: 'orange',
  medium: 'gold',
  low: 'blue',
  info: 'default',
};
const sevLabel: Record<string, string> = {
  critical: '严重',
  high: '高危',
  medium: '中危',
  low: '低危',
  info: '信息',
};

const statusColor: Record<string, string> = {
  open: 'blue',
  confirmed: 'green',
  false_positive: 'default',
};
const statusLabel: Record<string, string> = {
  open: '待审核',
  confirmed: '已确认',
  false_positive: '误报',
};

const statusOptions = [
  { label: '全部状态', value: 'all' },
  { label: '待审核', value: 'open' },
  { label: '已确认', value: 'confirmed' },
  { label: '误报', value: 'false_positive' },
];

// ── data loading ───────────────────────────────────────────────────────────
async function loadProjects() {
  try {
    projects.value = ((await getPentProjects()) as any[]) || [];
  } catch {
    projects.value = [];
  }
}

async function fetchFindings() {
  if (!selectedProjectId.value) {
    findings.value = [];
    summary.value = {};
    return;
  }
  loading.value = true;
  evidenceCache.value = {};
  expandedId.value = null;
  try {
    const resp: any = await getPentProjectFindings(selectedProjectId.value);
    findings.value = resp?.items ?? [];
    summary.value = resp?.severity_summary ?? {};
  } catch {
    findings.value = [];
    summary.value = {};
  } finally {
    loading.value = false;
  }
}

watch(selectedProjectId, () => {
  sevFilter.value = 'all';
  statusFilter.value = 'all';
  searchText.value = '';
  fetchFindings();
});

onMounted(async () => {
  await loadProjects();
  if (projects.value.length > 0) selectedProjectId.value = projects.value[0].id;
});

// ── stats board ────────────────────────────────────────────────────────────
const total = computed(() => findings.value.length);

function toggleSev(sev: string) {
  sevFilter.value = sevFilter.value === sev ? 'all' : sev;
}

// ── filter ─────────────────────────────────────────────────────────────────
const filtered = computed(() => {
  let list = findings.value;
  if (sevFilter.value !== 'all') {
    list = list.filter((f) => (f.severity || 'info') === sevFilter.value);
  }
  if (statusFilter.value !== 'all') {
    list = list.filter((f) => f.status === statusFilter.value);
  }
  if (searchText.value) {
    const q = searchText.value.toLowerCase();
    list = list.filter(
      (f) =>
        (f.title || '').toLowerCase().includes(q) ||
        (f.vuln_type || '').toLowerCase().includes(q) ||
        (f.target || '').toLowerCase().includes(q),
    );
  }
  return list;
});

function fmtTime(t?: string) {
  return t ? t.slice(0, 16).replace('T', ' ') : '—';
}

// ── expand & evidence ──────────────────────────────────────────────────────
const expandedId = ref<string | null>(null);
const evidenceCache = ref<Record<string, any[]>>({});
const loadingEvidence = ref<string | null>(null);

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

// ── review (status change) ─────────────────────────────────────────────────
const reviewing = ref<string | null>(null);

async function doReview(f: any, decision: string) {
  if (reviewing.value) return;
  reviewing.value = f.id;
  try {
    const updated: any = await requestClient.post(`/pent/findings/${f.id}`, { decision });
    if (updated?.status) f.status = updated.status;
    else f.status = decision;
    message.success(decision === 'confirmed' ? '已确认为漏洞' : '已标记为误报');
  } catch {
    message.error('状态更新失败（可能需要先关联证据）');
  } finally {
    reviewing.value = null;
  }
}
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <a-page-header title="漏洞管理" sub-title="项目漏洞统计、筛选与审核">
      <template #extra>
        <span style="font-size: 13px; color: var(--ant-color-text-secondary); margin-right: 8px">项目</span>
        <a-select
          v-model:value="selectedProjectId"
          :options="projects.map((p) => ({ label: p.name, value: p.id }))"
          placeholder="选择项目"
          style="width: 240px"
        />
        <a-button style="margin-left: 8px" :loading="loading" @click="fetchFindings">刷新</a-button>
      </template>
    </a-page-header>

    <a-empty
      v-if="!selectedProjectId && !loading"
      description="请先选择项目"
      style="margin-top: 96px"
    />

    <template v-else>
      <!-- 统计看板 -->
      <a-row :gutter="[12, 12]" style="margin-top: 16px">
        <a-col :xs="12" :sm="8" :lg="4">
          <a-card
            hoverable
            size="small"
            :style="sevFilter === 'all' ? { borderColor: 'var(--ant-color-primary)' } : {}"
            @click="sevFilter = 'all'"
          >
            <a-statistic title="总数" :value="total" />
          </a-card>
        </a-col>
        <a-col v-for="sev in ['critical', 'high', 'medium', 'low', 'info']" :key="sev" :xs="12" :sm="8" :lg="4">
          <a-card
            hoverable
            size="small"
            :style="sevFilter === sev ? { borderColor: 'var(--ant-color-primary)' } : {}"
            @click="toggleSev(sev)"
          >
            <a-statistic :title="sevLabel[sev]" :value="summary[sev] || 0">
              <template #suffix>
                <a-tag :color="sevColor[sev]" style="margin-inline-end: 0">{{ sev }}</a-tag>
              </template>
            </a-statistic>
          </a-card>
        </a-col>
      </a-row>

      <!-- 筛选行 -->
      <a-card size="small" style="margin-top: 16px">
        <div style="display: flex; gap: 12px; flex-wrap: wrap">
          <a-input-search
            v-model:value="searchText"
            placeholder="搜索标题 / 类型..."
            allow-clear
            style="width: 260px"
          />
          <a-select v-model:value="statusFilter" :options="statusOptions" style="width: 140px" />
          <span
            v-if="sevFilter !== 'all'"
            style="align-self: center; font-size: 13px; color: var(--ant-color-text-secondary)"
          >
            严重度筛选：
            <a-tag :color="sevColor[sevFilter]" closable @close="sevFilter = 'all'">
              {{ sevLabel[sevFilter] }}
            </a-tag>
          </span>
          <span style="margin-left: auto; font-size: 13px; color: var(--ant-color-text-tertiary); align-self: center">
            共 {{ filtered.length }} 个漏洞
          </span>
        </div>
      </a-card>

      <!-- 卡片列表 -->
      <a-spin :spinning="loading">
        <div style="margin-top: 16px; display: flex; flex-direction: column; gap: 12px">
          <a-card v-for="f in filtered" :key="f.id" size="small" hoverable>
            <div
              style="display: flex; align-items: center; gap: 8px; cursor: pointer; flex-wrap: wrap"
              @click="toggleExpand(f)"
            >
              <span style="font-weight: 600; font-size: 14px">{{ f.title || '未命名漏洞' }}</span>
              <a-tag :color="sevColor[f.severity] || 'default'">{{ f.severity || 'info' }}</a-tag>
              <a-tag :color="statusColor[f.status] || 'default'">
                {{ statusLabel[f.status] || f.status || '—' }}
              </a-tag>
              <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">
                {{ fmtTime(f.created_at) }}
              </span>
              <span style="margin-left: auto" @click.stop>
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
              <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">
                {{ expandedId === f.id ? '收起 ▲' : '展开 ▼' }}
              </span>
            </div>

            <!-- 展开详情 -->
            <div
              v-if="expandedId === f.id"
              style="margin-top: 12px; border-top: 1px solid var(--ant-color-border-secondary); padding-top: 12px"
            >
              <a-descriptions bordered size="small" :column="2">
                <a-descriptions-item label="ID">
                  <span style="font-family: monospace; font-size: 12px">{{ f.id }}</span>
                </a-descriptions-item>
                <a-descriptions-item label="目标">{{ f.target || '—' }}</a-descriptions-item>
                <a-descriptions-item label="类型">{{ f.vuln_type || f.category || '—' }}</a-descriptions-item>
                <a-descriptions-item label="置信度">
                  {{ typeof f.confidence === 'number' ? f.confidence.toFixed(2) : '—' }}
                </a-descriptions-item>
                <a-descriptions-item label="Run ID">
                  <span style="font-family: monospace; font-size: 12px">{{ f.run_id || '—' }}</span>
                </a-descriptions-item>
                <a-descriptions-item label="审核信息">
                  <template v-if="f.reviewed_by">
                    {{ f.reviewed_by }} · {{ fmtTime(f.reviewed_at) }}
                    <span v-if="f.review_comment"> · {{ f.review_comment }}</span>
                  </template>
                  <template v-else>未审核</template>
                </a-descriptions-item>
              </a-descriptions>

              <div style="margin-top: 12px">
                <strong>描述：</strong>
                <a-typography-paragraph
                  :ellipsis="{ rows: 4, expandable: true, symbol: '展开' }"
                  style="margin-bottom: 0; white-space: pre-wrap"
                >
                  {{ f.description || '（无描述）' }}
                </a-typography-paragraph>
              </div>

              <!-- 关联证据 -->
              <div style="margin-top: 12px">
                <strong>关联证据：</strong>
                <a-spin :spinning="loadingEvidence === f.id" size="small">
                  <div v-if="evidenceCache[f.id]?.length" style="margin-top: 8px">
                    <div
                      v-for="(ev, idx) in evidenceCache[f.id]"
                      :key="ev.id || idx"
                      style="padding: 8px 0; border-bottom: 1px dashed var(--ant-color-border-secondary)"
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
          <a-empty v-if="!filtered.length && !loading" description="暂无漏洞" style="margin-top: 48px" />
        </div>
      </a-spin>
    </template>
  </div>
</template>
