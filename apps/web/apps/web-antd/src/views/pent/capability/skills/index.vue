<script lang="ts" setup>
import type { PentSkill } from '#/api/pent';

import { computed, onMounted, ref } from 'vue';
import { message, Modal } from 'ant-design-vue';

import {
  createPentSkill,
  getPentSkill,
  getPentSkills,
  updatePentSkill,
} from '#/api/pent';

// ── list state ─────────────────────────────────────────────────────────────
const loading = ref(false);
const skills = ref<PentSkill[]>([]);
const searchKeyword = ref('');

const PAGE_SIZE_KEY = 'skillsPageSize';
function getSavedPageSize() {
  try {
    const size = Number.parseInt(localStorage.getItem(PAGE_SIZE_KEY) || '');
    if ([10, 20, 50, 100].includes(size)) return size;
  } catch {
    /* 忽略 */
  }
  return 20;
}
const pagination = ref({ current: 1, pageSize: getSavedPageSize() });

async function fetchSkills() {
  loading.value = true;
  try {
    skills.value = (await getPentSkills()) || [];
  } catch {
    message.error('加载技能列表失败');
  } finally {
    loading.value = false;
  }
}

const filtered = computed(() => {
  const q = searchKeyword.value.trim().toLowerCase();
  if (!q) return skills.value;
  return skills.value.filter(
    (s) =>
      (s.name || '').toLowerCase().includes(q) ||
      (s.title || '').toLowerCase().includes(q) ||
      (s.description || '').toLowerCase().includes(q),
  );
});

const pagedSkills = computed(() => {
  const { current, pageSize } = pagination.value;
  return filtered.value.slice((current - 1) * pageSize, current * pageSize);
});

// ── overview stats（对应原版 Skills / 当前页 / 文件 / 脚本 四张卡片）────────
const overview = computed(() => ({
  total: filtered.value.length,
  currentPageCount: pagedSkills.value.length,
}));

function onPageChange(page: number, pageSize: number) {
  pagination.value.current = page;
  pagination.value.pageSize = pageSize;
  try {
    localStorage.setItem(PAGE_SIZE_KEY, String(pageSize));
  } catch {
    /* 忽略 */
  }
}

function onSearch() {
  pagination.value.current = 1;
}

async function refreshSkills() {
  await fetchSkills();
  message.success('已刷新');
}

function fmtTime(t?: string) {
  return t ? t.slice(0, 19).replace('T', ' ') : '—';
}

const statusColor: Record<string, string> = {
  draft: 'default',
  reviewed: 'blue',
  published: 'green',
  disabled: 'red',
};
const statusLabel: Record<string, string> = {
  draft: '草稿',
  reviewed: '已审核',
  published: '已发布',
  disabled: '已停用',
};

// ── view modal ─────────────────────────────────────────────────────────────
const showView = ref(false);
const viewLoading = ref(false);
const viewSkill = ref<PentSkill | null>(null);

async function openView(s: PentSkill) {
  showView.value = true;
  viewLoading.value = true;
  viewSkill.value = s;
  try {
    viewSkill.value = await getPentSkill(s.id);
  } catch {
    message.error('加载技能详情失败');
  } finally {
    viewLoading.value = false;
  }
}

// ── create/edit modal ──────────────────────────────────────────────────────
const showEdit = ref(false);
const editingId = ref<null | string>(null);
const saving = ref(false);
const form = ref({ name: '', title: '', description: '', content: '' });

const NAME_RE = /^[a-z0-9]+(-[a-z0-9]+)*$/;

function openCreate() {
  editingId.value = null;
  form.value = { name: '', title: '', description: '', content: '' };
  showEdit.value = true;
}

function openEdit(s: PentSkill) {
  editingId.value = s.id;
  form.value = {
    name: s.name,
    title: s.title || s.name,
    description: s.description || '',
    content: s.content || '',
  };
  showEdit.value = true;
}

function openEditFromView() {
  if (!viewSkill.value) return;
  showView.value = false;
  openEdit(viewSkill.value);
}

