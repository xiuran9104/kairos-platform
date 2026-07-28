<script lang="ts" setup>
defineProps<{
  queued: number;
  running: number;
  completed: number;
  failed: number;
  loading: boolean;
}>();
</script>

<template>
  <a-row :gutter="[16, 16]" class="mt-4">
    <a-col :xs="12" :md="6" v-for="stat in [
      { title: '排队中', value: queued, color: 'var(--ant-color-text-tertiary)' },
      { title: '运行中', value: running, color: 'var(--ant-color-primary)' },
      { title: '已完成', value: completed, color: 'var(--ant-color-success)' },
      { title: '失败', value: failed, color: 'var(--ant-color-error)' },
    ]" :key="stat.title">
      <a-card :bordered="false" size="small" :loading="loading">
        <a-statistic
          :title="stat.title"
          :value="stat.value"
          :value-style="{ color: stat.color, fontSize: '28px' }"
        />
      </a-card>
    </a-col>
  </a-row>
</template>
