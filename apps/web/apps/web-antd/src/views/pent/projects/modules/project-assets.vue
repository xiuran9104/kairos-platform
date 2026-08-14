<script lang="ts" setup>
import type { PentAsset } from '#/api/pent/ops-assets';

import { onMounted, reactive, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import { message } from 'ant-design-vue';

import { getPentAssets } from '#/api/pent/ops-assets';

const props = defineProps<{ projectId: string }>();
const router = useRouter();

const loading = ref(false);
const assets = ref<PentAsset[]>([]);
const search = ref('');
const riskLevel = ref<string>();
const pagination = reactive({ current: 1, pageSize: 10, total: 0 });

const riskColor: Record<string, string> = {
  critical: 'red',
  high: 'orange',
  medium: 'gold',
  low: 'blue',
  info: 'default',
};

const columns = [
  { title: '目标', dataIndex: 'target', key: 'target' },
  { title: '服务', key: 'service', width: 140 },
  { title: '标题', dataIndex: 'title', key: 'title', ellipsis: true },
  { title: '来源', dataIndex: 'source', key: 'source', width: 110 },
  { title: '风险等级', dataIndex: 'risk_level', key: 'risk_level', width: 100 },
  { title: '最近发现', dataIndex: 'last_seen', key: 'last_seen', width: 150 },
];

async function fetchAssets() {
  loading.value = true;
  try {
    const resp = await getPentAssets({
      page: pagination.current,
      page_size: pagination.pageSize,
      project_id: props.projectId,
      q: search.value || undefined,
      risk_level: riskLevel.value,
    });
    assets.value = resp?.assets || [];
    pagination.total = resp?.total || 0;
  } catch {
    message.error('加载项目资产失败');
    assets.value = [];
    pagination.total = 0;
  } finally {
    loading.value = false;
  }
}

function searchAssets() {
  pagination.current = 1;
  fetchAssets();
}

function onTableChange(page: { current?: number; pageSize?: number }) {
  pagination.current = page.current || 1;
  pagination.pageSize = page.pageSize || 10;
  fetchAssets();
}

function openAssetLibrary() {
  router.push({
    path: '/pent/ops/asset-library',
    query: { project_id: props.projectId },
  });
}

function fmtTime(t?: string) {
  return t ? t.slice(0, 16).replace('T', ' ') : '—';
}

watch(
  () => props.projectId,
  () => {
    pagination.current = 1;
    search.value = '';
    riskLevel.value = undefined;
    fetchAssets();
  },
);
onMounted(fetchAssets);
</script>

<template>
  <div>
    <div style="display: flex; align-items: center; gap: 8px; margin-bottom: 12px; flex-wrap: wrap">
      <a-input-search
        v-model:value="search"
        placeholder="搜索目标、IP、域名或服务"
        allow-clear
        style="width: 260px"
        @search="searchAssets"
      />
      <a-select
        v-model:value="riskLevel"
        placeholder="全部风险"
        allow-clear
        style="width: 120px"
        :options="[
          { label: '严重', value: 'critical' },
          { label: '高危', value: 'high' },
          { label: '中危', value: 'medium' },
          { label: '低危', value: 'low' },
          { label: '提示', value: 'info' },
        ]"
        @change="searchAssets"
      />
      <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">
        共 {{ pagination.total }} 项
      </span>
      <a-button size="small" style="margin-left: auto" :loading="loading" @click="fetchAssets">刷新</a-button>
      <a-button size="small" type="primary" @click="openAssetLibrary">管理项目资产</a-button>
    </div>
    <a-table
      :columns="columns"
      :data-source="assets"
      :loading="loading"
      row-key="id"
      size="small"
      :pagination="{
        current: pagination.current,
        pageSize: pagination.pageSize,
        total: pagination.total,
        showSizeChanger: true,
        showTotal: (t: number) => `共 ${t} 条`,
      }"
      @change="onTableChange"
    >
      <template #bodyCell="{ column, record }">
        <template v-if="column.key === 'service'">
          <a-tag v-if="record.protocol" size="small" color="blue">{{ record.protocol }}</a-tag>
          <span style="font-size: 12px">{{ record.server || '' }}</span>
        </template>
        <template v-else-if="column.key === 'risk_level'">
          <a-tag size="small" :color="riskColor[record.risk_level] || 'default'">
            {{ record.risk_level || 'info' }}
          </a-tag>
        </template>
        <template v-else-if="column.key === 'last_seen'">
          {{ fmtTime(record.last_seen) }}
        </template>
      </template>
      <template #emptyText>
        <a-empty description="该项目暂无资产，请到「安全作业 → 资产库」绑定" />
      </template>
    </a-table>
  </div>
</template>
