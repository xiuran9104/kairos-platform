<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useAccessStore } from '@vben/stores';

interface ContentBlock {
  type: string;
  id: string;
  data: any;
  status: string;
}

interface ChatMessage {
  id: string;
  role: 'assistant' | 'user' | 'system';
  name?: string;
  status: string;
  datetime: string;
  content: ContentBlock[];
}

const props = defineProps<{
  agentType: 'scriptAgent' | 'productionAgent';
  projectId: number;
  scriptId?: number;
  messages: ChatMessage[];
  starterLabel?: string;
  starterPrompt?: string;
}>();

const emit = defineEmits<{
  (e: 'activity', payload: { status: string; toolName: string }): void;
  (e: 'clear-memory', memoryType: 'all' | 'message' | 'summary'): void;
  (e: 'workspace-preview', payload: { key: 'scriptPlan' | 'storyboardTable'; value: string }): void;
  (e: 'tool-result', payload: { toolName: string; result: any }): void;
}>();

const accessStore = useAccessStore();
const connected = ref(false);
const connecting = ref(false);
const thinkEnabled = ref(false);
const thinkLevel = ref(1);

let ws: WebSocket | null = null;
const completedToolCalls = new Set<string>();
const workspacePreviewValues = new Map<string, string>();

function emitWorkspacePreview(text: string) {
  for (const key of ['scriptPlan', 'storyboardTable'] as const) {
    const match = text.match(new RegExp(`<${key}>([\\s\\S]*?)<\\/${key}>`));
    const value = match?.[1]?.trim();
    if (value && workspacePreviewValues.get(key) !== value) {
      workspacePreviewValues.set(key, value);
      emit('workspace-preview', { key, value });
    }
  }
}

const isolationKey = computed(() => {
  const sid = props.agentType === 'productionAgent' ? (props.scriptId ?? 'none') : 'project';
  return `${props.agentType}:${props.projectId}:${sid}`;
});

function connect() {
  if (ws && ws.readyState === WebSocket.OPEN) return;
  if (ws && ws.readyState === WebSocket.CONNECTING) return;
  const token = accessStore.accessToken;
  if (!token) {
    console.warn('[AgentChat] No access token, skipping WebSocket connect');
    connecting.value = false;
    return;
  }
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  const params = new URLSearchParams({
    token: token ?? '',
    isolationKey: isolationKey.value,
    projectId: String(props.projectId),
  });
  if (props.scriptId) params.set('scriptId', String(props.scriptId));
  const url = `${proto}//${location.host}/api/socket/${props.agentType}?${params}`;

  ws = new WebSocket(url);

  ws.onopen = () => {
    connected.value = true;
    connecting.value = false;
  };

  ws.onmessage = (event) => {
    try {
      const frame = JSON.parse(event.data);
      handleServerMessage(frame);
    } catch { /* ignore invalid frames */ }
  };

  ws.onerror = () => {
    connecting.value = false;
    connected.value = false;
    setTimeout(() => { if (!connected.value && !connecting.value) connect(); }, 2000);
  };

  ws.onclose = () => {
    const was = connected.value;
    connected.value = false;
    connecting.value = false;
    if (was) setTimeout(() => { if (!connected.value && !connecting.value) connect(); }, 2000);
  };
}

function disconnect() {
  if (ws) {
    ws.close();
    ws = null;
  }
}

function send(content: string) {
  if (!ws || ws.readyState !== WebSocket.OPEN) return;
  ws.send(JSON.stringify({ type: 'chat', content }));
}

function stop() {
  if (!ws || ws.readyState !== WebSocket.OPEN) return;
  ws.send(JSON.stringify({ type: 'stop' }));
}

function updateThinkConfig(think: boolean, thinkLevel: number) {
  if (!ws || ws.readyState !== WebSocket.OPEN) return;
  ws.send(JSON.stringify({ type: 'updateThinkConfig', think, thinkLevel }));
}
function applyThinkConfig() {
  updateThinkConfig(thinkEnabled.value, thinkLevel.value);
}

