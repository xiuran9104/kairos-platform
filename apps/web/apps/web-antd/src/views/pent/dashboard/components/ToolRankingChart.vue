<script lang="ts" setup>
defineProps<{ tools: any[]; loading: boolean }>();

const barColors = ['#1677ff', '#13c2c2', '#fa8c16', '#eb2f96', '#722ed1', '#52c41a'];
</script>

<template>
  <a-card title="高频工具 TOP6" :bordered="false" size="small" :loading="loading">
    <a-empty v-if="!tools.length" description="暂无工具调用数据" :image="false" />
    <div v-else class="flex flex-col gap-2">
      <div
        v-for="(tool, idx) in tools.slice(0, 6)"
        :key="tool.name || idx"
        class="flex items-center gap-2"
      >
        <span class="text-sm truncate" style="width: 80px">{{ tool.name || tool.tool_id }}</span>
        <div class="flex-1 bg-gray-100 rounded" style="height: 16px">
          <div
            class="h-full rounded transition-all duration-300"
            :style="{
              width: `${tools.length ? ((tool.count || tool.call_count || 0) / Math.max(...tools.map((t: any) => t.count || t.call_count || 0), 1)) * 100 : 0}%`,
              backgroundColor: barColors[idx % barColors.length],
            }"
          />
        </div>
        <span class="text-sm font-medium" style="width: 32px; text-align: right">
          {{ tool.count || tool.call_count || 0 }}
        </span>
      </div>
    </div>
  </a-card>
</template>
