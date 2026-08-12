<script lang="ts" setup>
import { onMounted, ref } from 'vue';
import { requestClient } from '#/api/request';
defineOptions({ name: 'PentAttackGraph' });
const graph = ref<any>({ nodes: [], edges: [] });
const loading = ref(false);
async function load() {
  loading.value = true;
  try { graph.value = (await requestClient.get('/pent/rmos/attack-graph'))?.data ?? { nodes: [], edges: [] }; }
  finally { loading.value = false; }
}
onMounted(load);
const severityColor = (c: number) => ['#f5222d', '#fa8c16', '#faad14', '#1890ff'][c] || '#666';
</script>
<template>
  <div class="p-4">
    <a-page-header title="攻击图谱" sub-title="资产 / 漏洞 / 凭据关系可视化" />
    <a-card :bordered="false" :loading="loading">
      <div style="min-height: 400px; display: flex; flex-wrap: wrap; gap: 12px; padding: 20px;">
        <div v-for="node in graph.nodes" :key="node.id"
          style="padding: 10px 16px; border-radius: 8px; background: #f5f5f5; border: 2px solid #d9d9d9; cursor: pointer;"
          :style="{ borderColor: severityColor(node.category), minWidth: node.symbolSize > 30 ? '180px' : '140px' }">
          <div style="font-weight: 600; font-size: 13px;">{{ node.name }}</div>
        </div>
      </div>
      <a-empty v-if="!graph.nodes?.length && !loading" description="暂无攻击图谱数据，执行渗透任务后自动累积" />
    </a-card>
  </div>
</template>
