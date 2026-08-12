<script lang="ts" setup>
import type { PentAsset } from '#/api/pent/ops-assets';

import { onMounted, ref, watch } from 'vue';
import { message } from 'ant-design-vue';

import { getPentAssets } from '#/api/pent/ops-assets';

const props = defineProps<{ projectId: string }>();

const loading = ref(false);
const assets = ref<PentAsset[]>([]);
const total = ref(0);

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
      project_id: props.projectId,
      page_size: 50,
    });
    assets.value = resp?.assets || [];
    total.value = resp?.total || 0;
  } catch {
    message.error('加载项目资产失败');
    assets.value = [];
    total.value = 0;
  } finally {
    loading.value = false;
  }
}

function fmtTime(t?: string) {
  return t ? t.slice(0, 16).replace('T', ' ') : '—';
}

watch(() => props.projectId, fetchAssets);
onMounted(fetchAssets);
</script>

<template>
  <div>
    <div style="display: flex; align-items: center; margin-bottom: 12px">
      <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">
        该项目共 {{ total }} 项资产（最多显示 50 条）
      </span>
      <a-button size="small" style="margin-left: auto" :loading="loading" @click="fetchAssets">
        刷新
      </a-button>
    </div>
    <a-table
      :columns="columns"
      :data-source="assets"
      :loading="loading"
      row-key="id"
      size="small"
      :pagination="{ pageSize: 10, showTotal: (t: number) => `共 ${t} 条` }"
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
