<script lang="ts" setup>
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { message } from 'ant-design-vue';
import { cancelTurn, createTurn, createTurnStream, listTurns } from '#/api/pent/conversation';
import {
  createPentSession, deletePentSession, getPentSession,
  getPentSessions, getPentSessionMessages, streamPentRunEvents,
} from '#/api/pent';
import { createClientRequestId } from '#/utils/client-request-id';
import SessionSidebar from './components/SessionSidebar.vue';
import MessageItem from './components/MessageItem.vue';
import MessageInput from './components/MessageInput.vue';

defineOptions({ name: 'PentAgentSession' });

const route = useRoute();
const router = useRouter();
const sessionId = computed(() => route.params.id as string | undefined);

// ── Sidebar: session list ──────────────────────────────────────────
const sessions = ref<any[]>([]);
const sessionsLoading = ref(false);
const sidebarCollapsed = ref(false);
const chatBox = ref<HTMLElement>();

async function loadSessions() {
  sessionsLoading.value = true;
  try { sessions.value = (await getPentSessions()) as any[]; }
  catch { /* silent */ }
  finally { sessionsLoading.value = false; }
}

// ── Active session + messages ─────────────────────────────────────
const session = ref<any>(null);
const messages = ref<any[]>([]);
const initializing = ref(true);

async function initSession() {
  initializing.value = true;
  try {
    if (sessionId.value) {
      session.value = await getPentSession(sessionId.value);
      const msgs: any = await getPentSessionMessages(sessionId.value);
      const msgList = Array.isArray(msgs) ? msgs : (msgs?.data ?? []);
      if (msgList.length > 0) {
        messages.value = msgList
          .filter((m: any) => {
            const type = m.format || m.message_type || 'text';
            return m.role !== 'tool' && type !== 'tool_result' && type !== 'tool_calls';
          })
          .map((m: any) => ({
            id: m.id,
            role: m.role,
            type: m.format || m.message_type || 'text',
            content: m.content,
            payload: m.payload,
            createTime: m.created_at,
            status: m.status || 'completed',
          }));
      } else {
        messages.value = [];
      }
    }
  } catch {
    messages.value = [{
      role: 'assistant', type: 'error',
      content: '会话初始化失败',
      status: 'failed',
    }];
  } finally { initializing.value = false; }
}

onMounted(async () => {
  await loadSessions();
  if (!sessionId.value && sessions.value.length > 0) {
    router.replace(`/pent/sessions/${sessions.value[0].id}`);
    return;
  }
  await initSession();
});

watch(sessionId, async () => { if (sessionId.value) await initSession(); });

// ── Session management ────────────────────────────────────────────
async function deleteSession(s: any) {
  try {
    await deletePentSession(s.id);
    message.success('已删除');
    if (sessionId.value === s.id) {
      session.value = null;
      messages.value = [];
      router.replace('/pent/agent-session');
    }
    await loadSessions();
  } catch { message.error('删除失败'); }
}

function selectSession(s: any) { router.push(`/pent/sessions/${s.id}`); }

async function handleCreateSession() {
  try {
    const s: any = await createPentSession({
      objective: '新安全测试会话', targets: [], constraints: {}, budget: {},
    });
    await loadSessions();
    const id = s?.id ?? s?.data?.id;
    if (id) { router.push(`/pent/sessions/${id}`); }
  } catch { message.error('创建失败'); }
}

// ── Send message (SSE streaming + AbortController) ──────────────────
const inputText = ref('');
const loading = ref(false);
const cancelling = ref(false);
const abortController = ref<AbortController>();
const activeTurnId = ref<string>();

async function sendMessage() {
  const text = inputText.value.trim();
  if (!text || loading.value) return;
  let currentSessionId = sessionId.value;

  // Auto-create session
  if (!currentSessionId) {
    try {
      loading.value = true;
      const s: any = await createPentSession({
        objective: text.slice(0, 50), targets: [], constraints: {}, budget: {},
      });
      const newId = s?.id ?? s?.data?.id;
      if (newId) {
        currentSessionId = newId;
        await loadSessions();
        await router.replace(`/pent/sessions/${newId}`);
      }
    } catch {
      message.error('创建会话失败');
      loading.value = false;
      return;
    }
  }
  if (!currentSessionId) { loading.value = false; return; }

  messages.value.push({
    role: 'user', type: 'text', content: text,
    createTime: new Date().toISOString(),
  });
  inputText.value = '';
  loading.value = true;
  cancelling.value = false;
  await nextTick(); scrollToBottom(true);

  try {
    abortController.value = new AbortController();
    const hasActiveRun = !!session.value?.active_run_id;
    if (hasActiveRun) {
      await sendPollingMessage(currentSessionId, text);
    } else {
      await sendStreamingMessage(currentSessionId, text);
    }
  } catch (e: any) {
    if (!cancelling.value) {
      messages.value.push({
        role: 'assistant', type: 'error',
        content: `发送失败：${e?.message || '未知错误'}`, status: 'failed',
      });
    } else {
      messages.value.push({
        role: 'assistant', type: 'text',
        content: '⏹ 已中止', status: 'cancelled',
      });
    }
  }
  activeTurnId.value = undefined;
  cancelling.value = false;
  loading.value = false;
  abortController.value = undefined;
  await nextTick(); scrollToBottom(true);
}

