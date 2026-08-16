<script lang="ts" setup>
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue';
import { useRoute } from 'vue-router';
import { message, Modal } from 'ant-design-vue';

import {
  confirmPentSessionScope,
  createPentSession,
  deletePentSession,
  getPentProjects,
  getPentSessionMessages,
  getPentSessions,
  sendPentSessionMessage,
  startPentSessionRun,
  streamPentRunEvents,
  streamPentTurn,
} from '#/api/pent';

import { parseTurnStreamEvent } from './turn-stream';
import { groupSessionsByTime } from './session-groups';

const route = useRoute();

// ── sidebar state ───────────────────────────────────────────────────────────
const sessions = ref<any[]>([]);
const sessionsLoading = ref(false);
const searchText = ref('');
const projectFilter = ref('');
const projects = ref<any[]>([]);
const currentId = ref<null | string>(null);

async function loadSessions() {
  sessionsLoading.value = true;
  try {
    sessions.value = ((await getPentSessions()) as any[]) || [];
  } catch {
    sessions.value = [];
  } finally {
    sessionsLoading.value = false;
  }
}

const filteredSessions = computed(() => {
  let list = [...sessions.value].sort((a, b) =>
    (b.updated_at || b.created_at || '').localeCompare(a.updated_at || a.created_at || ''),
  );
  if (projectFilter.value === '__unbound__') {
    list = list.filter((s) => !s.project_id);
  } else if (projectFilter.value) {
    list = list.filter((s) => s.project_id === projectFilter.value);
  }
  const q = searchText.value.trim().toLowerCase();
  if (q) {
    list = list.filter(
      (s) =>
        (s.objective || '').toLowerCase().includes(q) ||
        (s.id || '').toLowerCase().includes(q),
    );
  }
  return list;
});

const sessionGroups = computed(() =>
  groupSessionsByTime(filteredSessions.value),
);

const current = computed(
  () => sessions.value.find((s) => s.id === currentId.value) || null,
);

const projectName = computed(() => {
  const m: Record<string, string> = {};
  for (const p of projects.value) m[p.id] = p.name || p.id;
  return m;
});

// ── new session modal ────────────────────────────────────────────────────────
const showCreate = ref(false);
const creating = ref(false);
const createForm = ref({ objective: '', project_id: undefined as string | undefined, targetsText: '' });

function openCreate() {
  createForm.value = { objective: '', project_id: undefined, targetsText: '' };
  showCreate.value = true;
}

function projectTargetsOf(project: any): string[] {
  const scope = project?.asset_scope;
  if (Array.isArray(scope)) {
    return scope.filter((t: any) => typeof t === 'string');
  }
  if (scope && typeof scope === 'object' && Array.isArray(scope.targets)) {
    return scope.targets.filter((t: any) => typeof t === 'string');
  }
  return [];
}

function onProjectChange() {
  const project = projects.value.find((p) => p.id === createForm.value.project_id);
  const targets = projectTargetsOf(project);
  if (targets.length > 0) {
    createForm.value.targetsText = targets.join('\n');
  }
}

async function doCreate() {
  const targets = createForm.value.targetsText
    .split(/[\n,]/)
    .map((t) => t.trim())
    .filter(Boolean);
  if (!createForm.value.objective.trim()) {
    message.warning('请输入对话目标');
    return;
  }
  if (!targets.length) {
    message.warning('请输入至少一个授权目标');
    return;
  }
  creating.value = true;
  try {
    const session: any = await createPentSession({
      budget: {},
      constraints: {},
      objective: createForm.value.objective.trim(),
      project_id: createForm.value.project_id || undefined,
      targets,
    });
    message.success('对话已创建');
    showCreate.value = false;
    await loadSessions();
    if (session?.id) selectSession(session.id);
  } catch (error: any) {
    message.error(error?.message || '创建失败');
  } finally {
    creating.value = false;
  }
}

function removeSession(s: any) {
  Modal.confirm({
    content: `确定删除对话「${s.objective || s.id}」？此操作不可撤销。`,
    async onOk() {
      await deletePentSession(s.id);
      message.success('已删除');
      if (currentId.value === s.id) currentId.value = null;
      await loadSessions();
    },
    title: '删除对话',
  });
}

// ── messages ────────────────────────────────────────────────────────────────
const messages = ref<any[]>([]);
const messagesLoading = ref(false);
const liveEvents = ref<any[]>([]);
const messagesEl = ref<HTMLElement>();

