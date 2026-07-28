<script lang="ts" setup>
import { computed } from 'vue';
import { renderContent } from './renderContent';

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

const avatarIcon = computed(() => {
  if (props.message.type === 'error') return '⚠️';
  if (props.message.status === 'streaming') return '🔄';
  return '🤖';
});

const renderedContent = computed(() =>
  renderContent(props.message.content, props.message.type, props.message.role),
);

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
      : 'var(--ant-color-bg-layout)',
  color:
    props.message.type === 'error'
      ? 'var(--ant-color-error)'
      : 'var(--ant-color-text)',
  border: props.message.type === 'error' ? '1px solid #ffccc7' : 'none',
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
        style="padding:10px 16px;border-radius:12px 12px 0 12px;background:var(--ant-color-primary);color:#fff;font-size:13px;line-height:1.6;white-space:pre-wrap;word-break:break-word"
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
        <div :style="assistantBubbleStyle" v-dompurify-html="renderedContent" />
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
