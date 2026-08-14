<script lang="ts" setup>
import type { PentProjectConversation } from '#/api/pent/ops-facts';

import { computed, onMounted, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import { message } from 'ant-design-vue';

import { getPentSessions } from '#/api/pent';
import {
  bindPentConversationProject,
  getPentProjectConversations,
} from '#/api/pent/ops-facts';

const props = defineProps<{ projectId: string }>();
const router = useRouter();

const loading = ref(false);
const conversations = ref<PentProjectConversation[]>([]);

async function fetchConversations() {
  loading.value = true;
  try {
    conversations.value =
      (await getPentProjectConversations(props.projectId)) || [];
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
  <div>
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
  </div>
</template>
