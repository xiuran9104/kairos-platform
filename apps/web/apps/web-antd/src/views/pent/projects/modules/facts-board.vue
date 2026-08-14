<script lang="ts" setup>
import type { PentFact, PentProjectStats } from '#/api/pent/ops-facts';

import { onMounted, ref, watch } from 'vue';
import { message } from 'ant-design-vue';

import {
  createPentFact,
  deletePentFact,
  deprecatePentFact,
  getPentProjectFacts,
  getPentProjectStats,
  restorePentFact,
  updatePentFact,
} from '#/api/pent/ops-facts';

const props = defineProps<{ projectId: string }>();

// ── 统计 ───────────────────────────────────────────────────────────────────
const stats = ref<PentProjectStats | null>(null);

async function fetchStats() {
  try {
    stats.value = await getPentProjectStats(props.projectId);
  } catch {
    stats.value = null;
  }
}

// ── 列表与筛选 ─────────────────────────────────────────────────────────────
const loading = ref(false);
const facts = ref<PentFact[]>([]);
const search = ref('');
const category = ref<string>();
const confidence = ref<string>();
const sparseOnly = ref(false);
const excludeDeprecated = ref(false);

const categories = [
  'target',
  'auth',
  'infra',
  'business',
  'finding',
  'chain',
  'exploit',
  'poc',
  'note',
  'company',
  'supplychain',
];
const categoryLabel: Record<string, string> = {
  auth: '认证',
  business: '业务',
  chain: '关系链',
  company: '企业',
  exploit: '利用',
  finding: '发现',
  infra: '基础设施',
  note: '备注',
  poc: '验证',
  supplychain: '供应链',
  target: '目标',
};
const categoryColor: Record<string, string> = {
  auth: 'purple',
  business: 'cyan',
  company: 'geekblue',
  exploit: 'volcano',
  finding: 'red',
  infra: 'blue',
  supplychain: 'orange',
  target: 'green',
};
const confidenceColor: Record<string, string> = {
  confirmed: 'green',
  tentative: 'orange',
  deprecated: 'default',
};
const confidenceLabel: Record<string, string> = {
  confirmed: '已确认',
  tentative: '待确认',
  deprecated: '已废弃',
};

async function fetchFacts() {
  loading.value = true;
  try {
    const params: Record<string, any> = {};
    if (search.value) params.search = search.value;
    if (category.value) params.category = category.value;
    if (confidence.value) params.confidence = confidence.value;
    if (sparseOnly.value) params.sparse_only = true;
    if (excludeDeprecated.value) params.exclude_deprecated = true;
    facts.value = (await getPentProjectFacts(props.projectId, params)) || [];
  } catch {
    message.error('加载事实失败');
    facts.value = [];
  } finally {
    loading.value = false;
  }
}

async function refresh() {
  await Promise.all([fetchStats(), fetchFacts()]);
}

// ── 新建 / 编辑 ────────────────────────────────────────────────────────────
const showEdit = ref(false);
const saving = ref(false);
const editingId = ref<string | null>(null);
const form = ref({
  fact_key: '',
  category: 'note',
  summary: '',
  body: '',
  confidence: 'tentative',
  pinned: false,
  vuln_id: '',
  links_text: '',
});

function openCreate() {
  editingId.value = null;
  form.value = {
    fact_key: '',
    category: 'note',
    summary: '',
    body: '',
    confidence: 'tentative',
    pinned: false,
    vuln_id: '',
    links_text: '',
  };
  showEdit.value = true;
}

function openEdit(f: PentFact) {
  editingId.value = f.id;
  form.value = {
    fact_key: f.fact_key,
    category: f.category || 'note',
    summary: f.summary || '',
    body: f.body || '',
    confidence: f.confidence === 'deprecated' ? 'tentative' : f.confidence,
    pinned: !!f.pinned,
    vuln_id: f.vuln_id || '',
    links_text: f.links_text || '',
  };
  showEdit.value = true;
}

async function doSave() {
  if (!form.value.fact_key.trim() || !form.value.summary.trim()) {
    message.warning('fact_key 与摘要必填');
    return;
  }
  saving.value = true;
  try {
    const payload = {
      ...form.value,
      fact_key: form.value.fact_key.trim(),
      summary: form.value.summary.trim(),
      vuln_id: form.value.vuln_id.trim() || undefined,
    };
    if (editingId.value) await updatePentFact(editingId.value, payload);
    else await createPentFact(props.projectId, payload);
    message.success('已保存');
    showEdit.value = false;
    await refresh();
  } catch {
    message.error('保存失败（fact_key 可能重复）');
  } finally {
    saving.value = false;
  }
}

