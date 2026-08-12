<script lang="ts" setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import { message } from 'ant-design-vue';

import {
  cancelPentTask,
  createPentTask,
  deletePentTask,
  getPentProjects,
  getPentTasks,
} from '#/api/pent';

const router = useRouter();

// ── state ──────────────────────────────────────────────────────────────────
const loading = ref(false);
const tasks = ref<any[]>([]);
const projects = ref<any[]>([]);
const searchText = ref('');
const statusFilter = ref('all');

async function fetchTasks(silent = false) {
  if (!silent) loading.value = true;
  try {
    tasks.value = ((await getPentTasks()) as any[]) || [];
  } catch {
    if (!silent) message.error('加载失败');
  } finally {
    if (!silent) loading.value = false;
  }
}

async function fetchProjects() {
  try {
    projects.value = ((await getPentProjects()) as any[]) || [];
  } catch {
    projects.value = [];
  }
}

function projectName(projectId?: string) {
  return projects.value.find((p) => p.id === projectId)?.name || '—';
}

// ── auto refresh ───────────────────────────────────────────────────────────
const autoRefresh = ref(false);
let timer: ReturnType<typeof setInterval> | null = null;

watch(autoRefresh, (on) => {
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
  if (on) timer = setInterval(() => fetchTasks(true), 10_000);
});

onBeforeUnmount(() => {
  if (timer) clearInterval(timer);
});

// ── filter & pagination ────────────────────────────────────────────────────
const statusOptions = [
  { label: '全部状态', value: 'all' },
  { label: '草稿', value: 'draft' },
  { label: '排队中', value: 'queued' },
  { label: '待执行', value: 'pending' },
  { label: '运行中', value: 'running' },
  { label: '已暂停', value: 'paused' },
  { label: '已完成', value: 'completed' },
  { label: '失败', value: 'failed' },
  { label: '已取消', value: 'cancelled' },
];

const statusColor: Record<string, string> = {
  draft: 'default',
  queued: 'blue',
  pending: 'blue',
  running: 'processing',
  paused: 'warning',
  waiting_approval: 'warning',
  completed: 'success',
  failed: 'error',
  cancelled: 'default',
  cancelling: 'warning',
};
const statusLabel: Record<string, string> = {
  draft: '草稿',
  queued: '排队中',
  pending: '待执行',
  running: '运行中',
  paused: '已暂停',
  waiting_approval: '待审批',
  completed: '已完成',
  failed: '失败',
  cancelled: '已取消',
  cancelling: '取消中',
};

const priorityColor: Record<string, string> = {
  low: 'green',
  medium: 'orange',
  high: 'red',
  critical: 'var(--ant-color-error)',
};
const priorityLabel: Record<string, string> = {
  low: '低',
  medium: '中',
  high: '高',
  critical: '紧急',
};

const filtered = computed(() => {
  let list = tasks.value;
  if (statusFilter.value !== 'all') {
    list = list.filter((t) => t.status === statusFilter.value);
  }
  if (searchText.value) {
    const q = searchText.value.toLowerCase();
    list = list.filter(
      (t) =>
        (t.name || '').toLowerCase().includes(q) ||
        (t.id || '').toLowerCase().includes(q),
    );
  }
  return list;
});

const page = ref(1);
const pageSize = ref(12);
const paged = computed(() => {
  const start = (page.value - 1) * pageSize.value;
  return filtered.value.slice(start, start + pageSize.value);
});

watch([searchText, statusFilter], () => {
  page.value = 1;
});

function fmtTime(t?: string) {
  return t ? t.slice(0, 16).replace('T', ' ') : '—';
}

const canCancel = (t: any) =>
  ['queued', 'pending', 'running', 'paused', 'waiting_approval'].includes(t.status);

// ── actions ────────────────────────────────────────────────────────────────
function goMonitor(t: any) {
  router.push(`/pent/tasks/${t.id}`);
}

async function doCancel(t: any) {
  try {
    await cancelPentTask(t.id);
    message.success('任务已取消');
    fetchTasks(true);
  } catch {
    message.error('取消失败');
  }
}

async function doDelete(t: any) {
  try {
    await deletePentTask(t.id);
    message.success('已删除');
    fetchTasks(true);
  } catch {
    message.error('删除失败');
  }
}

// ── create modal ───────────────────────────────────────────────────────────
const showCreate = ref(false);
const creating = ref(false);
const form = ref({
  project_id: undefined as string | undefined,
  name: '',
  description: '',
  priority: 'medium',
});

function openCreate() {
  form.value = {
    project_id: projects.value.length === 1 ? projects.value[0].id : undefined,
    name: '',
    description: '',
    priority: 'medium',
  };
  showCreate.value = true;
}

async function doCreate() {
  if (!form.value.project_id) {
    message.warning('请选择所属项目');
    return;
  }
  if (!form.value.name.trim()) {
    message.warning('请输入任务名称');
    return;
  }
  creating.value = true;
  try {
    await createPentTask({
      project_id: form.value.project_id,
      name: form.value.name.trim(),
      description: form.value.description.trim(),
      priority: form.value.priority,
    });
    message.success('创建成功');
    showCreate.value = false;
    fetchTasks(true);
  } catch {
    message.error('创建失败');
  } finally {
    creating.value = false;
  }
}

