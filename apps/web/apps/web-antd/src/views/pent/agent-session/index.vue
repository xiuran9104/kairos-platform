<script lang="ts" setup>
import { computed, nextTick, onMounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { message } from 'ant-design-vue';
import { cancelTurn, createTurn, listTurns } from '#/api/pent/conversation';
import {
  createPentSession,
  deletePentSession,
  getPentSession,
  getPentSessions,
  getPentSessionMessages,
} from '#/api/pent';
import { createClientRequestId } from '#/utils/client-request-id';

const route = useRoute();
const router = useRouter();
const sessionId = computed(() => route.params.id as string | undefined);
const sessions = ref<any[]>([]);
const session = ref<any>(null);
const messages = ref<any[]>([]);
const inputText = ref('');
const loading = ref(false);
const cancelling = ref(false);
const activeTurnId = ref<string>();
const initializing = ref(true);
const chatBox = ref<HTMLElement>();
const sessionsLoading = ref(false);
const sidebarCollapsed = ref(false);

// ── Session list ──────────────────────────────────────────────
async function loadSessions() {
  sessionsLoading.value = true;
  try { sessions.value = (await getPentSessions()) as any[]; }
  catch (e: any) { console.error('加载会话列表失败:', e?.message || e); }
  finally { sessionsLoading.value = false; }
}

// ── Load session + history ─────────────────────────────────────
async function initSession() {
  initializing.value = true;
  try {
    if (sessionId.value) {
      session.value = await getPentSession(sessionId.value);
      // Load historical messages from API
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
            time: m.created_at ? new Date(m.created_at).toLocaleTimeString() : '',
            status: m.status || 'completed',
          }));
      } else {
        messages.value = [{
          role: 'assistant', type: 'text',
          content: `已恢复 Session ${sessionId.value.slice(0, 12)}。你可以继续对话，追加指令或查看运行状态。`,
          time: new Date().toLocaleTimeString(), status: 'completed',
        }];
      }
    }
  } catch (e: any) {
    messages.value = [{
      role: 'assistant', type: 'error',
      content: `会话初始化失败：${e?.message || '未知错误'}`,
      time: new Date().toLocaleTimeString(), status: 'failed',
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

// Reload when navigating to a different session
watch(sessionId, async () => {
  if (sessionId.value) await initSession();
});

// ── Send message ───────────────────────────────────────────────
async function sendMessage() {
  const text = inputText.value.trim();
  if (!text || loading.value) return;
  let currentSessionId = sessionId.value;

  // Auto-create session if none selected
  if (!currentSessionId) {
    try {
      loading.value = true;
      const s: any = await createPentSession({
        objective: text.slice(0, 50),
        targets: [],
        constraints: {},
        budget: {},
      });
      const newId = s?.id ?? s?.data?.id;
      if (newId) {
        currentSessionId = newId;
        await loadSessions();
        await router.replace(`/pent/sessions/${newId}`);
      }
    } catch (e: any) {
      messages.value.push({
        role: 'assistant', type: 'error',
        content: `创建会话失败：${e?.message || '未知错误'}`,
        time: new Date().toLocaleTimeString(), status: 'failed',
      });
      loading.value = false;
      return;
    }
  }
  if (!currentSessionId) {
    loading.value = false;
    message.error('无法创建会话');
    return;
  }

  const msgText = text;
  messages.value.push({ role: 'user', type: 'text', content: msgText, time: new Date().toLocaleTimeString() });
  inputText.value = '';
  loading.value = true;
  await nextTick();
  scrollToBottom();

  try {
    const resp: any = await createTurn(currentSessionId, {
      client_request_id: createClientRequestId(), content: text,
    });
    const turnId = resp?.turn_id ?? resp?.data?.turn_id;

    if (turnId) {
      activeTurnId.value = turnId;
      // Track which message IDs we've already shown
      const seenIds = new Set(messages.value.filter((m: any) => m.id).map((m: any) => m.id));
      const progressId = `scan_progress_${turnId}`;
      const progress = {
        completed: 0,
        requested: 0,
        tools: new Set<string>(),
      };
      const updateProgress = () => {
        const toolNames = [...progress.tools];
        const content = [
          '🔄 扫描进行中',
          `已完成 ${progress.completed}/${Math.max(progress.requested, progress.completed)} 次工具调用`,
          toolNames.length > 0 ? `已使用：${toolNames.join('、')}` : '',
          '原始结果正在写入 Evidence，扫描结束后统一生成报告。',
        ].filter(Boolean).join('\n');
        const existing = messages.value.find((item: any) => item.id === progressId);
        if (existing) {
          existing.content = content;
          existing.time = new Date().toLocaleTimeString();
        } else {
          messages.value.push({
            id: progressId,
            role: 'assistant',
            type: 'progress',
            content,
            time: new Date().toLocaleTimeString(),
            status: 'streaming',
          });
        }
      };

      // Keep polling for the backend's absolute turn limit (2 hours).
      // Long-running tool turns must not be reported as failed while the
      // backend is still processing them.
      let attempts = 0;
      const MAX_POLL = 3600;

      while (attempts < MAX_POLL) {
        await new Promise(r => setTimeout(r, 2000));
        const turns: any = await listTurns(currentSessionId);
        const turnList = Array.isArray(turns) ? turns : (turns?.data ?? []);
        const turn = turnList.find((t: any) => t.id === turnId);

        if (turn?.status === 'failed') {
          throw new Error(turn.error_message || '模型处理失败');
        }
        // Tool calls/results update one progress row. Only assistant text is
        // rendered as chat; raw observations remain in Evidence.
        if (turn?.messages && Array.isArray(turn.messages)) {
          for (const m of turn.messages) {
            if (m.role === 'user') continue; // skip user messages
            const msgId = m.role + '_' + (m.content?.slice(0, 50) || '') + '_' + m.sequence;
            if (seenIds.has(msgId)) continue;
            seenIds.add(msgId);

            const msgType = m.format || m.message_type || 'text';
            let displayContent = m.content;
            let displayRole = m.role;

            if (m.role === 'tool' || msgType === 'tool_result') {
              progress.completed += 1;
              try {
                const payload = typeof m.content === 'string' ? JSON.parse(m.content) : m.content;
                if (payload?.tool_name) progress.tools.add(payload.tool_name);
              } catch { /* progress count is still valid */ }
              updateProgress();
              continue;
            } else if (msgType === 'tool_calls') {
              try {
                const payload = typeof m.content === 'string' ? JSON.parse(m.content) : m.content;
                if (payload?.tool_calls) {
                  const names = payload.tool_calls.map((tc: any) => tc.function?.name || 'unknown');
                  progress.requested += names.length;
                  names.forEach((name: string) => progress.tools.add(name));
                }
              } catch { /* progress will update on results */ }
              updateProgress();
              continue;
            }

            if (displayContent?.trim()) {
              messages.value.push({
                id: msgId,
                role: displayRole,
                type: msgType,
                content: displayContent,
                payload: m.payload,
                time: new Date().toLocaleTimeString(),
                status: m.status || 'completed',
              });
            }
          }
        }

        if (turn?.status === 'cancelled') {
          messages.value.push({
            role: 'assistant', type: 'text',
            content: '⏹ 本轮处理已中止。',
            time: new Date().toLocaleTimeString(), status: 'cancelled',
          });
          break;
        }
        if (turn?.status === 'completed') {
          messages.value = messages.value.filter((item: any) => item.id !== progressId);
          break;
        }
        attempts++;
      }

      if (attempts >= MAX_POLL) {
        messages.value.push({
          role: 'assistant', type: 'text',
          content: '⏱ 已达到本轮最长处理时间，请在运行页面确认最终状态。',
          time: new Date().toLocaleTimeString(), status: 'timeout',
        });
      }
    }
  } catch (e: any) {
    messages.value.push({
      role: 'assistant', type: 'error',
      content: `发送失败：${e?.message || '未知错误'}`,
      time: new Date().toLocaleTimeString(), status: 'failed',
    });
  }
  activeTurnId.value = undefined;
  cancelling.value = false;
  loading.value = false;
  await nextTick(); scrollToBottom();
}

async function stopMessage() {
  const turnId = activeTurnId.value;
  if (!turnId || cancelling.value) return;

  cancelling.value = true;
  try {
    await cancelTurn(turnId);
    message.success('正在中止本轮处理');
  } catch (e: any) {
    cancelling.value = false;
    message.error(`中止失败：${e?.message || '未知错误'}`);
  }
}

function scrollToBottom() {
  if (chatBox.value) {
    chatBox.value.scrollTop = chatBox.value.scrollHeight;
  }
}

watch(
  [messages, () => messages.value.length],
  async () => {
    await nextTick();
    scrollToBottom();
  },
  { flush: 'post' },
);

// ── Session management ─────────────────────────────────────────
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

function selectSession(s: any) {
  router.push(`/pent/sessions/${s.id}`);
}

// ── Create session ─────────────────────────────────────────────
async function handleCreateSession() {
  try {
    const s: any = await createPentSession({
      objective: '新安全测试会话',
      targets: [],
      constraints: {},
      budget: {},
    });
    await loadSessions();
    const id = s?.id ?? s?.data?.id;
    if (id) { router.push(`/pent/sessions/${id}`); }
  } catch { message.error('创建失败'); }
}

// Simple markdown rendering for message content
function renderContent(content: string, type: string, role?: string) {
  if (!content) return '';
  // Escape HTML
  let html = content
    .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  // Bold
  html = html.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
  // Inline code
  html = html.replace(/`([^`]+)`/g, '<code style="background:#f0f0f0;padding:1px 4px;border-radius:3px;font-size:12px">$1</code>');
  // Links
  const linkStyle = role === 'user'
    ? 'color:#fff;text-decoration:underline;text-underline-offset:2px'
    : 'color:#1677ff';
  html = html.replace(
    /https?:\/\/[^\s<>"]+/g,
    `<a href="$&" target="_blank" rel="noopener noreferrer" style="${linkStyle}">$&</a>`,
  );
  // Line breaks
  html = html.replace(/\n/g, '<br>');
  // Emoji for tool_calls
  if (type === 'tool_calls') {
    html = '🔍 ' + html;
  }
  return html;
}
</script>

<template>
  <div class="agent-session-shell">
    <!-- Sidebar -->
    <div :style="{width: sidebarCollapsed ? '0px' : '260px', transition: 'width 0.2s', overflow: 'hidden', background: '#fff', borderRight: '1px solid #e8e8e8', display: 'flex', flexDirection: 'column'}">
      <div style="padding:16px;border-bottom:1px solid #f0f0f0;display:flex;justify-content:space-between;align-items:center">
        <span style="font-weight:600;font-size:15px">Sessions</span>
        <a-space>
          <a-button size="small" type="primary" @click="handleCreateSession">+ 新建</a-button>
          <a-button size="small" @click="sidebarCollapsed = true">✕</a-button>
        </a-space>
      </div>
      <div style="flex:1;overflow-y:auto;padding:8px">
        <div v-if="sessionsLoading" style="text-align:center;padding:20px;color:#999">加载中…</div>
        <div v-for="s in sessions" :key="s.id" @click="selectSession(s)"
          :style="{padding:'10px 12px',cursor:'pointer',borderRadius:'6px',marginBottom:'4px',background: sessionId === s.id ? '#e6f4ff' : 'transparent',border: sessionId === s.id ? '1px solid #91caff' : '1px solid transparent'}">
          <div style="display:flex;justify-content:space-between;align-items:center">
            <div style="flex:1;min-width:0">
              <div style="font-size:13px;font-weight:500;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{{ s.objective }}</div>
              <div style="font-size:11px;color:#999;margin-top:2px">{{ s.created_at?.slice(0,16) }} · {{ s.status }}</div>
            </div>
            <a-button type="link" size="small" danger @click.stop="deleteSession(s)" style="flex-shrink:0;margin-left:4px">删除</a-button>
          </div>
        </div>
      </div>
    </div>

    <!-- Expand sidebar button -->
    <div v-if="sidebarCollapsed" style="position:absolute;left:0;top:16px;z-index:10">
      <a-button size="small" @click="sidebarCollapsed = false">☰ Sessions</a-button>
    </div>

    <!-- Main chat area -->
    <div style="flex:1;display:flex;flex-direction:column;min-width:0">
      <!-- Header -->
      <div style="background:#fff;border-bottom:1px solid #e8e8e8;padding:12px 24px;display:flex;align-items:center;justify-content:space-between">
        <div>
          <h2 style="margin:0;font-size:16px">
            {{ session?.objective || 'Pent Agent 对话' }}
            <a-tag v-if="session?.status === 'draft'" color="blue" size="small" style="margin-left:8px">草稿</a-tag>
            <a-tag v-if="session?.active_run_id" color="green" size="small" style="margin-left:8px">运行中</a-tag>
          </h2>
        </div>
      </div>

      <!-- Messages -->
      <div ref="chatBox" style="flex:1;overflow-y:auto;padding:16px 24px">
        <a-spin v-if="initializing" size="large" style="display:block;text-align:center;margin-top:40px">
          <template #tip>加载会话…</template>
        </a-spin>

        <div v-for="(msg, idx) in messages" :key="idx" :style="{marginBottom:'16px',display:'flex',justifyContent:msg.role==='user'?'flex-end':'flex-start'}">
          <div :style="{maxWidth:'80%',padding:'10px 16px',borderRadius:'8px',fontSize:'13px',lineHeight:'1.6',whiteSpace:'pre-wrap',wordBreak:'break-word',
            background: msg.role === 'user' ? '#1677ff' : msg.type === 'tool_result' || msg.role === 'tool' ? '#f6ffed' : msg.type === 'tool_calls' ? '#fff7e6' : msg.type === 'error' ? '#fff2f0' : '#fff',
            color: msg.role === 'user' ? '#fff' : msg.type === 'error' ? '#cf1322' : '#333',
            border: msg.role !== 'user' ? '1px solid ' + (msg.type === 'error' ? '#ffccc7' : msg.type === 'tool_result' || msg.role === 'tool' ? '#b7eb8f' : msg.type === 'tool_calls' ? '#ffe58f' : '#d9d9d9') : 'none',
            overflowWrap: 'break-word'}">
            <div style="font-size:10px;color:msg.role==='user'?'rgba(255,255,255,.6)':'#999';margin-bottom:2px">
              {{ msg.role === 'user' ? '我' : msg.role === 'tool' ? '🔧 工具' : '🤖 Pent' }} · {{ msg.time }}
              <a-tag v-if="msg.status === 'timeout'" color="orange" size="small" style="margin-left:4px">超时</a-tag>
              <a-tag v-if="msg.status === 'failed'" color="red" size="small" style="margin-left:4px">失败</a-tag>
            </div>
            <div v-html="renderContent(msg.content, msg.type, msg.role)" />
          </div>
        </div>
      </div>

      <!-- Processing state + input -->
      <div style="background:#fff;border-top:1px solid #e8e8e8;padding:10px 24px 12px">
        <div
          v-if="loading"
          style="display:flex;align-items:center;gap:8px;margin-bottom:8px;color:#666;font-size:13px"
        >
          <a-spin size="small" />
          <span>{{ cancelling ? '正在中止本轮处理…' : '正在处理，可点击右侧按钮中止' }}</span>
        </div>
        <div style="display:flex;align-items:flex-end;gap:8px">
          <a-textarea
            v-model:value="inputText"
            :auto-size="{ minRows:1, maxRows:4 }"
            placeholder="输入测试目标或指令… (Enter 发送，Shift+Enter 换行)"
            :disabled="loading"
            @keydown.enter.exact.prevent="sendMessage"
            style="flex:1;border-radius:8px"
          />
          <a-button
            v-if="loading"
            danger
            :loading="cancelling"
            :disabled="!activeTurnId"
            style="flex-shrink:0;height:36px"
            @click="stopMessage"
          >
            {{ cancelling ? '中止中…' : '中止' }}
          </a-button>
          <a-button
            v-else
            type="primary"
            :disabled="!inputText.trim()"
            style="flex-shrink:0;height:36px"
            @click="sendMessage"
          >
            发送
          </a-button>
        </div>
      </div>
    </div>

  </div>
</template>

<style scoped>
.agent-session-shell {
  display: flex;
  width: 100%;
  height: 100%;
  overflow: hidden;
  background: #f5f5f5;
}
</style>
