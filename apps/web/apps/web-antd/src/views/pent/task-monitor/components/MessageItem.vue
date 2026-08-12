<script lang="ts" setup>
import { computed } from 'vue';
import { renderContent } from './renderContent';
import ToolMessageCard from './ToolMessageCard.vue';

defineOptions({ name: 'MessageItem' });

const props = defineProps<{
  message: any;
}>();

const emit = defineEmits<{
  (e: 'copy', text: string): void;
  (e: 'edit', message: any): void;
}>();

const timeStr = computed(() => {
  if (!props.message.createTime) return '';
  return new Date(props.message.createTime).toLocaleTimeString();
});

const isToolMsg = computed(() =>
  props.message.type === 'tool_calls' || props.message.type === 'tool_result' || props.message.role === 'tool',
);

const avatarIcon = computed(() => {
  if (props.message.type === 'error') return '⚠️';
  if (props.message.type === 'thinking') return '';
  if (isToolMsg.value) return '🔧';
  if (props.message.status === 'streaming') return '🔄';
  return '🤖';
});

const renderedContent = computed(() => {
  if (isToolMsg.value) {
    // Pretty-print JSON content, truncate very long results
    const raw = props.message.content || '';
    try {
      const parsed = JSON.parse(raw);
      if (parsed.tool_name && parsed.tool_call_id) {
        const short = { ...parsed };
        if (short.result) short.result = typeof short.result === 'string' && short.result.length > 2000
          ? short.result.slice(0, 2000) + '...[截断]' : short.result;
        return '<pre style="font-size:11px;max-height:300px;overflow:auto;margin:0">' +
          JSON.stringify(short, null, 2).replace(/</g, '&lt;') + '</pre>';
      }
      return '<pre style="font-size:11px;max-height:300px;overflow:auto;margin:0">' +
        JSON.stringify(parsed, null, 2).replace(/</g, '&lt;') + '</pre>';
    } catch {
      return '<pre style="font-size:11px;max-height:300px;overflow:auto;margin:0">' +
        (raw.length > 2000 ? raw.slice(0, 2000) + '...[截断]' : raw).replace(/</g, '&lt;') + '</pre>';
    }
  }
  return renderContent(props.message.content, props.message.type, props.message.role);
});

const assistantBubbleStyle = computed(() => ({
  padding: '10px 14px',
  borderRadius: '0 12px 12px 12px',
  fontSize: '13px',
  lineHeight: '1.6',
  whiteSpace: 'pre-wrap' as const,
  wordBreak: 'break-word' as const,
  background:
    props.message.type === 'error'
      ? 'var(--ant-color-error-bg)'
      : isToolMsg.value
        ? 'var(--ant-color-fill-secondary)'
        : 'var(--ant-color-bg-layout)',
  color:
    props.message.type === 'error'
      ? 'var(--ant-color-error)'
      : props.message.role === 'tool' && !props.message.success
        ? 'var(--ant-color-warning-text)'
        : 'var(--ant-color-text)',
  border: props.message.type === 'error' ? '1px solid #ffccc7'
    : isToolMsg.value ? '1px solid var(--ant-color-border-secondary)' : 'none',
}));
</script>

<template>
  <div
    :style="{
      marginBottom: '20px',
      display: 'flex',
      justifyContent: message.role === 'user' ? 'flex-end' : 'flex-start',
    }"
  >
    <!-- User message (right-aligned, blue bubble) -->
    <div
      v-if="message.role === 'user'"
      style="max-width:75%;display:flex;flex-direction:column;align-items:flex-end;gap:4px"
    >
      <div style="font-size:11px;color:#999">{{ timeStr }}</div>
      <div
        class="user-message-bubble"
      >
        {{ message.content }}
      </div>
    </div>

    <!-- Assistant / system / error (left-aligned) -->
    <div v-else style="max-width:75%;display:flex;flex-direction:row;gap:8px">
      <div
        style="width:32px;height:32px;border-radius:50%;background:var(--ant-color-fill-secondary);display:flex;align-items:center;justify-content:center;flex-shrink:0;font-size:16px"
      >
        {{ avatarIcon }}
      </div>
      <div style="display:flex;flex-direction:column;gap:4px">
        <div style="font-size:11px;color:#999">
          Pent · {{ timeStr }}
          <a-tag
            v-if="message.status === 'timeout'"
            color="orange"
            size="small"
            style="font-size:10px"
          >超时</a-tag>
          <a-tag
            v-if="message.status === 'failed'"
            color="red"
            size="small"
            style="font-size:10px"
          >失败</a-tag>
          <a-tag
            v-if="message.status === 'cancelled'"
            color="default"
            size="small"
            style="font-size:10px"
          >已中止</a-tag>
        </div>
        <ToolMessageCard v-if="isToolMsg" :message="message" />
        <div v-else-if="message.type === 'thinking'" :style="assistantBubbleStyle">
          <span class="thinking-dot">●</span> 思考中...
        </div>
        <div v-else :style="assistantBubbleStyle" v-dompurify-html="renderedContent" />
        <!-- Action buttons -->
        <div
          v-if="message.type !== 'error'"
          style="display:flex;gap:4px;margin-top:2px"
        >
          <a-button
            size="small"
            type="text"
            style="font-size:11px;padding:0 4px;color:#999"
            @click="emit('copy', message.content)"
          >复制</a-button>
          <a-button
            size="small"
            type="text"
            style="font-size:11px;padding:0 4px;color:#999"
            @click="emit('edit', message)"
          >编辑</a-button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.thinking-dot {
  display: inline-block;
  animation: pulse 1.2s ease-in-out infinite;
  color: var(--ant-color-primary);
}
@keyframes pulse {
  0%, 100% { opacity: 0.3; }
  50% { opacity: 1; }
}
.user-message-bubble {
  padding: 10px 16px;
  color: var(--ant-color-text);
  font-size: 13px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
  background: var(--ant-color-primary-bg);
  border: 1px solid var(--ant-color-primary-border);
  border-radius: 12px 12px 0 12px;
}
</style>
