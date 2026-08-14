<script lang="ts" setup>
import { onMounted, reactive, ref } from 'vue';
import { message } from 'ant-design-vue';
import { getPentProjects, getPentRuns } from '#/api/pent';
import { getPentTrafficDetail, getPentTrafficPage } from '#/api/pent/traffic';
import PageHeader from '../components/PageHeader.vue';

const projects = ref<any[]>([]);
const runs = ref<any[]>([]);
const rows = ref<any[]>([]);
const loading = ref(false);
const total = ref(0);
const pagination = reactive({ current: 1, pageSize: 20, total: 0, showTotal: (t: number) => `共 ${t} 条` });

const filters = reactive<{
  project_id?: string;
  run_id?: string;
  method?: string;
  status_code?: number;
  url?: string;
  source?: string;
  duration_max_ms?: number;
}>({});

const detailVisible = ref(false);
const detail = ref<any>(null);
const detailLoading = ref(false);

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

async function fetch() {
  loading.value = true;
  try {
    const params: Record<string, any> = {
      page: pagination.current,
      page_size: pagination.pageSize,
    };
    for (const [key, value] of Object.entries(filters)) {
      if (value !== undefined && value !== null && value !== '') params[key] = value;
    }
    const res = await getPentTrafficPage(params);
    rows.value = res?.items ?? [];
    total.value = res?.total ?? 0;
    pagination.total = total.value;
  } catch (error: any) {
    message.error(error?.message || '加载失败');
  } finally {
    loading.value = false;
  }
}

function resetFilters() {
  filters.project_id = undefined;
  filters.run_id = undefined;
  filters.method = undefined;
  filters.status_code = undefined;
  filters.url = undefined;
  filters.source = undefined;
  filters.duration_max_ms = undefined;
  pagination.current = 1;
  fetch();
}

function handleTableChange(pager: any) {
  pagination.current = pager.current;
  pagination.pageSize = pager.pageSize;
  fetch();
}

async function showDetail(record: any) {
  detail.value = record;
  detailVisible.value = true;
  detailLoading.value = true;
  try {
    detail.value = await getPentTrafficDetail(record.id);
  } catch (error: any) {
    message.error(error?.message || '加载详情失败');
  } finally {
    detailLoading.value = false;
  }
}