async function doSave() {
  const name = form.value.name.trim();
  if (!name) {
    message.warning('技能名称不能为空');
    return;
  }
  if (!NAME_RE.test(name)) {
    message.error('名称须为小写字母、数字与连字符，如 sql-injection-testing');
    return;
  }
  if (!form.value.title.trim()) {
    message.warning('标题不能为空');
    return;
  }
  if (!editingId.value) {
    if (!form.value.description.trim()) {
      message.warning('描述不能为空（将写入 SKILL.md 的 front matter）');
      return;
    }
    if (!form.value.content.trim()) {
      message.warning('技能内容不能为空');
      return;
    }
  }
  saving.value = true;
  try {
    if (editingId.value) {
      await updatePentSkill(editingId.value, {
        title: form.value.title.trim(),
        description: form.value.description.trim(),
        content: form.value.content,
      });
      message.success('技能已更新');
    } else {
      await createPentSkill({
        name,
        title: form.value.title.trim(),
        description: form.value.description.trim(),
        content: form.value.content.trim(),
      });
      message.success('技能已创建');
    }
    showEdit.value = false;
    fetchSkills();
  } catch {
    message.error('保存技能失败');
  } finally {
    saving.value = false;
  }
}

function confirmDiscard() {
  Modal.confirm({
    title: '确定关闭？未保存的修改将丢失。',
    onOk: () => {
      showEdit.value = false;
    },
  });
}