function handleServerMessage(frame: any) {
  const { event: type, data } = frame;
  if (!data) return;

  switch (type) {
    case 'message': {
      const msg: ChatMessage = {
        id: data.id,
        role: data.role || 'assistant',
        name: data.name,
        status: data.status || 'pending',
        datetime: data.datetime || new Date().toISOString(),
        content: [],
      };
      props.messages.push(msg);
      scrollToBottom();
      break;
    }
    case 'message:update': {
      const msg = props.messages.find((m) => m.id === data.id);
      if (msg) {
        msg.status = data.status;
        if (data.ext) Object.assign(msg, { ext: data.ext });
      }
      emit('activity', { status: data.status || 'pending', toolName: '__agent__' });
      break;
    }
    case 'content:add': {
      const msg = props.messages.find((m) => m.id === data.messageId);
      if (msg) {
        msg.content.push(data.content);
        if (data.content?.type === 'toolcall') {
          emit('activity', {
            status: data.content.status || 'pending',
            toolName: data.content.data?.toolCallName || 'unknown',
          });
        }
        scrollToBottom();
      }
      break;
    }
    case 'content:update': {
      const msg = props.messages.find((m) => m.id === data.messageId);
      if (msg) {
        const block = msg.content.find((c) => c.id === data.contentId);
        if (block) {
          block.status = data.status || block.status;
          if (data.strategy === 'append' && data.data !== undefined) {
            if (block.type === 'text' || block.type === 'markdown') {
              block.data = (block.data || '') + (typeof data.data === 'string' ? data.data : '');
              emitWorkspacePreview(String(block.data || ''));
            } else if (block.type === 'thinking') {
              if (typeof data.data === 'object' && data.data.text) {
                block.data.text = (block.data.text || '') + data.data.text;
              } else if (block.data.title && data.data?.title) {
                block.data.title = data.data.title;
              }
            } else if (block.type === 'toolcall') {
              if (data.data.toolCallId) block.data.toolCallId = data.data.toolCallId;
              if (data.data.toolCallName) block.data.toolCallName = data.data.toolCallName;
              if (data.data.args) block.data.args = (block.data.args || '') + data.data.args;
              if (data.data.chunk) block.data.result = (block.data.result || '') + data.data.chunk;
            }
          } else if (data.strategy === 'merge' && data.data !== undefined) {
            if (typeof data.data === 'object') {
              Object.assign(block.data, data.data);
            } else {
              block.data = data.data;
            }
          }
          if (
            block.type === 'toolcall' &&
            (data.status === 'complete' || data.status === 'error') &&
            !completedToolCalls.has(block.id)
          ) {
            completedToolCalls.add(block.id);
            const toolName = block.data.toolCallName || 'unknown';
            emit('activity', { status: data.status, toolName });
            emit('tool-result', {
              toolName,
              result: data.data?.result || block.data.result || block.data,
            });
          }
        }
      }
      break;
    }
  }
}

const chatContainer = ref<HTMLElement | null>(null);

function scrollToBottom() {
  nextTick(() => {
    if (chatContainer.value) {
      chatContainer.value.scrollTop = chatContainer.value.scrollHeight;
    }
  });
}

onBeforeUnmount(() => {
  disconnect();
});

function toolCallStatusIcon(status: string) {
  if (status === 'streaming') return '⟳';
  if (status === 'complete') return '✓';
  if (status === 'error') return '✗';
  return '…';
}

function toolCallStatusColor(status: string) {
  if (status === 'complete') return '#52c41a';
  if (status === 'error') return '#ff4d4f';
  return '#1677ff';
}

const inputText = ref('');

const hasActiveRun = computed(() =>
  props.messages.some((m) => m.status === 'pending' || m.status === 'streaming'),
);

function handleKeydown(e: KeyboardEvent) {
  if (e.key !== 'Enter') return;
  if (e.ctrlKey || e.metaKey || e.shiftKey) {
    // Ctrl+Enter / Shift+Enter = newline
    return;
  }
  // Plain Enter = send
  e.preventDefault();
  e.stopPropagation();
  handleSend();
}

function handleSend() {
  const text = inputText.value.trim();
  if (!text) return;
  if (!connected.value) {
    connect();
    return;
  }
  inputText.value = '';
  send(text);
}

function handleStarter() {
  if (!props.starterPrompt) return;
  inputText.value = props.starterPrompt;
  handleSend();
}

// Auto-connect on mount and when agentType/projectId/scriptId changes
onMounted(() => {
  connect();
});

watch(
  () => [props.agentType, props.projectId, props.scriptId],
  () => {
    disconnect();
    connect();
  },
);

defineExpose({ connect, disconnect, send, stop, updateThinkConfig, connected });
</script>