function fmt(value?: string) {
  return value ? new Date(value).toLocaleString() : '-';
}
function sizeText(value?: number) {
  if (value == null) return '-';
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 / 1024).toFixed(2)} MB`;
}
function statusColor(status: number | undefined) {
  if (status == null) return 'default';
  if (status >= 500) return 'red';
  if (status >= 400) return 'volcano';
  if (status >= 300) return 'cyan';
  return 'green';
}
function headersText(headers: any) {
  if (!headers || typeof headers !== 'object') return '-';
  return Object.entries(headers)
    .map(([key, value]) => `${key}: ${value}`)
    .join('\n');
}

const columns = [
  { title: '时间', dataIndex: 'created_at', key: 'time', width: 170 },
  { title: '方法', dataIndex: 'method', key: 'method', width: 90 },
  { title: 'URL', dataIndex: 'url', key: 'url', ellipsis: true },
  { title: '状态码', dataIndex: 'response_status', key: 'status', width: 90, align: 'right' },
  { title: '来源', dataIndex: 'tool_name', key: 'source', width: 120, ellipsis: true },
  { title: '耗时', dataIndex: 'duration_ms', key: 'duration', width: 90, align: 'right' },
  { title: '请求', dataIndex: 'request_size_bytes', key: 'req', width: 90, align: 'right' },
  { title: '响应', dataIndex: 'response_size_bytes', key: 'resp', width: 90, align: 'right' },
  { title: 'Run ID', dataIndex: 'run_id', key: 'run', width: 130, ellipsis: true },
  { title: '操作', key: 'actions', width: 80 },
];

onMounted(async () => {
  await Promise.all([loadProjects(), loadRuns()]);
  await fetch();
});
</script>

<template>
  <div style="padding: 24px; min-height: 100%; background-color: var(--ant-color-bg-layout)">
    <PageHeader title="LLM 流量" subtitle="HTTP 请求 / 响应交换记录（自动脱敏）">
      <template #extra>
        <a-button @click="fetch" :loading="loading">刷新</a-button>
      </template>
    </PageHeader>

    <a-card size="small" style="border-radius: 8px; margin-bottom: 12px">
      <a-form layout="inline" style="row-gap: 8px">
        <a-form-item label="项目">
          <a-select
            v-model:value="filters.project_id"
            allow-clear
            show-search
            option-filter-prop="label"
            placeholder="全部项目"
            style="width: 180px"
            :options="projects.map((p: any) => ({ label: p.name, value: p.id }))"
          />
        </a-form-item>
        <a-form-item label="Run">
          <a-select
            v-model:value="filters.run_id"
            allow-clear
            show-search
            option-filter-prop="label"
            placeholder="全部 Run"
            style="width: 200px"
            :options="runs.map((r: any) => ({ label: r.id?.slice(0, 16), value: r.id }))"
          />
        </a-form-item>
        <a-form-item label="方法">
          <a-select v-model:value="filters.method" allow-clear placeholder="全部" style="width: 110px">
            <a-select-option value="GET">GET</a-select-option>
            <a-select-option value="POST">POST</a-select-option>
            <a-select-option value="PUT">PUT</a-select-option>
            <a-select-option value="DELETE">DELETE</a-select-option>
            <a-select-option value="PATCH">PATCH</a-select-option>
            <a-select-option value="HEAD">HEAD</a-select-option>
            <a-select-option value="OPTIONS">OPTIONS</a-select-option>
          </a-select>
        </a-form-item>
        <a-form-item label="状态码">
          <a-input-number
            v-model:value="filters.status_code"
            :min="100"
            :max="599"
            placeholder="如 200"
            style="width: 110px"
          />
        </a-form-item>
        <a-form-item label="来源工具">
          <a-input v-model:value="filters.source" allow-clear placeholder="工具名" style="width: 140px" />
        </a-form-item>
        <a-form-item label="URL 包含">
          <a-input v-model:value="filters.url" allow-clear placeholder="关键字" style="width: 180px" />
        </a-form-item>
        <a-form-item label="耗时 ≤">
          <a-input-number
            v-model:value="filters.duration_max_ms"
            :min="0"
            placeholder="毫秒"
            style="width: 120px"
          />
        </a-form-item>
        <a-form-item>
          <a-space>
            <a-button type="primary" @click="pagination.current = 1; fetch()">查询</a-button>
            <a-button @click="resetFilters">重置</a-button>
          </a-space>
        </a-form-item>
      </a-form>
    </a-card>

    <a-card size="small" style="border-radius: 8px">
      <a-table
        :columns="columns"
        :data-source="rows"
        row-key="id"
        size="middle"
        :loading="loading"
        :pagination="pagination"
        :scroll="{ x: 1200 }"
        @change="handleTableChange"
      >
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'time'">{{ fmt(record.created_at) }}</template>
          <template v-if="column.key === 'method'">
            <a-tag color="blue">{{ record.method }}</a-tag>
          </template>
          <template v-if="column.key === 'url'">
            <a-tooltip :title="record.url">
              <span style="font-family: monospace; font-size: 12px">{{ record.url }}</span>
            </a-tooltip>
          </template>
          <template v-if="column.key === 'status'">
            <a-tag :color="statusColor(record.response_status)">{{ record.response_status ?? '-' }}</a-tag>
          </template>
          <template v-if="column.key === 'duration'">
            <span style="font-family: monospace">{{ record.duration_ms != null ? `${record.duration_ms} ms` : '-' }}</span>
          </template>
          <template v-if="column.key === 'req'">{{ sizeText(record.request_size_bytes) }}</template>
          <template v-if="column.key === 'resp'">{{ sizeText(record.response_size_bytes) }}</template>
          <template v-if="column.key === 'run'">
            <span style="font-family: monospace; font-size: 12px">{{ record.run_id?.slice(0, 16) || '-' }}</span>
          </template>
          <template v-if="column.key === 'actions'">
            <a-button type="link" size="small" @click="showDetail(record)">详情</a-button>
          </template>
        </template>
      </a-table>
      <a-empty v-if="!rows.length && !loading" description="暂无 HTTP 流量" />
    </a-card>

    <a-drawer v-model:open="detailVisible" title="流量详情" width="720px" :loading="detailLoading">
      <template v-if="detail">
        <a-descriptions bordered size="small" :column="2" style="margin-bottom: 16px">
          <a-descriptions-item label="方法">
            <a-tag color="blue">{{ detail.method }}</a-tag>
          </a-descriptions-item>
          <a-descriptions-item label="状态码">
            <a-tag :color="statusColor(detail.response_status)">{{ detail.response_status ?? '-' }}</a-tag>
          </a-descriptions-item>
          <a-descriptions-item label="URL" :span="2">
            <span style="font-family: monospace; font-size: 12px; word-break: break-all">{{ detail.url }}</span>
          </a-descriptions-item>
          <a-descriptions-item label="来源工具">{{ detail.tool_name || '-' }}</a-descriptions-item>
          <a-descriptions-item label="耗时">{{ detail.duration_ms != null ? `${detail.duration_ms} ms` : '-' }}</a-descriptions-item>
          <a-descriptions-item label="Run ID">{{ detail.run_id || '-' }}</a-descriptions-item>
          <a-descriptions-item label="采集时间">{{ fmt(detail.created_at) }}</a-descriptions-item>
          <a-descriptions-item v-if="detail.replay_of" label="重放自" :span="2">{{ detail.replay_of }}</a-descriptions-item>
        </a-descriptions>

        <a-tabs size="small">
          <a-tab-pane key="request" tab="请求">
            <a-collapse :default-active-key="['headers', 'body']">
              <a-collapse-panel key="headers" header="请求头">
                <pre class="payload">{{ headersText(detail.request_headers) }}</pre>
              </a-collapse-panel>
              <a-collapse-panel key="body" header="请求体">
                <pre class="payload">{{ detail.request_body || '(空)' }}</pre>
              </a-collapse-panel>
            </a-collapse>
          </a-tab-pane>
          <a-tab-pane key="response" tab="响应">
            <a-collapse :default-active-key="['headers', 'body']">
              <a-collapse-panel key="headers" header="响应头">
                <pre class="payload">{{ headersText(detail.response_headers) }}</pre>
              </a-collapse-panel>
              <a-collapse-panel key="body" header="响应体">
                <pre class="payload">{{ detail.response_body || '(空)' }}</pre>
              </a-collapse-panel>
            </a-collapse>
          </a-tab-pane>
        </a-tabs>
        <a-alert
          type="info"
          show-icon
          style="margin-top: 12px"
          message="敏感字段（Authorization / Cookie / 密钥）已由后端脱敏处理"
        />
      </template>
    </a-drawer>
  </div>
</template>

<style scoped>
.payload {
  margin: 0;
  padding: 10px;
  max-height: 360px;
  overflow: auto;
  color: var(--ant-color-text);
  font-size: 12px;
  white-space: pre-wrap;
  word-break: break-all;
  background: var(--ant-color-fill-quaternary);
  border-radius: 6px;
}
</style>