async function loadMessages(id: string) {
  messagesLoading.value = true;
  try {
    messages.value = ((await getPentSessionMessages(id)) as any[]) || [];
  } catch {
    messages.value = [];
  } finally {
    messagesLoading.value = false;
    scrollToBottom();
  }
}

function selectSession(id: string) {
  stopStream();
  stopTurnStream();
  turnEvents.value = [];
  currentId.value = id;
  liveEvents.value = [];
  loadMessages(id);
}

function scrollToBottom() {
  nextTick(() => {
    if (messagesEl.value) messagesEl.value.scrollTop = messagesEl.value.scrollHeight;
  });
}

// ── composer ────────────────────────────────────────────────────────────────
const inputText = ref('');
const sending = ref(false);
/** 正在流式回复的 turn 事件（assistant/tool 消息） */
const turnEvents = ref<any[]>([]);
const turnStreaming = ref(false);
let turnCtrl: AbortController | null = null;
let turnStreamSessionId: null | string = null;

async function send() {
  const content = inputText.value.trim();
  if (!content || !currentId.value) return;
  sending.value = true;
  try {
    const resp: any = await sendPentSessionMessage(currentId.value, { content });
    inputText.value = '';
    await loadMessages(currentId.value);
    const turnId = resp?.turn_id;
    if (turnId) startTurnStream(turnId);
  } catch (error: any) {
    message.error(error?.message || '发送失败');
  } finally {
    sending.value = false;
  }
}

/** 订阅 turn 的流式回复：助手消息/工具调用实时渲染，结束后重新拉取消息 */
function startTurnStream(turnId: string) {
  stopTurnStream();
  turnEvents.value = [];
  turnStreaming.value = true;
  const ctrl = new AbortController();
  turnCtrl = ctrl;
  turnStreamSessionId = currentId.value;
  streamPentTurn(
    turnId,
    ctrl,
    (event: any) => {
      if (ctrl.signal.aborted || currentId.value !== turnStreamSessionId) return;
      let parsed;
      try {
        parsed = parseTurnStreamEvent(event);
      } catch {
        message.error('收到无法解析的回复数据');
        finishTurnStream(ctrl, true);
        return;
      }
      if (parsed.kind === 'done' || parsed.kind === 'cancelled') {
        finishTurnStream(ctrl, true);
        return;
      }
      if (parsed.kind === 'error') {
        message.error(parsed.message);
        finishTurnStream(ctrl, true);
        return;
      }
      turnEvents.value.push(parsed.message);
      scrollToBottom();
    },
    (error: any) => {
      if (!ctrl.signal.aborted) {
        message.error(error?.message || '回复流连接失败');
        finishTurnStream(ctrl, true);
      }
      throw error;
    },
    () => {
      finishTurnStream(ctrl, true);
    },
  ).catch((error: any) => {
    if (!ctrl.signal.aborted) {
      message.error(error?.message || '回复流连接失败');
      finishTurnStream(ctrl, true);
    }
  });
}

function finishTurnStream(ctrl: AbortController, reload: boolean) {
  if (turnCtrl !== ctrl) return;
  const sessionId = turnStreamSessionId;
  turnCtrl = null;
  turnStreamSessionId = null;
  turnStreaming.value = false;
  ctrl.abort();
  if (reload && sessionId && currentId.value === sessionId) loadMessages(sessionId);
}

function stopTurnStream() {
  turnCtrl?.abort();
  turnCtrl = null;
  turnStreamSessionId = null;
  turnStreaming.value = false;
}

function onComposerKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    send();
  }
}

// ── run execution & streaming ────────────────────────────────────────────────
const running = ref(false);
const streamCtrl = ref<AbortController | null>(null);

async function startRun() {
  if (!current.value || running.value) return;
  running.value = true;
  try {
    const resp: any = await startPentSessionRun(current.value.id, {});
    const runId = resp?.run_id;
    if (!runId) throw new Error('未返回 run_id');
    message.success('Agent 已开始执行');
    streamRunEvents(runId);
  } catch (error: any) {
    const text = error?.message || '';
    // 范围未确认时先用会话目标自动确认再重试一次
    if (/scope|范围|授权/i.test(text) && current.value?.targets?.length) {
      try {
        await confirmPentSessionScope(current.value.id, {
          constraints: {},
          targets: current.value.targets,
        });
        const resp: any = await startPentSessionRun(current.value.id, {});
        if (resp?.run_id) {
          message.success('Agent 已开始执行');
          streamRunEvents(resp.run_id);
          return;
        }
      } catch (e2: any) {
        message.error(e2?.message || '启动执行失败');
      }
    } else {
      message.error(text || '启动执行失败');
    }
    running.value = false;
  }
}