<template>
  <div class="agent-chat-wrapper">
    <!-- Connection status -->
    <div class="connection-status" :class="{ connected, connecting }">
      <span class="status-dot" />
      <span class="status-text">
        {{ connecting ? '连接中...' : connected ? '已连接' : '未连接' }}
      </span>
      <div class="agent-controls">
        <label><a-switch v-model:checked="thinkEnabled" size="small" @change="applyThinkConfig" /> 深度思考</label>
        <a-select v-model:value="thinkLevel" size="small" :disabled="!thinkEnabled" style="width: 88px" @change="applyThinkConfig">
          <a-select-option :value="0">快速</a-select-option><a-select-option :value="1">基础</a-select-option><a-select-option :value="2">深入</a-select-option><a-select-option :value="3">完整</a-select-option>
        </a-select>
        <a-dropdown>
          <a-button size="small">记忆</a-button>
          <template #overlay><a-menu>
            <a-menu-item @click="emit('clear-memory', 'message')">清除消息记忆</a-menu-item>
            <a-menu-item @click="emit('clear-memory', 'summary')">清除摘要记忆</a-menu-item>
            <a-menu-item danger @click="emit('clear-memory', 'all')">清除全部记忆</a-menu-item>
          </a-menu></template>
        </a-dropdown>
      </div>
    </div>

    <!-- Messages area -->
    <div ref="chatContainer" class="chat-messages">
      <div v-if="messages.length === 0" class="chat-empty">
        <div>让 Agent 读取当前剧本和资产并启动制作流程</div>
        <a-button
          v-if="starterPrompt"
          class="starter-button"
          type="primary"
          :disabled="!connected"
          @click="handleStarter"
        >{{ starterLabel || '开始制作' }}</a-button>
      </div>

      <div
        v-for="msg in messages"
        :key="msg.id"
        class="chat-message"
        :class="[`chat-message-${msg.role.replace('assistant', 'agent')}`]"
      >
        <!-- Message header -->
        <div class="message-header">
          <span class="message-role">
            {{ msg.role.startsWith('user') ? '你' : (msg.name || 'Agent') }}
          </span>
          <span class="message-time">{{ new Date(msg.datetime).toLocaleTimeString() }}</span>
        </div>

        <!-- Content blocks -->
        <div v-for="block in msg.content" :key="block.id" class="content-block">
          <!-- Text content -->
          <div v-if="block.type === 'text' || block.type === 'markdown'" class="content-text">
            <div class="markdown-body" v-dompurify-html="block.data" />
            <span v-if="block.status === 'streaming'" class="streaming-cursor">▊</span>
          </div>

          <!-- Thinking content -->
          <details
            v-else-if="block.type === 'thinking'"
            :id="block.id"
            class="content-thinking"
            :open="block.status !== 'complete'"
          >
            <summary class="thinking-title">
              {{ block.data?.title || '思考中...' }}
            </summary>
            <div class="thinking-body">{{ block.data?.text }}</div>
          </details>

          <!-- Tool call content -->
          <details
            v-else-if="block.type === 'toolcall'"
            class="content-toolcall"
            :open="block.status === 'streaming'"
          >
            <summary class="toolcall-title">
              <span
                class="toolcall-status-icon"
                :style="{ color: toolCallStatusColor(block.status) }"
              >
                {{ toolCallStatusIcon(block.status) }}
              </span>
              工具调用：{{ block.data?.toolCallName || 'unknown' }}
            </summary>
            <div class="toolcall-body">
              <div v-if="block.data?.args" class="toolcall-section">
                <strong>参数：</strong>
                <pre>{{ block.data.args }}</pre>
              </div>
              <div v-if="block.data?.result" class="toolcall-section">
                <strong>结果：</strong>
                <pre>{{ block.data.result }}</pre>
              </div>
            </div>
          </details>
        </div>

        <!-- Status indicator -->
        <div v-if="msg.status === 'pending' || msg.status === 'streaming'" class="message-loading">
          <span class="loading-dots">...</span>
        </div>
        <div v-else-if="msg.status === 'error'" class="message-error">
          {{ (msg as any).ext?.error || '执行出错' }}
        </div>
      </div>
    </div>

    <!-- Input area -->
    <div class="chat-input-area">
      <div class="input-wrapper">
        <a-textarea
          v-model:value="inputText"
          :auto-size="{ minRows: 2, maxRows: 6 }"
          placeholder="输入消息，Enter 发送，Ctrl+Enter 换行"
          @keydown="handleKeydown"
        />
        <a-button
          v-if="hasActiveRun"
          class="send-btn stop-btn"
          danger
          @click="stop"
        >■</a-button>
        <a-button
          v-else
          class="send-btn"
          type="primary"
          :disabled="!inputText.trim()"
          @click="handleSend"
        >▶</a-button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.agent-chat-wrapper {
  display: flex;
  flex-direction: column;
  height: 680px;
  border: 1px solid var(--ant-color-border-secondary);
  border-radius: 8px;
  overflow: hidden;
  background: var(--ant-color-fill-quaternary);
}

