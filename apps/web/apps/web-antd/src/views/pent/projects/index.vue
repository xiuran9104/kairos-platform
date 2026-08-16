<script lang="ts" setup>
import { computed, onMounted, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import { message } from 'ant-design-vue';

import {
  createPentProject,
  deletePentProject,
  getPentProjectFindings,
  getPentProjects,
  updatePentProject,
} from '#/api/pent';
import { getPentProjectStats, type PentProjectStats } from '#/api/pent/ops-facts';

import AttackPath from './modules/attack-path.vue';
import FactsBoard from './modules/facts-board.vue';
import ProjectAssets from './modules/project-assets.vue';
import ProjectConversations from './modules/project-conversations.vue';
import ProjectSettings from './modules/project-settings.vue';

const router = useRouter();

// ── state ──────────────────────────────────────────────────────────────────
const loading = ref(false);
const projects = ref<any[]>([]);
const searchText = ref('');
const selectedId = ref<string | null>(null);
const activeTab = ref('facts');
const showArchived = ref(false);
const stats = ref<PentProjectStats | null>(null);

async function fetchProjects() {
  loading.value = true;
  try {
    projects.value = ((await getPentProjects()) as any[]) || [];
    if (selectedId.value && !projects.value.some((p) => p.id === selectedId.value)) {
      selectedId.value = null;
    }
  } catch {
    /* 静默 */
  } finally {
    loading.value = false;
  }
}

const filteredProjects = computed(() => {
  let list = projects.value;
  if (!showArchived.value) {
    list = list.filter((p) => p.status !== 'archived');
  }
  if (!searchText.value) return list;
  const q = searchText.value.toLowerCase();
  return list.filter(
    (p) =>
      (p.name || '').toLowerCase().includes(q) ||
      (p.description || '').toLowerCase().includes(q),
  );
});

const current = computed(
  () => projects.value.find((p) => p.id === selectedId.value) || null,
);

const statusColor: Record<string, string> = {
  active: 'green',
  archived: 'default',
  paused: 'orange',
};
const statusLabel: Record<string, string> = {
  active: '进行中',
  archived: '已归档',
  paused: '已暂停',
};

function fmtTime(t?: string) {
  return t ? t.slice(0, 16).replace('T', ' ') : '—';
}

// ── create modal ───────────────────────────────────────────────────────────
const showCreate = ref(false);
const creating = ref(false);
const createForm = ref({ name: '', description: '' });

function openCreate() {
  createForm.value = { name: '', description: '' };
  showCreate.value = true;
}

async function doCreate() {
  if (!createForm.value.name.trim()) {
    message.warning('请输入项目名称');
    return;
  }
  creating.value = true;
  try {
    const resp: any = await createPentProject({
      name: createForm.value.name.trim(),
      description: createForm.value.description.trim(),
    });
    message.success('项目已创建');
    showCreate.value = false;
    await fetchProjects();
    if (resp?.id) selectedId.value = resp.id;
  } catch {
    message.error('创建失败');
  } finally {
    creating.value = false;
  }
}

// ── edit / settings ────────────────────────────────────────────────────────
const showEdit = ref(false);
const saving = ref(false);
const editForm = ref({ name: '', description: '' });

function openEdit() {
  if (!current.value) return;
  editForm.value = {
    name: current.value.name || '',
    description: current.value.description || '',
  };
  showEdit.value = true;
}

// 选中项目变化时同步设置表单
watch(
  current,
  (p) => {
    editForm.value = { name: p?.name || '', description: p?.description || '' };
  },
  { immediate: true },
);

async function doUpdate() {
  if (!current.value) return;
  if (!editForm.value.name.trim()) {
    message.warning('请输入项目名称');
    return;
  }
  saving.value = true;
  try {
    await updatePentProject(current.value.id, {
      name: editForm.value.name.trim(),
      description: editForm.value.description.trim(),
      asset_scope: Array.isArray(current.value.asset_scope)
        ? current.value.asset_scope
        : [],
    });
    message.success('已保存');
    showEdit.value = false;
    await fetchProjects();
  } catch {
    message.error('保存失败');
  } finally {
    saving.value = false;
  }
}

async function doDelete() {
  if (!current.value) return;
  try {
    await deletePentProject(current.value.id);
    message.success('已删除');
    selectedId.value = null;
    await fetchProjects();
  } catch {
    message.error('删除失败');
  }
}

// ── findings tab ───────────────────────────────────────────────────────────
const findingsLoading = ref(false);
const findings = ref<any[]>([]);

const sevColor: Record<string, string> = {
  critical: 'red',
  high: 'orange',
  medium: 'gold',
  low: 'blue',
  info: 'default',
};

const findingColumns = [
  { title: '标题', dataIndex: 'title', key: 'title' },
  { title: '严重度', dataIndex: 'severity', key: 'severity', width: 100 },
  { title: '状态', dataIndex: 'status', key: 'status', width: 110 },
  { title: '创建时间', dataIndex: 'created_at', key: 'created_at', width: 170 },
];

async function fetchFindings(projectId: string) {
  findingsLoading.value = true;
  try {
    const resp: any = await getPentProjectFindings(projectId);
    findings.value = resp?.items ?? [];
  } catch {
    findings.value = [];
  } finally {
    findingsLoading.value = false;
  }
}

watch(
  selectedId,
  (id) => {
    findings.value = [];
    stats.value = null;
    if (id) {
      fetchFindings(id);
      getPentProjectStats(id)
        .then((s) => (stats.value = s))
        .catch(() => {});
    }
  },
);

onMounted(fetchProjects);
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <div style="display: flex; gap: 16px; align-items: stretch">
      <!-- 左侧项目列表 -->
      <a-card
        size="small"
        style="width: 280px; flex-shrink: 0; height: 100%; display: flex; flex-direction: column"
        :body-style="{ padding: '12px', flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }"
      >
        <div style="display: flex; gap: 8px; margin-bottom: 12px">
          <a-input-search
            v-model:value="searchText"
            placeholder="搜索项目..."
            allow-clear
            style="flex: 1"
          />
        </div>
        <a-button type="primary" block style="margin-bottom: 12px" @click="openCreate">
          新建项目
        </a-button>
        <a-checkbox v-model:checked="showArchived" style="margin-bottom: 8px; font-size: 12px">
          显示已归档
        </a-checkbox>
        <a-spin :spinning="loading" style="flex: 1; min-height: 0; display: flex; flex-direction: column">
          <div style="flex: 1; min-height: 0; overflow-y: auto">
            <div
              v-for="p in filteredProjects"
              :key="p.id"
              :style="{
                padding: '10px 12px',
                marginBottom: '8px',
                borderRadius: '6px',
                cursor: 'pointer',
                border:
                  p.id === selectedId
                    ? '1px solid var(--ant-color-primary)'
                    : '1px solid var(--ant-color-border-secondary)',
                background:
                  p.id === selectedId ? 'var(--ant-color-primary-bg)' : 'transparent',
              }"
              @click="selectedId = p.id"
            >
              <div style="display: flex; align-items: center; justify-content: space-between; gap: 8px">
                <span style="font-weight: 600; font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap">
                  {{ p.name }}
                </span>
                <a-tag :color="statusColor[p.status] || 'default'" style="margin-inline-end: 0">
                  {{ statusLabel[p.status] || p.status || '—' }}
                </a-tag>
              </div>
              <div
                style="
                  margin-top: 4px;
                  font-size: 12px;
                  color: var(--ant-color-text-tertiary);
                  overflow: hidden;
                  text-overflow: ellipsis;
                  white-space: nowrap;
                "
              >
                {{ p.description || '（无描述）' }}
              </div>
            </div>
            <a-empty
              v-if="!filteredProjects.length && !loading"
              description="暂无项目"
              style="margin-top: 24px"
            />
          </div>
        </a-spin>
      </a-card>

      <!-- 右侧详情区 -->
      <div style="flex: 1; min-width: 0">
        <template v-if="current">
          <a-page-header :title="current.name">
            <template #tags>
              <a-tag :color="statusColor[current.status] || 'default'">
                {{ statusLabel[current.status] || current.status || '—' }}
              </a-tag>
              <template v-if="stats">
                <a-tag color="blue">{{ stats.fact_count }} 事实</a-tag>
                <a-tag color="orange">{{ stats.vuln_count }} 漏洞</a-tag>
                <a-tag color="purple">{{ stats.conversation_count }} 对话</a-tag>
              </template>
            </template>
            <template #extra>
              <a-button @click="router.push('/pent/findings')">漏洞管理</a-button>
              <a-button
                type="primary"
                style="margin-left: 8px"
                @click="activeTab = 'facts'"
              >+ 新增事实</a-button>
              <a-button style="margin-left: 8px" @click="openEdit">编辑</a-button>
              <a-popconfirm
                title="确定删除该项目？"
                ok-text="删除"
                ok-type="danger"
                cancel-text="取消"
                @confirm="doDelete"
              >
                <a-button danger style="margin-left: 8px">删除</a-button>
              </a-popconfirm>
            </template>
            <div style="font-size: 13px; color: var(--ant-color-text-secondary)">
              <div>{{ current.description || '（无描述）' }}</div>
              <div style="margin-top: 4px; color: var(--ant-color-text-tertiary)">
                更新时间：{{ fmtTime(current.updated_at) }}
              </div>
            </div>
          </a-page-header>

          <a-card size="small" style="margin-top: 16px">
            <a-tabs v-model:activeKey="activeTab">
              <a-tab-pane key="facts" tab="事实黑板">
                <FactsBoard :project-id="current.id" />
              </a-tab-pane>
              <a-tab-pane key="paths" tab="攻击路径">
                <AttackPath :project-id="current.id" />
              </a-tab-pane>
              <a-tab-pane key="assets" tab="资产库">
                <ProjectAssets :project-id="current.id" />
              </a-tab-pane>
              <a-tab-pane key="chats" tab="关联对话">
                <ProjectConversations :project-id="current.id" />
              </a-tab-pane>
              <a-tab-pane key="findings" tab="关联漏洞">
                <a-table
                  :columns="findingColumns"
                  :data-source="findings"
                  :loading="findingsLoading"
                  row-key="id"
                  size="middle"
                  :pagination="{ pageSize: 10, showTotal: (t: number) => `共 ${t} 条` }"
                  :custom-row="
                    () => ({
                      style: 'cursor: pointer',
                      onClick: () => router.push('/pent/findings'),
                    })
                  "
                >
                  <template #bodyCell="{ column, record }">
                    <template v-if="column.key === 'severity'">
                      <a-tag :color="sevColor[record.severity] || 'default'">
                        {{ record.severity || 'info' }}
                      </a-tag>
                    </template>
                    <template v-if="column.key === 'created_at'">
                      {{ fmtTime(record.created_at) }}
                    </template>
                  </template>
                  <template #emptyText>
                    <a-empty description="暂无关联漏洞" />
                  </template>
                </a-table>
              </a-tab-pane>
              <a-tab-pane key="settings" tab="设置">
                <ProjectSettings :project="current" @saved="fetchProjects" />
              </a-tab-pane>
            </a-tabs>
          </a-card>
        </template>
        <a-card v-else size="small">
          <a-empty description="请选择左侧项目查看详情" style="margin: 96px 0" />
        </a-card>
      </div>
    </div>

    <!-- 新建项目 modal -->
    <a-modal
      v-model:open="showCreate"
      title="新建项目"
      :confirm-loading="creating"
      @ok="doCreate"
    >
      <a-form layout="vertical" :model="createForm">
        <a-form-item label="名称" required>
          <a-input v-model:value="createForm.name" placeholder="项目名称" />
        </a-form-item>
        <a-form-item label="描述">
          <a-textarea v-model:value="createForm.description" :rows="3" placeholder="项目描述（可选）" />
        </a-form-item>
      </a-form>
    </a-modal>

    <!-- 编辑项目 modal -->
    <a-modal
      v-model:open="showEdit"
      title="编辑项目"
      :confirm-loading="saving"
      @ok="doUpdate"
    >
      <a-form layout="vertical" :model="editForm">
        <a-form-item label="名称" required>
          <a-input v-model:value="editForm.name" placeholder="项目名称" />
        </a-form-item>
        <a-form-item label="描述">
          <a-textarea v-model:value="editForm.description" :rows="3" placeholder="项目描述" />
        </a-form-item>
      </a-form>
    </a-modal>
  </div>
</template>