function streamRunEvents(runId: string) {
  const ctrl = new AbortController();
  streamCtrl.value = ctrl;
  streamPentRunEvents(
    runId,
    ctrl,
    (event: any) => {
      const data = JSON.parse(event.data || '{}');
      liveEvents.value.push({
        occurred_at: data.occurred_at || data.created_at,
        payload: data.payload || data,
        type: event.event || data.event_type || 'event',
      });
      if (liveEvents.value.length > 300) liveEvents.value.shift();
      scrollToBottom();
    },
    () => {
      running.value = false;
    },
    () => {
      running.value = false;
      loadMessages(currentId.value!);
    },
  );
}

function stopStream() {
  streamCtrl.value?.abort();
  streamCtrl.value = null;
  running.value = false;
}

onUnmounted(() => {
  stopStream();
  stopTurnStream();
});

// ── rendering helpers ────────────────────────────────────────────────────────
function fmtTime(t?: string) {
  if (!t) return '';
  const d = new Date(t);
  return Number.isNaN(d.getTime()) ? t : d.toLocaleTimeString();
}

function payloadText(payload: any): string {
  if (!payload) return '';
  if (typeof payload === 'string') return payload;
  return JSON.stringify(payload, null, 2);
}

const sessionStatus: Record<string, { color: string; label: string }> = {
  active: { color: 'processing', label: '进行中' },
  archived: { color: 'default', label: '已归档' },
  completed: { color: 'success', label: '已完成' },
  draft: { color: 'default', label: '草稿' },
};

onMounted(async () => {
  try {
    projects.value = ((await getPentProjects()) as any[]) || [];
  } catch {
    projects.value = [];
  }
  await loadSessions();
  const routeProjectId = Array.isArray(route.query.project_id)
    ? route.query.project_id[0]
    : route.query.project_id;
  const routeSessionId = Array.isArray(route.query.session_id)
    ? route.query.session_id[0]
    : route.query.session_id;
  if (routeProjectId) projectFilter.value = routeProjectId;
  const requested = routeSessionId
    ? sessions.value.find((session) => session.id === routeSessionId)
    : null;
  if (requested) selectSession(requested.id);
  else if (filteredSessions.value.length > 0) selectSession(filteredSessions.value[0].id);
});
</script>

