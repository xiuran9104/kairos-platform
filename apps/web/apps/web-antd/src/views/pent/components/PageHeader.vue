<script lang="ts" setup>
defineProps<{
  title: string;
  subtitle?: string;
}>();
</script>

<template>
  <div
    style="background:var(--ant-color-bg-container);border-radius:8px;margin-bottom:16px;padding:16px 24px;display:flex;justify-content:space-between;align-items:center"
  >
    <div>
      <h2 style="margin:0">{{ title }}</h2>
      <span
        v-if="subtitle"
        style="color:var(--ant-color-text-tertiary);font-size:13px"
      >{{ subtitle }}</span>
    </div>
    <slot name="extra" />
  </div>
</template>
