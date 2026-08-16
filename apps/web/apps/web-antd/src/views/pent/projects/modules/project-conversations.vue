<script lang="ts" setup>
import type { PentProjectConversation } from '#/api/pent/ops-facts';

import { computed, onMounted, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import { message } from 'ant-design-vue';

import { getPentSessions, getPentSessionMessages } from '#/api/pent';
import {
  bindPentConversationProject,
  getPentProjectConversations,
} from '#/api/pent/ops-facts';

const props = defineProps<{ projectId: string }>();
const router = useRouter();

const loading = ref(false);
const conversations = ref<PentProjectConversation[]>([]);
const selectedId = ref<string>();
const selectedRowKeys = ref<string[]>([]);
const messages = ref<any[]>([]);
const messagesLoading = ref(false);

async function loadMessages(id: string) {
  messagesLoading.value = true;
  try {
    messages.value = ((await getPentSessionMessages(id)) as any[]) || [];
  } catch {
    messages.value = [];
  } finally {
    messagesLoading.value = false;
  }
}

function selectConversation(id: string) {
  selectedId.value = id;
  selectedRowKeys.value = [id];
  loadMessages(id);
}

async function fetchConversations() {
  loading.value = true;
  try {
    conversations.value =
      (await getPentProjectConversations(props.projectId)) || [];
    // 默认直接展示第一个（最新）会话的对话内容
    const first = conversations.value[0]?.id;
    if (first && selectedId.value !== first) {
      selectConversation(first);
    }
    if (!first) {
      selectedId.value = undefined;
      messages.value = [];
    }
  } catch {
    message.error('加载关联对话失败');
    conversations.value = [];
  } finally {
    loading.value = false;
  }
}

// ── 绑定对话 ───────────────────────────────────────────────────────────────
const showBind = ref(false);
const binding = ref(false);
const allSessions = ref<any[]>([]);
const bindSessionId = ref<string>();

const bindOptions = computed(() => {
  const bound = new Set(conversations.value.map((c) => c.id));
  return allSessions.value
    .filter((s) => !bound.has(s.id))
    .map((s) => ({
      label: `${(s.objective || s.id).slice(0, 40)}（${s.status}）`,
      value: s.id,
    }));
});

async function openBind() {
  showBind.value = true;
  bindSessionId.value = undefined;
  try {
    allSessions.value = ((await getPentSessions()) as any[]) || [];
  } catch {
    allSessions.value = [];
  }
}

async function doBind() {
  if (!bindSessionId.value) {
    message.warning('请选择对话');
    return;
  }
  binding.value = true;
  try {
    await bindPentConversationProject(bindSessionId.value, props.projectId);
    message.success('已绑定');
    showBind.value = false;
    await fetchConversations();
  } catch {
    message.error('绑定失败');
  } finally {
    binding.value = false;
  }
}

async function unbind(id: string) {
  try {
    await bindPentConversationProject(id, null);
    message.success('已解绑');
    await fetchConversations();
  } catch {
    message.error('解绑失败');
  }
}

const statusColor: Record<string, string> = {
  ready: 'green',
  running: 'processing',
  draft: 'default',
};

const columns = [
  { title: 'ID', dataIndex: 'id', key: 'id', width: 120 },
  { title: '目标', dataIndex: 'objective', key: 'objective', ellipsis: true },
  { title: '状态', dataIndex: 'status', key: 'status', width: 100 },
  { title: '更新时间', dataIndex: 'updated_at', key: 'updated_at', width: 160 },
  { title: '操作', key: 'actions', width: 160 },
];

function fmtTime(t?: string) {
  return t ? t.slice(0, 16).replace('T', ' ') : '—';
}

watch(() => props.projectId, fetchConversations);
onMounted(fetchConversations);
</script>

<template>
  <div style="display: flex; gap: 16px; min-height: 360px">
    <!-- 左侧：关联对话列表 -->
    <div style="flex: 1; min-width: 0">
      <div style="display: flex; margin-bottom: 12px">
        <a-button size="small" :loading="loading" @click="fetchConversations">刷新</a-button>
        <a-button size="small" type="primary" style="margin-left: 8px" @click="openBind">
          绑定对话
        </a-button>
      </div>
      <a-table
        :columns="columns"
        :data-source="conversations"
        :loading="loading"
        row-key="id"
        size="small"
        :row-selection="{
          selectedRowKeys,
          type: 'radio',
          onChange: (keys: any[]) => keys[0] && selectConversation(keys[0]),
        }"
        :custom-row="(record: any) => ({
          style: 'cursor: pointer',
          onClick: () => selectConversation(record.id),
        })"
        :pagination="{ pageSize: 10, showTotal: (t: number) => `共 ${t} 条` }"
      >
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'id'">
            <code style="font-size: 11px">{{ record.id.slice(0, 8) }}</code>
          </template>
          <template v-else-if="column.key === 'status'">
            <a-tag size="small" :color="statusColor[record.status] || 'default'">
              {{ record.status }}
            </a-tag>
          </template>
          <template v-else-if="column.key === 'updated_at'">
            {{ fmtTime(record.updated_at) }}
          </template>
          <template v-else-if="column.key === 'actions'">
            <a-space :size="4">
              <a-button
                size="small"
                type="link"
                @click="router.push({
                  path: '/pent/chat',
                  query: { project_id: projectId, session_id: record.id },
                })"
              >
                打开对话
              </a-button>
              <a-popconfirm title="解绑该对话？" @confirm="unbind(record.id)">
                <a-button size="small" type="link" danger>解绑</a-button>
              </a-popconfirm>
            </a-space>
          </template>
        </template>
        <template #emptyText>
          <a-empty description="暂无关联对话，点击「绑定对话」关联已有会话" />
        </template>
      </a-table>
    </div>

    <!-- 右侧：会话内容预览 -->
    <div
      style="
        width: 46%;
        min-width: 340px;
        border: 1px solid var(--ant-color-border-secondary);
        border-radius: 8px;
        background: var(--ant-color-bg-container);
        display: flex;
        flex-direction: column;
      "
    >
      <div
        style="
          padding: 10px 12px;
          border-bottom: 1px solid var(--ant-color-border-secondary);
          font-weight: 600;
          font-size: 13px;
          display: flex;
          align-items: center;
          justify-content: space-between;
        "
      >
        <span>会话内容</span>
        <a-button
          v-if="selectedId"
          size="small"
          type="link"
          @click="router.push({ path: '/pent/chat', query: { project_id: projectId, session_id: selectedId } })"
        >
          在对话页打开
        </a-button>
      </div>
      <div style="flex: 1; overflow-y: auto; padding: 12px; min-height: 260px; max-height: 420px">
        <a-spin :spinning="messagesLoading">
          <a-empty
            v-if="!messages.length && !messagesLoading"
            description="暂无消息"
            style="padding: 40px 0"
          />
          <div v-for="m in messages" :key="m.id" style="margin-bottom: 10px">
            <div v-if="m.role === 'user'" style="display: flex; justify-content: flex-end">
              <div
                style="
                  max-width: 82%;
                  background: var(--ant-color-primary);
                  color: #fff;
                  padding: 8px 12px;
                  border-radius: 8px;
                  font-size: 13px;
                  white-space: pre-wrap;
                  word-break: break-all;
                "
              >
                {{ m.content }}
              </div>
            </div>
            <div
              v-else-if="m.role === 'system'"
              style="text-align: center; color: var(--ant-color-text-tertiary); font-size: 12px"
            >
              {{ m.content }}
            </div>
            <div v-else style="display: flex; justify-content: flex-start">
              <div
                style="
                  max-width: 88%;
                  background: var(--ant-color-fill-quaternary);
                  padding: 8px 12px;
                  border-radius: 8px;
                  font-size: 13px;
                  white-space: pre-wrap;
                  word-break: break-all;
                "
              >
                {{ m.content }}
              </div>
            </div>
          </div>
        </a-spin>
      </div>
    </div>
  </div>

    <a-modal
      v-model:open="showBind"
      title="绑定对话"
      :confirm-loading="binding"
      @ok="doBind"
    >
      <a-form layout="vertical">
        <a-form-item label="选择对话" required>
          <a-select
            v-model:value="bindSessionId"
            :options="bindOptions"
            placeholder="选择一个未绑定的对话"
            show-search
            option-filter-prop="label"
          />
        </a-form-item>
      </a-form>
    </a-modal>
</template>
