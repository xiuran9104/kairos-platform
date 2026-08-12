<script lang="ts" setup>
import { computed, ref } from 'vue';

const props = defineProps<{ data: any; compact?: boolean }>();
const active = ref<'request' | 'response'>('response');

function parse(value: any) {
  if (typeof value !== 'string') return value || {};
  try { return JSON.parse(value); } catch { return value; }
}
function headerLines(headers: any) {
  const value = parse(headers);
  if (!value || typeof value !== 'object') return '';
  return Object.entries(value).map(([key, item]) => `${key}: ${item}`).join('\n');
}

const payload = computed(() => {
  const outer = parse(props.data) || {};
  return parse(outer.result) || outer;
});
const method = computed(() => payload.value.method || props.data?.method || 'HTTP');
const url = computed(() => payload.value.url || props.data?.url || payload.value.target || '-');
const status = computed(() => payload.value.response_status ?? payload.value.status_code ?? payload.value.status ?? props.data?.response_status);
const duration = computed(() => payload.value.duration_ms ?? payload.value.elapsed_ms ?? props.data?.duration_ms);
const requestRaw = computed(() => payload.value.raw_request || [
  `${method.value} ${url.value} HTTP/1.1`,
  headerLines(payload.value.request_headers || props.data?.request_headers),
  payload.value.request_body || props.data?.request_body || '',
].filter(Boolean).join('\n\n'));
const responseRaw = computed(() => payload.value.raw_response || [
  status.value ? `HTTP/1.1 ${status.value}` : '等待响应',
  headerLines(payload.value.response_headers || props.data?.response_headers),
  payload.value.response_body || props.data?.response_body || payload.value.body || '',
].filter(Boolean).join('\n\n'));
const statusColor = computed(() => !status.value ? 'default' : status.value >= 500 ? 'error' : status.value >= 400 ? 'warning' : status.value >= 300 ? 'processing' : 'success');
</script>

<template>
  <div class="http-card" :class="{ compact }">
    <div class="http-title">
      <a-tag color="blue">{{ method }}</a-tag>
      <strong :title="url">{{ url }}</strong>
      <a-tag v-if="status" :color="statusColor">{{ status }}</a-tag>
      <span v-if="duration != null">{{ duration }} ms</span>
    </div>
    <div class="http-tabs">
      <button :class="{ active: active === 'request' }" @click="active = 'request'">Request</button>
      <button :class="{ active: active === 'response' }" @click="active = 'response'">Response</button>
    </div>
    <pre>{{ active === 'request' ? requestRaw : responseRaw }}</pre>
  </div>
</template>

<style scoped>
.http-card { width:min(820px,80vw);overflow:hidden;background:#15191f;border:1px solid #303844;border-radius:8px;box-shadow:0 5px 18px rgb(0 0 0 / 12%); }
.http-title { display:flex;align-items:center;gap:8px;padding:9px 11px;color:#c9d2dc;background:#20262e;border-bottom:1px solid #303844; }
.http-title strong { flex:1;overflow:hidden;font-family:'JetBrains Mono','Cascadia Code',monospace;font-size:12px;text-overflow:ellipsis;white-space:nowrap; }
.http-title span { color:#8e9aa8;font-size:11px; }
.http-tabs { display:flex;gap:2px;padding:6px 8px 0;background:#15191f; }
.http-tabs button { padding:5px 10px;color:#84909e;font-size:11px;background:transparent;border:0;border-bottom:2px solid transparent;cursor:pointer; }
.http-tabs button.active { color:#f0a33a;border-bottom-color:#f0a33a; }
pre { max-height:420px;margin:0;padding:11px;overflow:auto;color:#d7dee7;font-family:'JetBrains Mono','Cascadia Code',monospace;font-size:11px;line-height:1.55;white-space:pre-wrap;word-break:break-all; }
.compact pre { max-height:280px; }
</style>