// ── 详情 / 状态操作 ────────────────────────────────────────────────────────
const detail = ref<PentFact | null>(null);
const showDetail = ref(false);

function openDetail(f: PentFact) {
  detail.value = f;
  showDetail.value = true;
}

async function toggleDeprecate(f: PentFact) {
  try {
    if (f.deprecated) await restorePentFact(f.id);
    else await deprecatePentFact(f.id);
    message.success(f.deprecated ? '已恢复' : '已废弃');
    await refresh();
  } catch {
    message.error('操作失败');
  }
}

async function confirmFact(f: PentFact) {
  try {
    await updatePentFact(f.id, { confidence: 'confirmed' });
    message.success('事实已确认');
    await refresh();
  } catch {
    message.error('确认失败');
  }
}

async function doDelete(f: PentFact) {
  try {
    await deletePentFact(f.id);
    message.success('已删除');
    await refresh();
  } catch {
    message.error('删除失败');
  }
}

const columns = [
  { title: 'Key', dataIndex: 'fact_key', key: 'fact_key', width: 200 },
  { title: '分类', dataIndex: 'category', key: 'category', width: 100 },
  { title: '摘要', dataIndex: 'summary', key: 'summary' },
  { title: '置信度', dataIndex: 'confidence', key: 'confidence', width: 100 },
  { title: '更新', dataIndex: 'updated_at', key: 'updated_at', width: 150 },
  { title: '操作', key: 'actions', width: 220 },
];

function fmtTime(t?: string) {
  return t ? t.slice(0, 16).replace('T', ' ') : '—';
}

watch(() => props.projectId, refresh);
onMounted(refresh);
</script>

