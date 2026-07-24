<script lang="ts" setup>
import type { FindingSeverity } from '../composables/useDashboard';

const props = defineProps<{ severity: FindingSeverity; loading: boolean }>();

const SEVERITIES = [
  { key: 'critical', label: '严重', color: '#5c0011' },
  { key: 'high', label: '高危', color: '#ff4d4f' },
  { key: 'medium', label: '中危', color: '#fa8c16' },
  { key: 'low', label: '低危', color: '#faad14' },
  { key: 'info', label: '信息', color: '#1677ff' },
] as const;

function maxVal() {
  return Math.max(...Object.values(props.severity), 1);
}
</script>

<template>
  <a-card title="漏洞等级分布" :bordered="false" size="small" :loading="loading">
    <a-empty v-if="maxVal() === 0" description="暂无漏洞数据" :image="false" />
    <div v-else class="flex flex-col gap-2">
      <div v-for="s in SEVERITIES" :key="s.key" class="flex items-center gap-2">
        <span class="text-sm" style="width: 40px">{{ s.label }}</span>
        <div class="flex-1 bg-gray-100 rounded" style="height: 18px">
          <div
            class="h-full rounded transition-all duration-300"
            :style="{ width: `${(severity[s.key] / maxVal()) * 100}%`, backgroundColor: s.color }"
          />
        </div>
        <span class="text-sm font-medium" style="width: 28px; color: s.color">
          {{ severity[s.key] }}
        </span>
      </div>
    </div>
  </a-card>
</template>