.connection-status {
  padding: 4px 12px;
  font-size: 12px;
  display: flex;
  align-items: center;
  gap: 6px;
  background: var(--ant-color-bg-container);
  border-bottom: 1px solid var(--ant-color-border-secondary);
}
.connection-status.connected .status-dot { background: #52c41a; }
.connection-status.connecting .status-dot { background: #faad14; }
.status-dot {
  width: 8px; height: 8px;
  border-radius: 50%;
  background: var(--ant-color-fill-secondary);
}
.status-text { color: var(--ant-color-text-tertiary); }
.agent-controls { display: flex; align-items: center; gap: 6px; margin-left: auto; }
.agent-controls label { display: flex; align-items: center; gap: 4px; white-space: nowrap; }

.chat-messages {
  flex: 1;
  overflow-y: auto;
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.chat-empty {
  text-align: center;
  color: var(--ant-color-text-quaternary);
  padding: 60px 0;
  font-size: 14px;
}
.starter-button { margin-top: 12px; }

.chat-message {
  max-width: 85%;
  padding: 10px 14px;
  border-radius: 10px;
  font-size: 14px;
  line-height: 1.6;
}

.chat-message-user {
  align-self: flex-end;
  background: var(--ant-color-primary);
  color: var(--ant-color-bg-container);
}
.chat-message-user .message-header { color: rgba(255,255,255,0.7); }
.chat-message-user .message-time { color: rgba(255,255,255,0.5); }

.chat-message-agent {
  align-self: flex-start;
  background: var(--ant-color-bg-container);
  border: 1px solid #e8e8e8;
}

.message-header {
  display: flex;
  justify-content: space-between;
  margin-bottom: 6px;
  font-size: 12px;
}
.message-role { font-weight: 600; }
.message-time { color: var(--ant-color-text-quaternary); }

.content-block { margin-top: 4px; }

.content-text {
  word-break: break-word;
}
.streaming-cursor {
  animation: blink 1s infinite;
  color: var(--ant-color-primary);
}
@keyframes blink { 50% { opacity: 0; } }

.content-thinking {
  background: #f6f8fa;
  border-radius: 6px;
  padding: 8px;
  margin: 4px 0;
}
.thinking-title { font-size: 12px; color: var(--ant-color-text-tertiary); cursor: pointer; }
.thinking-body {
  margin-top: 6px;
  font-size: 13px;
  color: #595959;
  white-space: pre-wrap;
  max-height: 200px;
  overflow-y: auto;
}

.content-toolcall {
  background: var(--ant-color-warning-bg);
  border: 1px solid #ffe58f;
  border-radius: 6px;
  padding: 8px;
  margin: 4px 0;
}
.toolcall-title {
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 4px;
}
.toolcall-status-icon { font-size: 14px; }
.toolcall-body { margin-top: 6px; }
.toolcall-section {
  margin-bottom: 4px;
  font-size: 12px;
  color: #595959;
}
.toolcall-section pre {
  background: var(--ant-color-bg-container);
  padding: 6px;
  border-radius: 4px;
  font-size: 11px;
  max-height: 120px;
  overflow: auto;
  margin: 2px 0;
}

.message-loading { color: var(--ant-color-primary); font-size: 12px; }
.message-error { color: #ff4d4f; font-size: 12px; }

.chat-input-area {
  padding: 10px 12px;
  background: var(--ant-color-bg-container);
  border-top: 1px solid var(--ant-color-border-secondary);
}
.input-wrapper {
  position: relative;
}
.send-btn {
  position: absolute;
  right: 8px;
  bottom: 8px;
  width: 32px;
  height: 32px;
  min-width: 32px;
  padding: 0;
  border-radius: 6px;
  font-size: 14px;
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1;
}
.stop-btn {
  animation: pulse-stop 1.5s infinite;
}
@keyframes pulse-stop {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.6; }
}
.input-wrapper :deep(.ant-input) {
  padding-right: 48px;
}
</style>
