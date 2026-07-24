<script lang="ts" setup>
import type { ActivityItem } from '../composables/useDashboard';

defineProps<{ items: ActivityItem[]; loading: boolean }>();
</script>

<template>
  <a-card title="实时活动" :bordered="false" size="small" :loading="loading">
    <a-empty v-if="!items.length" description="暂无活动" :image="false" />
    <a-timeline v-else>
      <a-timeline-item v-for="(item, idx) in items" :key="idx" :color="item.color">
        <div class="text-sm">{{ item.text }}</div>
        <div class="text-gray-400 text-xs mt-1">
          {{ item.time ? new Date(item.time).toLocaleString() : '' }}
        </div>
      </a-timeline-item>
    </a-timeline>
  </a-card>
</template>
