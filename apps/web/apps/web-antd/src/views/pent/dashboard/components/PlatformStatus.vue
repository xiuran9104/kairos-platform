<script lang="ts" setup>
defineProps<{
  tasks: number;
  runs: number;
  activeSessions: number;
  pendingApprovals: number;
  toolTotal: number;
  toolHealthy: number;
  toolSuccessRate: number | null;
  loading: boolean;
}>();
</script>

<template>
  <a-card title="平台状态" :bordered="false" size="small" :loading="loading">
    <a-descriptions :column="1" size="small" bordered>
      <a-descriptions-item label="任务总数">{{ tasks }}</a-descriptions-item>
      <a-descriptions-item label="Run 总数">{{ runs }}</a-descriptions-item>
      <a-descriptions-item label="活跃 Session">{{ activeSessions }}</a-descriptions-item>
      <a-descriptions-item label="待审批">{{ pendingApprovals }}</a-descriptions-item>
      <a-descriptions-item label="已注册工具">{{ toolTotal || '-' }}</a-descriptions-item>
      <a-descriptions-item label="健康工具">{{ toolHealthy || '-' }}</a-descriptions-item>
      <a-descriptions-item label="工具成功率">
        {{ toolSuccessRate != null ? `${(toolSuccessRate * 100).toFixed(1)}%` : '-' }}
      </a-descriptions-item>
      <a-descriptions-item label="执行模式">授权范围内自主执行</a-descriptions-item>
    </a-descriptions>
    <a-alert
      class="mt-3"
      type="info"
      show-icon
      message="Agent 不依赖浏览器持续在线，重新进入 Session 后会恢复消息和运行事件。"
      :banner="false"
    />
  </a-card>
</template>