onMounted(fetchSkills);
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <a-page-header title="技能管理">
      <template #extra>
        <a-button :loading="loading" @click="refreshSkills">刷新</a-button>
        <a-button type="primary" style="margin-left: 8px" @click="openCreate">
          创建 Skill
        </a-button>
      </template>
    </a-page-header>

    <!-- 统计卡片：Skills / 当前页 / 文件 / 脚本 -->
    <a-row :gutter="16" style="margin-top: 16px">
      <a-col :span="6">
        <a-card size="small">
          <a-statistic title="Skills" :value="overview.total" />
        </a-card>
      </a-col>
      <a-col :span="6">
        <a-card size="small">
          <a-statistic title="当前页" :value="overview.currentPageCount" />
        </a-card>
      </a-col>
      <a-col :span="6">
        <a-card size="small">
          <a-statistic title="文件" value="—" />
        </a-card>
      </a-col>
      <a-col :span="6">
        <a-card size="small">
          <a-statistic title="脚本" value="—" />
        </a-card>
      </a-col>
    </a-row>

    <!-- 搜索 -->
    <a-input
      v-model:value="searchKeyword"
      placeholder="搜索 Skills..."
      allow-clear
      style="margin-top: 16px"
      @change="onSearch"
    />

    <!-- 技能卡片列表 -->
    <a-spin :spinning="loading">
      <div style="margin-top: 16px; display: flex; flex-direction: column; gap: 12px">
        <a-card v-for="s in pagedSkills" :key="s.id" size="small">
          <div style="display: flex; justify-content: space-between; gap: 16px">
            <div style="flex: 1; min-width: 0">
              <div style="display: flex; align-items: center; flex-wrap: wrap; gap: 8px">
                <span style="font-size: 15px; font-weight: 600">{{ s.name }}</span>
                <a-tag :color="statusColor[s.status]" style="margin-inline-end: 0">
                  {{ statusLabel[s.status] || s.status }}
                </a-tag>
                <span style="color: var(--ant-color-text-tertiary); font-size: 12px">
                  v{{ s.version || '—' }}
                </span>
              </div>
              <div style="margin-top: 6px; color: var(--ant-color-text-secondary)">
                {{ s.description || '无描述' }}
              </div>
            </div>
            <div style="flex-shrink: 0; display: flex; align-items: flex-start; gap: 8px">
              <a-button size="small" @click="openView(s)">查看</a-button>
              <a-button size="small" @click="openEdit(s)">编辑</a-button>
            </div>
          </div>
        </a-card>
        <a-empty
          v-if="!pagedSkills.length && !loading"
          :description="searchKeyword ? '没有找到匹配的技能' : '暂无技能，点击「创建 Skill」创建第一个技能'"
          style="margin: 48px 0"
        />
      </div>
    </a-spin>

    <!-- 分页 -->
    <div style="margin-top: 16px; display: flex; justify-content: flex-end">
      <a-pagination
        :current="pagination.current"
        :page-size="pagination.pageSize"
        :total="filtered.length"
        :page-size-options="['10', '20', '50', '100']"
        show-size-changer
        :show-total="(total: number, range: [number, number]) => `显示 ${range[0]}-${range[1]} / 共 ${total} 条`"
        @change="onPageChange"
        @show-size-change="onPageChange"
      />
    </div>

    <!-- 查看弹窗 -->
    <a-modal
      v-model:open="showView"
      :title="`查看技能: ${viewSkill?.name || ''}`"
      width="900px"
      :footer="null"
    >
      <a-spin :spinning="viewLoading">
        <template v-if="viewSkill">
          <div style="margin-bottom: 8px">
            <strong>版本：</strong>{{ viewSkill.version || '—' }}
          </div>
          <div style="margin-bottom: 16px">
            <strong>描述：</strong>{{ viewSkill.description || '无描述' }}
          </div>
          <div style="margin-bottom: 8px">
            <strong>路径：</strong>—
          </div>
          <div style="margin-bottom: 16px">
            <strong>修改时间：</strong>{{ fmtTime(viewSkill.updated_at) }}
          </div>
          <div style="margin-bottom: 8px"><strong>内容：</strong></div>
          <pre
            style="
              padding: 16px;
              border-radius: 4px;
              overflow-x: auto;
              white-space: pre-wrap;
              word-wrap: break-word;
              max-height: 50vh;
              overflow-y: auto;
              border: 1px solid var(--ant-color-border);
              background: var(--ant-color-bg-layout);
              font-size: 12px;
            "
          >{{ viewSkill.content || '（无内容）' }}</pre>
        </template>
      </a-spin>
      <div style="margin-top: 16px; display: flex; justify-content: flex-end; gap: 8px">
        <a-button @click="showView = false">关闭</a-button>
        <a-button type="primary" @click="openEditFromView">编辑</a-button>
      </div>
    </a-modal>

    <!-- 新建/编辑弹窗 -->
    <a-modal
      v-model:open="showEdit"
      :title="editingId ? '编辑技能' : '添加技能'"
      width="720px"
      :confirm-loading="saving"
      ok-text="保存"
      cancel-text="取消"
      @ok="doSave"
      @cancel="confirmDiscard"
    >
      <a-form layout="vertical" :model="form">
        <a-form-item
          label="技能名称"
          required
          extra="小写字母、数字、连字符（与 Agent Skills 的 name 一致）"
        >
          <a-input
            v-model:value="form.name"
            :disabled="!!editingId"
            placeholder="例如: sql-injection-testing"
          />
        </a-form-item>
        <a-form-item label="标题" required>
          <a-input v-model:value="form.title" placeholder="例如: SQL 注入测试规程" />
        </a-form-item>
        <a-form-item
          label="描述"
          :required="!editingId"
          extra="对应 SKILL.md 中 YAML 的 description 字段"
        >
          <a-textarea
            v-model:value="form.description"
            :rows="2"
            placeholder="技能的简短描述"
          />
        </a-form-item>
        <a-form-item
          label="内容"
          :required="!editingId"
          :extra="
            editingId
              ? '当前为 SKILL.md 的正文部分；保存时会合并名称/描述生成标准 SKILL.md。'
              : 'SKILL.md 正文（无需手写 YAML 头），保存后生成标准 SKILL.md。'
          "
        >
          <a-textarea
            v-model:value="form.content"
            :rows="12"
            :placeholder="editingId ? '编辑技能正文…' : 'SKILL.md 正文（无需手写 YAML 头）…'"
          />
        </a-form-item>
      </a-form>
    </a-modal>
  </div>
</template>