async function sendPollingMessage(currentSessionId: string, text: string) {
  const resp: any = await createTurn(currentSessionId, {
    client_request_id: createClientRequestId(), content: text,
  });
  const turnId = resp?.turn_id ?? resp?.data?.turn_id;
  if (!turnId) throw new Error('no turn_id');
  activeTurnId.value = turnId;

  // Poll for turn completion (SSE-backed via fetchEventSource with Authorization header, falls back to polling)
  const seenIds = new Set<string>();
  const runId = session.value?.active_run_id;
  const sseEvents: any[] = [];
  let sseActive = false;
  const sseAbort = new AbortController();
  if (runId) {
    try {
      streamPentRunEvents(
        runId,
        sseAbort,
        (event: any) => { sseEvents.push(event); sseActive = true; },
        () => { sseActive = false; },
        () => { sseActive = false; },
      );
    } catch { /* fall back to polling */ }
  }

  let attempts = 0;
  const MAX_POLL = 3600;
  while (attempts < MAX_POLL) {
    if (abortController.value?.signal.aborted) { sseAbort.abort(); break; }

    if (sseActive && sseEvents.length > 0) {
      sseEvents.length = 0;
    } else if (sseActive) {
      await new Promise<void>(r => setTimeout(r, 1000));
    } else {
      await new Promise<void>(r => setTimeout(r, 2000));
    }

    // Check for cancellation
    if (abortController.value?.signal.aborted) break;

    const turns: any = await listTurns(currentSessionId);
    const turnList = Array.isArray(turns) ? turns : (turns?.data ?? []);
    const turn = turnList.find((t: any) => t.id === turnId);
    if (turn?.status === 'failed') {
      messages.value.push({
        role: 'assistant', type: 'error',
        content: turn.error_message || '处理失败',
        status: 'failed',
      });
      break;
    }
    if (turn?.status === 'cancelled') {
      messages.value.push({
        role: 'assistant', type: 'text',
        content: '⏹ 已中止', status: 'cancelled',
      });
      break;
    }

    // Process new messages
    if (turn?.messages && Array.isArray(turn.messages)) {
      for (const m of turn.messages) {
        if (m.role === 'user') continue;
        const msgId = `${m.role}_${m.sequence}_${(m.content || '').slice(0, 40)}`;
        if (seenIds.has(msgId)) continue;
        seenIds.add(msgId);
        const msgType = m.format || m.message_type || 'text';
        if (m.role === 'tool' || msgType === 'tool_result') continue;
        if (msgType === 'tool_calls') continue;
        if (m.content?.trim()) {
          // Update existing streaming message or add new
          const lastMsg = messages.value[messages.value.length - 1];
          if (lastMsg?.status === 'streaming' && lastMsg?.role === 'assistant') {
            lastMsg.content = m.content;
          } else {
            messages.value.push({
              id: m.id,
              role: m.role,
              type: msgType,
              content: m.content,
              createTime: new Date().toISOString(),
              status: 'streaming',
            });
          }
        }
      }
    }

    if (turn?.status === 'completed') {
      const last = messages.value[messages.value.length - 1];
      if (last?.status === 'streaming') last.status = 'completed';
      break;
    }
    attempts++;
  }

  if (attempts >= MAX_POLL) {
    messages.value.push({
      role: 'assistant', type: 'text',
      content: '⏱ 已达到最长处理时间', status: 'timeout',
    });
  }
  sseAbort.abort();
}

async function sendStreamingMessage(currentSessionId: string, text: string) {
  return new Promise<void>((resolve, reject) => {
    let assistantMsg: any = null;
    createTurnStream(
      currentSessionId,
      text,
      createClientRequestId(),
      abortController.value!,
      (event: any) => {
        try {
          const data = JSON.parse(event.data || '{}');
          if (data.type === 'delta') {
            if (!assistantMsg) {
              assistantMsg = {
                id: data.message_id,
                role: 'assistant',
                type: 'text',
                content: data.content,
                createTime: new Date().toISOString(),
                status: 'streaming',
              };
              messages.value.push(assistantMsg);
            } else {
              assistantMsg.content += data.content;
            }
            nextTick().then(() => scrollToBottom(true));
          } else if (data.type === 'complete') {
            if (assistantMsg) {
              assistantMsg.content = data.content;
              assistantMsg.status = 'completed';
            }
          } else if (data.type === 'error') {
            if (assistantMsg) {
              assistantMsg.status = 'failed';
            }
            reject(new Error(data.msg || '流式响应失败'));
          }
        } catch (err) {
          console.error('parse stream event failed', err);
        }
      },
      (err: any) => {
        if (assistantMsg && assistantMsg.status === 'streaming') {
          assistantMsg.status = 'failed';
        }
        reject(err);
      },
      () => {
        if (assistantMsg && assistantMsg.status === 'streaming') {
          assistantMsg.status = 'completed';
        }
        resolve();
      },
    );
  });
}

