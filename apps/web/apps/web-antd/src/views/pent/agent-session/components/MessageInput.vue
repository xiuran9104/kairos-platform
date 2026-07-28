<script lang="ts" setup>
import { computed, ref } from 'vue';

defineOptions({ name: 'MessageInput' });

const props = defineProps<{
  modelValue: string;
  loading: boolean;
  cancelling?: boolean;
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'send'): void;
  (e: 'stop'): void;
}>();

const isComposing = ref(false);

const textValue = computed({
  get: () => props.modelValue,
  set: (val: string) => emit('update:modelValue', val),
});

function handleSend() {
  if (!props.modelValue.trim() || props.loading) return;
  emit('send');
}

function onKeydown(e: KeyboardEvent) {
  if (isComposing.value) return;
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    handleSend();
  }
}

function onCompositionstart() {
  isComposing.value = true;
}

function onCompositionend() {
  setTimeout(() => {
    isComposing.value = false;
  }, 200);
}
</script>

<template>
  <div
    style="border-top:1px solid var(--ant-color-border-secondary);padding:12px 20px 16px"
  >
    <!-- Loading / cancel bar -->
    <div
      v-if="loading"
      style="display:flex;align-items:center;gap:8px;margin-bottom:8px;color:#666;font-size:13px"
    >
      <a-spin size="small" />
      <span>{{ cancelling ? '中止中…' : '处理中…' }}</span>
      <a-button size="small" danger @click="emit('stop')" :loading="cancelling">中止</a-button>
    </div>
    <div style="display:flex;align-items:flex-end;gap:8px">
      <a-textarea
        v-model:value="textValue"
        :auto-size="{ minRows: 1, maxRows: 5 }"
        placeholder="输入测试目标或指令… (Enter 发送，Shift+Enter 换行)"
        :disabled="loading"
        @keydown="onKeydown"
        @compositionstart="onCompositionstart"
        @compositionend="onCompositionend"
        style="flex:1;border-radius:8px"
      />
      <a-button
        type="primary"
        @click="handleSend"
        :loading="loading"
        style="border-radius:8px;height:36px"
      >
        {{ loading ? '处理中' : '发送' }}
      </a-button>
    </div>
  </div>
</template>