onMounted(() => {
  fetchProjects();
  fetchTasks();
});
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <a-page-header title="任务管理" sub-title="创建、执行与取消安全测试任务">
      <template #extra>
        <span style="font-size: 13px; color: var(--ant-color-text-secondary); margin-right: 4px">
          自动刷新
        </span>
        <a-switch v-model:checked="autoRefresh" size="small" />
        <a-button style="margin-left: 12px" :loading="loading" @click="fetchTasks()">刷新</a-button>
        <a-button type="primary" style="margin-left: 8px" @click="openCreate">新建任务</a-button>
      </template>
    </a-page-header>

    <!-- 筛选 -->
    <a-card size="small" style="margin-top: 16px">
      <div style="display: flex; gap: 12px; flex-wrap: wrap">
        <a-select v-model:value="statusFilter" :options="statusOptions" style="width: 140px" />
        <a-input-search
          v-model:value="searchText"
          placeholder="搜索任务名称 / ID..."
          allow-clear
          style="width: 260px"
        />
        <span style="margin-left: auto; font-size: 13px; color: var(--ant-color-text-tertiary); align-self: center">
          共 {{ filtered.length }} 个任务
        </span>
      </div>
    </a-card>

    <!-- 任务卡片网格 -->
    <a-spin :spinning="loading">
      <a-row :gutter="[16, 16]" style="margin-top: 16px">
        <a-col v-for="t in paged" :key="t.id" :xs="24" :sm="12" :lg="8" :xl="6">
          <a-card hoverable size="small">
            <template #title>
              <span style="font-size: 13px">{{ t.name || '未命名任务' }}</span>
            </template>
            <template #extra>
              <a-tag :color="statusColor[t.status] || 'default'">
                {{ statusLabel[t.status] || t.status || '—' }}
              </a-tag>
            </template>
            <div style="font-size: 12px">
              <div style="display: flex; flex-wrap: wrap; gap: 4px; margin-bottom: 6px">
                <a-tag color="blue">{{ projectName(t.project_id) }}</a-tag>
                <a-tag :color="priorityColor[t.priority] || 'default'">
                  优先级：{{ priorityLabel[t.priority] || t.priority || '—' }}
                </a-tag>
              </div>
              <div style="color: var(--ant-color-text-tertiary); min-height: 18px">
                {{ (t.description || '').slice(0, 60) || '（无描述）' }}
              </div>
              <div style="margin-top: 4px; color: var(--ant-color-text-tertiary); font-family: monospace; font-size: 11px">
                {{ t.id?.slice(0, 18) }}
              </div>
              <div style="margin-top: 4px; color: var(--ant-color-text-tertiary)">
                创建时间：{{ fmtTime(t.created_at) }}
              </div>
              <div
                style="
                  margin-top: 8px;
                  border-top: 1px solid var(--ant-color-border-secondary);
                  padding-top: 8px;
                "
              >
                <a-button size="small" type="link" @click="goMonitor(t)">查看监控</a-button>
                <a-popconfirm
                  v-if="canCancel(t)"
                  title="确定取消该任务？"
                  ok-text="取消任务"
                  cancel-text="再想想"
                  @confirm="doCancel(t)"
                >
                  <a-button size="small" type="link" danger>取消</a-button>
                </a-popconfirm>
                <a-popconfirm
                  title="确定删除该任务？"
                  ok-text="删除"
                  ok-type="danger"
                  cancel-text="取消"
                  @confirm="doDelete(t)"
                >
                  <a-button size="small" type="link" danger>删除</a-button>
                </a-popconfirm>
              </div>
            </div>
          </a-card>
        </a-col>
      </a-row>
      <a-empty v-if="!paged.length && !loading" description="暂无任务" style="margin-top: 48px" />
    </a-spin>

    <div style="display: flex; justify-content: flex-end; margin-top: 16px">
      <a-pagination
        v-model:current="page"
        v-model:page-size="pageSize"
        :total="filtered.length"
        :page-size-options="['12', '24', '48']"
        show-size-changer
        :show-total="(t: number) => `共 ${t} 条`"
      />
    </div>

    <!-- 新建任务 modal -->
    <a-modal
      v-model:open="showCreate"
      title="新建任务"
      :confirm-loading="creating"
      width="520px"
      @ok="doCreate"
    >
      <a-form layout="vertical" :model="form">
        <a-form-item label="所属项目" required>
          <a-select
            v-model:value="form.project_id"
            placeholder="请选择项目"
            :options="projects.map((p) => ({ label: p.name, value: p.id }))"
          />
          <div v-if="projects.length === 0" style="margin-top: 6px; color: var(--ant-color-warning)">
            请先在项目管理中创建项目
          </div>
        </a-form-item>
        <a-form-item label="名称" required>
          <a-input v-model:value="form.name" placeholder="任务名称" />
        </a-form-item>
        <a-form-item label="描述">
          <a-textarea
            v-model:value="form.description"
            :rows="3"
            placeholder="任务目标 / 描述（可选）；资产范围自动使用所属项目配置"
          />
        </a-form-item>
        <a-form-item label="优先级">
          <a-select
            v-model:value="form.priority"
            :options="[
              { label: '低', value: 'low' },
              { label: '中', value: 'medium' },
              { label: '高', value: 'high' },
              { label: '紧急', value: 'critical' },
            ]"
          />
        </a-form-item>
      </a-form>
    </a-modal>
  </div>
</template>