<template>
  <div style="display: flex; height: calc(100vh - 90px); min-height: 480px">
    <!-- 左侧会话栏 -->
    <div
      style="
        width: 280px;
        flex-shrink: 0;
        border-right: 1px solid var(--ant-color-border-secondary);
        display: flex;
        flex-direction: column;
        background: var(--ant-color-bg-container);
      "
    >
      <div style="padding: 12px; border-bottom: 1px solid var(--ant-color-border-secondary)">
        <a-button type="primary" block @click="openCreate">新对话</a-button>
        <a-input
          v-model:value="searchText"
          placeholder="搜索对话..."
          allow-clear
          size="small"
          style="margin-top: 8px"
        />
        <a-select
          v-model:value="projectFilter"
          size="small"
          style="width: 100%; margin-top: 8px"
          :options="[
            { label: '全部项目', value: '' },
            { label: '未绑定项目', value: '__unbound__' },
            ...projects.map((p) => ({ label: p.name || p.id, value: p.id })),
          ]"
          title="按项目筛选"
        />
      </div>
      <div style="flex: 1; overflow-y: auto; padding: 8px">
        <a-spin :spinning="sessionsLoading">
          <a-empty v-if="!filteredSessions.length" description="暂无对话" :image="false" />
          <section v-for="group in sessionGroups" :key="group.key">
            <div
              style="
                padding: 8px 10px 4px;
                color: var(--ant-color-text-tertiary);
                font-size: 12px;
                font-weight: 500;
              "
            >{{ group.label }}</div>
            <div
              v-for="s in group.items"
              :key="s.id"
              :style="{
                background:
                  s.id === currentId ? 'var(--ant-color-primary-bg)' : 'transparent',
                borderRadius: '6px',
              }"
              style="padding: 8px 10px; cursor: pointer; margin-bottom: 4px"
              @click="selectSession(s.id)"
            >
              <div style="display: flex; align-items: center; gap: 6px">
                <span
                  style="
                    font-size: 13px;
                    font-weight: 500;
                    overflow: hidden;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                    flex: 1;
                  "
                >{{ s.objective || s.id?.slice(0, 12) }}</span>
                <a
                  style="font-size: 12px; color: var(--ant-color-text-tertiary)"
                  @click.stop="removeSession(s)"
                >删除</a>
              </div>
              <div style="display: flex; align-items: center; gap: 6px; margin-top: 4px">
                <a-tag
                  :color="sessionStatus[s.status]?.color || 'default'"
                  style="font-size: 11px; margin-inline-end: 0"
                >
                  {{ sessionStatus[s.status]?.label || s.status || '—' }}
                </a-tag>
                <span style="font-size: 11px; color: var(--ant-color-text-tertiary)">
                  {{ (s.updated_at || s.created_at || '').slice(0, 16).replace('T', ' ') }}
                </span>
              </div>
            </div>
          </section>
        </a-spin>
      </div>
    </div>

    <!-- 右侧对话区 -->
    <div style="flex: 1; display: flex; flex-direction: column; min-width: 0">
      <template v-if="current">
        <!-- 对话头 -->
        <div
          style="
            padding: 10px 16px;
            border-bottom: 1px solid var(--ant-color-border-secondary);
            display: flex;
            align-items: center;
            gap: 8px;
            flex-wrap: wrap;
          "
        >
          <span style="font-weight: 600">{{ current.objective || '未命名对话' }}</span>
          <a-tag :color="sessionStatus[current.status]?.color || 'default'">
            {{ sessionStatus[current.status]?.label || current.status || '—' }}
          </a-tag>
          <a-tag v-if="current.project_id" style="font-size: 12px">
            {{ projectName[current.project_id] || current.project_id }}
          </a-tag>
          <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">
            目标：{{ (current.targets || []).join('、') || '—' }}
          </span>
          <span style="margin-left: auto">
            <a-button
              type="primary"
              size="small"
              :loading="running"
              @click="startRun"
            >
              {{ running ? '执行中…' : '启动执行' }}
            </a-button>
          </span>
        </div>

        <!-- 消息区 -->
        <div ref="messagesEl" style="flex: 1; overflow-y: auto; padding: 16px">
          <a-spin :spinning="messagesLoading">
            <a-empty
              v-if="!messages.length && !liveEvents.length"
              description="暂无消息，在下方输入内容开始对话；点击「启动执行」让 Agent 自主工作"
              style="margin-top: 64px"
            />
            <div v-for="m in messages" :key="m.id" style="margin-bottom: 12px">
              <!-- 用户消息靠右 -->
              <div v-if="m.role === 'user'" style="display: flex; justify-content: flex-end">
                <div
                  style="
                    max-width: 70%;
                    background: var(--ant-color-primary);
                    color: #fff;
                    border-radius: 10px 10px 2px 10px;
                    padding: 8px 12px;
                    white-space: pre-wrap;
                    word-break: break-word;
                    font-size: 13px;
                  "
                >{{ m.content }}</div>
              </div>
              <!-- 系统消息居中 -->
              <div
                v-else-if="m.role === 'system'"
                style="text-align: center; font-size: 12px; color: var(--ant-color-text-tertiary)"
              >{{ m.content }}</div>
              <!-- 助手消息靠左 -->
              <div v-else style="display: flex; justify-content: flex-start">
                <div
                  style="
                    max-width: 78%;
                    background: var(--ant-color-bg-layout);
                    border-radius: 10px 10px 10px 2px;
                    padding: 8px 12px;
                    font-size: 13px;
                  "
                >
                  <div style="white-space: pre-wrap; word-break: break-word">{{ m.content }}</div>
                  <div
                    v-if="m.message_type && m.message_type !== 'text'"
                    style="margin-top: 4px; font-size: 11px; color: var(--ant-color-text-tertiary)"
                  >{{ m.message_type }}</div>
                </div>
              </div>
            </div>

            <!-- 流式回复中的 turn 事件 -->
            <template v-if="turnEvents.length || turnStreaming">
              <a-divider style="font-size: 12px; color: var(--ant-color-text-tertiary)">
                {{ turnStreaming ? '正在回复…' : '本轮回复' }}
              </a-divider>
              <div v-for="(ev, i) in turnEvents" :key="i" style="margin-bottom: 12px">
                <!-- 工具调用/结果：可折叠卡片 -->
                <div
                  v-if="ev.role === 'tool' || ev.format === 'tool_calls'"
                  style="
                    border: 1px solid var(--ant-color-border-secondary);
                    border-radius: 6px;
                    padding: 6px 10px;
                    font-size: 12px;
                    background: var(--ant-color-bg-layout);
                  "
                >
                  <a-tag size="small" style="margin-inline-end: 4px">
                    {{ ev.format === 'tool_calls' ? '工具调用' : '工具结果' }}
                  </a-tag>
                  <pre
                    style="
                      margin: 4px 0 0;
                      white-space: pre-wrap;
                      word-break: break-all;
                      max-height: 160px;
                      overflow: auto;
                      color: var(--ant-color-text-secondary);
                    "
                  >{{ ev.content.slice(0, 1500) }}</pre>
                </div>
                <!-- 助手消息 -->
                <div v-else style="display: flex; justify-content: flex-start">
                  <div
                    style="
                      max-width: 78%;
                      background: var(--ant-color-bg-layout);
                      border-radius: 10px 10px 10px 2px;
                      padding: 8px 12px;
                      font-size: 13px;
                      white-space: pre-wrap;
                      word-break: break-word;
                    "
                  >{{ ev.content }}</div>
                </div>
              </div>
            </template>

            <!-- 实时执行事件流 -->
            <template v-if="liveEvents.length">
              <a-divider style="font-size: 12px; color: var(--ant-color-text-tertiary)">
                执行事件
              </a-divider>
              <div
                v-for="(ev, i) in liveEvents"
                :key="i"
                style="
                  margin-bottom: 6px;
                  padding: 6px 10px;
                  border-left: 2px solid var(--ant-color-primary);
                  background: var(--ant-color-bg-layout);
                  border-radius: 4px;
                  font-size: 12px;
                "
              >
                <span style="color: var(--ant-color-primary); font-weight: 600">{{ ev.type }}</span>
                <span style="color: var(--ant-color-text-tertiary); margin-left: 8px">
                  {{ fmtTime(ev.occurred_at) }}
                </span>
                <pre
                  style="
                    margin: 4px 0 0;
                    white-space: pre-wrap;
                    word-break: break-all;
                    max-height: 120px;
                    overflow: auto;
                    color: var(--ant-color-text-secondary);
                  "
                >{{ payloadText(ev.payload).slice(0, 800) }}</pre>
              </div>
            </template>
          </a-spin>
        </div>

        <!-- 输入区 -->
        <div
          style="
            border-top: 1px solid var(--ant-color-border-secondary);
            padding: 12px 16px;
            display: flex;
            gap: 8px;
            align-items: flex-end;
          "
        >
          <a-textarea
            v-model:value="inputText"
            :auto-size="{ maxRows: 6, minRows: 1 }"
            placeholder="输入消息，Enter 发送，Shift+Enter 换行"
            style="flex: 1"
            @keydown="onComposerKeydown"
          />
          <a-button
            type="primary"
            :loading="sending"
            :disabled="!inputText.trim()"
            @click="send"
          >发送</a-button>
        </div>
      </template>

      <a-empty
        v-else
        description="选择左侧对话，或点击「新对话」开始"
        style="margin-top: 128px"
      />
    </div>

    <!-- 新建对话弹窗 -->
    <a-modal
      v-model:open="showCreate"
      title="新对话"
      :confirm-loading="creating"
      ok-text="创建"
      @ok="doCreate"
    >
      <a-form layout="vertical">
        <a-form-item label="对话目标" required>
          <a-input
            v-model:value="createForm.objective"
            placeholder="例如：对 Juice Shop 做 Web 渗透测试"
          />
        </a-form-item>
        <a-form-item label="关联项目">
          <a-select
            v-model:value="createForm.project_id"
            allow-clear
            placeholder="可选（选择后自动继承项目目标）"
            :options="projects.map((p) => ({ label: p.name || p.id, value: p.id }))"
            @change="onProjectChange"
          />
        </a-form-item>
        <a-form-item label="授权目标" required>
          <a-textarea
            v-model:value="createForm.targetsText"
            :rows="3"
            placeholder="每行一个 URL 或 IP，例如 http://localhost:13000；选择项目后自动继承项目目标"
          />
        </a-form-item>
      </a-form>
    </a-modal>
  </div>
</template>
