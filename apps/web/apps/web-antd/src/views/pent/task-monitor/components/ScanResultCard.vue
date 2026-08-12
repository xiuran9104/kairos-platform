<script lang="ts" setup>
import { computed } from 'vue';
const props = defineProps<{ data: any; toolName: string }>();
function parse(value: any): any { if (typeof value !== 'string') return value || {}; try { return JSON.parse(value); } catch { return value; } }
const result = computed(() => { const outer = parse(props.data); return parse(outer?.result) || outer; });
const findings = computed(() => result.value?.ports || result.value?.open_ports || result.value?.findings || result.value?.results || []);
const detail = computed(() => typeof result.value === 'string' ? result.value : JSON.stringify(result.value, null, 2));
</script>
<template>
  <div class="scan-card">
    <div class="scan-head"><span>⌁</span><strong>{{ toolName }}</strong><a-tag color="processing">扫描结果</a-tag></div>
    <div v-if="Array.isArray(findings) && findings.length" class="scan-grid">
      <div v-for="(item,index) in findings.slice(0,20)" :key="index" class="scan-row">
        <strong>{{ item.port || item.name || item.template || `#${index + 1}` }}</strong>
        <span>{{ item.service || item.status || item.severity || item.url || '' }}</span>
      </div>
    </div>
    <details><summary>查看完整扫描输出</summary><pre>{{ detail }}</pre></details>
  </div>
</template>
<style scoped>
.scan-card { width:min(760px,78vw);padding:11px;background:var(--ant-color-fill-quaternary);border:1px solid var(--ant-color-border-secondary);border-left:3px solid #1677ff;border-radius:8px; }
.scan-head { display:flex;align-items:center;gap:8px; }.scan-head strong{flex:1}.scan-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:6px;margin:9px 0}.scan-row{display:flex;justify-content:space-between;gap:8px;padding:6px 8px;background:var(--ant-color-bg-container);border-radius:5px}.scan-row span{color:var(--ant-color-text-secondary);font-size:11px}summary{width:fit-content;color:var(--ant-color-primary);font-size:12px;cursor:pointer}pre{max-height:360px;overflow:auto;color:var(--ant-color-text);font-size:11px;white-space:pre-wrap}
</style>