async function stopMessage() {
  if (cancelling.value) return;
  cancelling.value = true;
  abortController.value?.abort();
  const turnId = activeTurnId.value;
  if (turnId) {
    try { await cancelTurn(turnId); }
    catch { /* best-effort */ }
  }
}

// ── Scroll ─────────────────────────────────────────────────────────
const isScrolling = ref(false);

function handleScroll() {
  const el = chatBox.value;
  if (!el) return;
  isScrolling.value = el.scrollTop + el.clientHeight < el.scrollHeight - 80;
}

async function scrollToBottom(force = false) {
  await nextTick();
  if (chatBox.value && (force || !isScrolling.value)) {
    chatBox.value.scrollTop = chatBox.value.scrollHeight;
  }
}

watch([messages, () => messages.value.length], async () => {
  await nextTick(); scrollToBottom();
}, { flush: 'post' });

// ── Message actions ────────────────────────────────────────────────
async function copyContent(text: string) {
  try { await navigator.clipboard.writeText(text); message.success('已复制'); }
  catch { message.error('复制失败'); }
}

function editMessage(msg: any) { inputText.value = msg.content; }

onMounted(() => { chatBox.value?.addEventListener('scroll', handleScroll); });
onUnmounted(() => { chatBox.value?.removeEventListener('scroll', handleScroll); });

</script>

<template>
  <div class="agent-session-shell" style="display:flex;height:100%;background:var(--ant-color-bg-layout)">
    <SessionSidebar
      :sessions="sessions"
      :current-session-id="sessionId"
      :collapsed="sidebarCollapsed"
      :loading="sessionsLoading"
      @select="selectSession"
      @create="handleCreateSession"
      @delete="deleteSession"
      @toggle-collapse="sidebarCollapsed = !sidebarCollapsed"
    />

    <!-- ── Main chat area ───────────────────────────────────────────── -->
    <div style="flex:1;display:flex;flex-direction:column;min-width:0;background:var(--ant-color-bg-container);margin:8px;border-radius:8px;box-shadow:0 1px 3px rgba(0,0,0,.08)">
      <!-- Header -->
      <div style="display:flex;align-items:center;justify-content:space-between;padding:12px 20px;border-bottom:1px solid var(--ant-color-border-secondary)">
        <div style="display:flex;align-items:center;gap:8px">
          <span style="font-size:16px;font-weight:600">
            {{ session?.objective || 'Pent Agent 对话' }}
          </span>
          <span v-if="messages.length > 0" style="color:#999;font-size:13px">({{ messages.length }})</span>
          <a-tag v-if="session?.status === 'draft'" color="blue" size="small">草稿</a-tag>
          <a-tag v-if="session?.active_run_id" color="green" size="small">运行中</a-tag>
        </div>
          <a-button size="small" @click="messages = []" :disabled="loading || messages.length === 0" style="color:#999">
            清空
          </a-button>
      </div>

      <!-- Messages -->
      <div ref="chatBox" style="flex:1;overflow-y:auto;padding:16px 20px" @scroll="handleScroll">
        <a-spin v-if="initializing" size="large" style="display:block;text-align:center;margin-top:60px" tip="加载会话…" />

        <!-- Empty state -->
        <div v-if="!initializing && messages.length === 0" style="display:flex;flex-direction:column;align-items:center;justify-content:center;height:100%;color:#999;gap:8px">
          <span style="font-size:48px">🤖</span>
          <span style="font-size:15px">输入测试目标开始安全对话</span>
          <span style="font-size:12px">例如：扫描 192.168.1.0/24、分析 Web 应用漏洞</span>
        </div>

        <!-- Message list -->
        <MessageItem
          v-for="(msg, idx) in messages"
          :key="idx"
          :message="msg"
          @copy="copyContent"
          @edit="editMessage"
        />
      </div>

      <!-- Input area -->
      <MessageInput
        v-model="inputText"
        :loading="loading"
        :cancelling="cancelling"
        @send="sendMessage"
        @stop="stopMessage"
      />
    </div>
  </div>
</template>

<style scoped>
.agent-session-shell {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}
</style>