<template>
  <div>
    <!-- 统计 chips -->
    <div style="display: flex; gap: 8px; margin-bottom: 12px; flex-wrap: wrap">
      <a-tag color="blue">事实 {{ stats?.fact_count ?? 0 }}</a-tag>
      <a-tag color="red">漏洞 {{ stats?.vuln_count ?? 0 }}</a-tag>
      <a-tag color="purple">对话 {{ stats?.conversation_count ?? 0 }}</a-tag>
      <a-tag color="orange">待补全 {{ stats?.sparse_fact_count ?? 0 }}</a-tag>
    </div>

    <!-- 筛选行 -->
    <div style="display: flex; gap: 8px; margin-bottom: 12px; flex-wrap: wrap; align-items: center">
      <a-input-search
        v-model:value="search"
        placeholder="搜索 key/摘要/正文"
        style="width: 220px"
        allow-clear
        @search="fetchFacts"
      />
      <a-select
        v-model:value="category"
        placeholder="分类"
        allow-clear
        style="width: 130px"
        :options="categories.map((c) => ({ label: c, value: c }))"
        @change="fetchFacts"
      />
      <a-select
        v-model:value="confidence"
        placeholder="置信度"
        allow-clear
        style="width: 120px"
        :options="[
          { label: '已确认', value: 'confirmed' },
          { label: '待确认', value: 'tentative' },
          { label: '已废弃', value: 'deprecated' },
        ]"
        @change="fetchFacts"
      />
      <a-checkbox v-model:checked="sparseOnly" @change="fetchFacts">仅待补全</a-checkbox>
      <a-checkbox v-model:checked="excludeDeprecated" @change="fetchFacts">隐藏废弃</a-checkbox>
      <a-button size="small" :loading="loading" @click="refresh">刷新</a-button>
      <a-button size="small" type="primary" @click="openCreate">添加事实</a-button>
    </div>

    <a-table
      :columns="columns"
      :data-source="facts"
      :loading="loading"
      row-key="id"
      size="small"
      :pagination="{ pageSize: 10, showTotal: (t: number) => `共 ${t} 条` }"
    >
      <template #bodyCell="{ column, record }">
        <template v-if="column.key === 'fact_key'">
          <span v-if="record.pinned" title="置顶" style="margin-right: 4px">📌</span>
          <span :style="record.deprecated ? 'text-decoration: line-through; opacity: 0.6' : ''">
            {{ record.fact_key }}
          </span>
        </template>
        <template v-else-if="column.key === 'category'">
          <a-tag size="small" :color="categoryColor[record.category]">
            {{ categoryLabel[record.category] || record.category }}
          </a-tag>
        </template>
        <template v-else-if="column.key === 'summary'">
          <a style="color: var(--ant-color-text)" @click="openDetail(record)">
            {{ record.summary }}
          </a>
        </template>
        <template v-else-if="column.key === 'confidence'">
          <a-tag size="small" :color="confidenceColor[record.confidence] || 'default'">
            {{ confidenceLabel[record.confidence] || record.confidence }}
          </a-tag>
        </template>
        <template v-else-if="column.key === 'updated_at'">
          {{ fmtTime(record.updated_at) }}
        </template>
        <template v-else-if="column.key === 'actions'">
          <a-space :size="4">
            <a-button size="small" type="link" @click="openDetail(record)">详情</a-button>
            <a-button size="small" type="link" @click="openEdit(record)">编辑</a-button>
            <a-button
              v-if="record.confidence === 'tentative' && !record.deprecated"
              size="small"
              type="link"
              @click="confirmFact(record)"
            >确认</a-button>
            <a-button size="small" type="link" @click="toggleDeprecate(record)">
              {{ record.deprecated ? '恢复' : '废弃' }}
            </a-button>
            <a-popconfirm title="确定删除该事实？" @confirm="doDelete(record)">
              <a-button size="small" type="link" danger>删除</a-button>
            </a-popconfirm>
          </a-space>
        </template>
      </template>
      <template #emptyText>
        <a-empty description="暂无事实，点击「添加事实」创建" />
      </template>
    </a-table>

    <!-- 新建/编辑 modal -->
    <a-modal
      v-model:open="showEdit"
      :title="editingId ? '编辑事实' : '添加事实'"
      :confirm-loading="saving"
      width="640px"
      @ok="doSave"
    >
      <a-form layout="vertical">
        <a-row :gutter="12">
          <a-col :span="12">
            <a-form-item label="fact_key" required>
              <a-input
                v-model:value="form.fact_key"
                :disabled="!!editingId"
                placeholder="如 company/root-example"
              />
            </a-form-item>
          </a-col>
          <a-col :span="6">
            <a-form-item label="分类">
              <a-select
                v-model:value="form.category"
                :options="categories.map((c) => ({ label: c, value: c }))"
              />
            </a-form-item>
          </a-col>
          <a-col :span="6">
            <a-form-item label="置信度">
              <a-select
                v-model:value="form.confidence"
                :options="[
                  { label: '已确认', value: 'confirmed' },
                  { label: '待确认', value: 'tentative' },
                ]"
              />
            </a-form-item>
          </a-col>
        </a-row>
        <a-form-item label="摘要" required>
          <a-input v-model:value="form.summary" placeholder="一句话概括" />
        </a-form-item>
        <a-form-item label="正文">
          <a-textarea v-model:value="form.body" :rows="5" placeholder="详情，可为 JSON" />
        </a-form-item>
        <a-row :gutter="12">
          <a-col :span="8">
            <a-form-item label="关联漏洞 ID">
              <a-input v-model:value="form.vuln_id" placeholder="可选" />
            </a-form-item>
          </a-col>
          <a-col :span="8">
            <a-form-item label="关系链接文本">
              <a-input v-model:value="form.links_text" placeholder="可选" />
            </a-form-item>
          </a-col>
          <a-col :span="8">
            <a-form-item label="置顶">
              <a-switch v-model:checked="form.pinned" />
            </a-form-item>
          </a-col>
        </a-row>
      </a-form>
    </a-modal>

    <!-- 详情 drawer -->
    <a-drawer v-model:open="showDetail" title="事实详情" width="520">
      <template v-if="detail">
        <a-descriptions :column="1" size="small" bordered>
          <a-descriptions-item label="fact_key">{{ detail.fact_key }}</a-descriptions-item>
          <a-descriptions-item label="分类">{{ detail.category }}</a-descriptions-item>
          <a-descriptions-item label="置信度">
            {{ confidenceLabel[detail.confidence] || detail.confidence }}
          </a-descriptions-item>
          <a-descriptions-item label="摘要">{{ detail.summary }}</a-descriptions-item>
          <a-descriptions-item label="关联漏洞">{{ detail.vuln_id || '—' }}</a-descriptions-item>
          <a-descriptions-item label="更新时间">{{ fmtTime(detail.updated_at) }}</a-descriptions-item>
        </a-descriptions>
        <a-card size="small" title="正文" style="margin-top: 12px">
          <pre style="white-space: pre-wrap; word-break: break-all; font-size: 12px; margin: 0">{{
            detail.body || '（空）'
          }}</pre>
        </a-card>
        <a-card v-if="detail.links_text" size="small" title="关系链接" style="margin-top: 12px">
          <pre style="white-space: pre-wrap; font-size: 12px; margin: 0">{{ detail.links_text }}</pre>
        </a-card>
      </template>
    </a-drawer>
  </div>
</template>
