<script lang="ts" setup>
import { computed } from 'vue';
import HttpExchangeCard from './HttpExchangeCard.vue';
import ScanResultCard from './ScanResultCard.vue';

const props = defineProps<{ message: any }>();

function parse(value: unknown): any {
  if (typeof value !== 'string') return value;
  try { return JSON.parse(value); } catch { return value; }
}

const outer = computed(() => parse(props.message.content) || {});
const result = computed(() => parse(outer.value.result));
const isCall = computed(() => props.message.type === 'tool_calls');
const toolName = computed(() => {
  if (isCall.value) {
    return (outer.value.tool_calls || [])
      .map((call: any) => call.function?.name || call.name)
      .filter(Boolean)
      .join('、') || '工具调用';
  }
  return outer.value.tool_name || outer.value.name || '工具结果';
});
const success = computed(() => isCall.value ? undefined : outer.value.success !== false && !result.value?.error);
const httpTools = ['http_request', 'http_probe', 'curl', 'web_request'];
const scanTools = ['nmap', 'port_scan', 'dir_fuzz', 'nuclei', 'nikto', 'sqlmap', 'commix', 'js_analyze', 'js_collect'];
const isHttp = computed(() => !isCall.value && httpTools.includes(toolName.value));
const isScan = computed(() => !isCall.value && scanTools.includes(toolName.value));
const summary = computed(() => {
  if (isCall.value) return outer.value.content || '模型决定执行工具';
  if (result.value?.error) return String(result.value.error);
  if (result.value?.status_code) return `HTTP ${result.value.status_code}`;
  return success.value ? '执行成功，展开查看返回内容' : '执行失败，展开查看错误';
});
const detail = computed(() => {
  const value = isCall.value ? outer.value : result.value;
  return typeof value === 'string' ? value : JSON.stringify(value, null, 2);
});
</script>

<template>
  <HttpExchangeCard v-if="isHttp" :data="outer" compact />
  <ScanResultCard v-else-if="isScan" :data="outer" :tool-name="toolName" />
  <div v-else class="tool-message" :class="{ failed: success === false }">
    <div class="tool-header">
      <span class="tool-icon">🔧</span>
      <strong>{{ toolName }}</strong>
      <span class="tool-type">{{ isCall ? '工具调用' : '工具结果' }}</span>
      <a-tag v-if="success === true" color="success">成功</a-tag>
      <a-tag v-else-if="success === false" color="error">失败</a-tag>
    </div>
    <div class="tool-summary">{{ summary }}</div>
    <details>
      <summary>查看完整内容</summary>
      <pre>{{ detail }}</pre>
    </details>
  </div>
</template>

<style scoped>
.tool-message { width:min(760px,78vw);padding:10px 12px;background:var(--ant-color-fill-quaternary);border:1px solid var(--ant-color-border-secondary);border-left:3px solid var(--ant-color-primary);border-radius:8px; }
.tool-message.failed { border-left-color:var(--ant-color-error); }
.tool-header { display:flex;align-items:center;gap:8px; }
.tool-header strong { min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap; }
.tool-icon { font-size:16px; }
.tool-type { flex:1;color:var(--ant-color-text-tertiary);font-size:11px; }
.tool-summary { margin:7px 0;color:var(--ant-color-text-secondary);font-size:12px; }
summary { width:fit-content;color:var(--ant-color-primary);font-size:12px;cursor:pointer; }
pre { max-height:360px;margin:8px 0 0;padding:10px;overflow:auto;color:var(--ant-color-text);font-size:11px;white-space:pre-wrap;background:var(--ant-color-bg-container);border-radius:6px; }
</style>
