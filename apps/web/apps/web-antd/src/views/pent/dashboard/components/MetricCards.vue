<script lang="ts" setup>
import type { DashboardMetrics } from '../composables/useDashboard';

defineProps<{
  metrics: DashboardMetrics;
  sessions: number;
  running: number;
  pendingApprovals: number;
  loading: boolean;
}>();
</script>

<template>
  <a-row :gutter="[16, 16]">
    <a-col :xs="12" :sm="8" :lg="4" v-for="card in [
      { title: 'Agent Sessions', value: sessions, color: '#1677ff' },
      { title: '总任务', value: metrics.totalTasks, color: '#13c2c2' },
      { title: '漏洞总数', value: metrics.totalFindings, color: '#ff4d4f' },
      { title: '资产数量', value: metrics.totalAssets, color: '#722ed1' },
      { title: '工具调用量', value: metrics.totalToolCalls, color: '#fa8c16' },
      { title: 'LLM 调用量', value: metrics.totalLlmCalls, color: '#eb2f96' },
    ]" :key="card.title">
      <a-card :bordered="false" size="small" :loading="loading" class="metric-card">
        <a-statistic
          :title="card.title"
          :value="card.value"
          :value-style="{ color: card.color, fontSize: '28px' }"
        />
      </a-card>
    </a-col>
  </a-row>
</template>

<style scoped>
.metric-card { transition: box-shadow 0.2s; }
.metric-card:hover { box-shadow: 0 2px 8px rgba(0,0,0,0.09); }
</style>
