<script lang="ts" setup>
import type { PentAgentRole } from '#/api/pent/roles';

import { computed, onMounted, ref } from 'vue';
import { message, Modal } from 'ant-design-vue';

import { getPentTools } from '#/api/pent';
import {
  createPentRole,
  deletePentRole,
  getPentRoles,
  updatePentRole,
  updatePentRoleStatus,
} from '#/api/pent/roles';

const loading = ref(false);
const roles = ref<PentAgentRole[]>([]);
const tools = ref<any[]>([]);
const searchText = ref('');

async function loadTools() {
  try {
    tools.value = (await getPentTools()) as any[];
  } catch {
    tools.value = [];
  }
}

async function fetchRoles(notify = false) {
  loading.value = true;
  try {
    roles.value = (await getPentRoles()) as PentAgentRole[];
    if (notify) message.success('已刷新');
  } catch {
    message.error('加载角色失败');
  } finally {
    loading.value = false;
  }
}

const filtered = computed(() => {
  const q = searchText.value.trim().toLowerCase();
  let list = roles.value;
  if (q) {
    list = list.filter(
      (r) =>
        (r.name || '').toLowerCase().includes(q) ||
        (r.id || '').toLowerCase().includes(q) ||
        (r.description || '').toLowerCase().includes(q),
    );
  }
  return [...list].sort((a, b) => {
    if (a.is_configured_default !== b.is_configured_default) return a.is_configured_default ? -1 : 1;
    if (a.is_default !== b.is_default) return a.is_default ? -1 : 1;
    if (a.tenant_id !== b.tenant_id) return a.tenant_id - b.tenant_id;
    return (a.name || '').localeCompare(b.name || '', 'zh-CN');
  });
});

function toolCount(role: PentAgentRole) {
  const count = role.tool_whitelist?.length || 0;
  return count > 0 ? `${count} 个工具` : '全部工具';
}

function doDelete(role: PentAgentRole) {
  if (role.is_default || role.tenant_id === 0) {
    message.error('平台预设角色或默认角色不能删除');
    return;
  }
  Modal.confirm({
    title: '删除角色',
    content: `确定要删除角色“${role.name}”吗？此操作不可撤销。`,
    okText: '删除',
    okButtonProps: { danger: true },
    cancelText: '取消',
    async onOk() {
      try {
        await deletePentRole(role.id);
        message.success('角色已删除');
        fetchRoles();
      } catch (error: any) {
        message.error(error?.message || '删除角色失败');
      }
    },
  });
}

async function toggleEnabled(role: PentAgentRole) {
  if (role.tenant_id === 0) {
    message.error('平台预设角色不能修改');
    return;
  }
  try {
    await updatePentRoleStatus(role.id, !role.enabled);
    message.success(role.enabled ? '已禁用' : '已启用');
    fetchRoles();
  } catch (error: any) {
    message.error(error?.message || '操作失败');
  }
}

// ── modal ──────────────────────────────────────────────────────────────────
const showModal = ref(false);
const editing = ref<PentAgentRole | null>(null);
const saving = ref(false);
const form = ref({
  name: '',
  description: '',
  icon: '',
  user_prompt: '',
  tool_whitelist: [] as string[],
  enabled: true,
});

function openCreate() {
  editing.value = null;
  form.value = { name: '', description: '', icon: '', user_prompt: '', tool_whitelist: [], enabled: true };
  showModal.value = true;
}

function openEdit(role: PentAgentRole) {
  editing.value = role;
  form.value = {
    name: role.name,
    description: role.description || '',
    icon: role.icon || '',
    user_prompt: role.user_prompt || '',
    tool_whitelist: [...(role.tool_whitelist || [])],
    enabled: role.enabled,
  };
  showModal.value = true;
}

async function doSave() {
  const name = form.value.name.trim();
  if (!name) {
    message.warning('角色名称不能为空');
    return;
  }
  saving.value = true;
  try {
    const payload = {
      name,
      description: form.value.description.trim(),
      icon: form.value.icon.trim(),
      user_prompt: form.value.user_prompt.trim(),
      tool_whitelist: form.value.tool_whitelist,
      enabled: form.value.enabled,
    };
    if (editing.value) {
      await updatePentRole(editing.value.id, payload);
      message.success('角色已更新');
    } else {
      await createPentRole(payload);
      message.success('角色已创建');
    }
    showModal.value = false;
    fetchRoles();
  } catch (error: any) {
    message.error(error?.message || '保存角色失败');
  } finally {
    saving.value = false;
  }
}

const toolOptions = computed(() =>
  tools.value.map((t: any) => ({ label: t.name, value: t.name })),
);

onMounted(async () => {
  await Promise.all([fetchRoles(), loadTools()]);
});
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <a-page-header title="角色管理" sub-title="角色 = 提示词预设 + 工具白名单，切换角色后模型提示词与工具面随动">
      <template #extra>
        <a-button :loading="loading" @click="fetchRoles(true)">刷新</a-button>
        <a-button type="primary" style="margin-left: 8px" @click="openCreate">创建角色</a-button>
      </template>
    </a-page-header>

    <a-alert
      type="info"
      show-icon
      message="这里管理主对话角色；多 Agent 执行中的专家智能体请到“智能体管理”查看。带“当前平台默认”的角色会在项目和对话未单独指定时生效。"
      style="margin-top: 16px"
    />

    <a-input
      v-model:value="searchText"
      placeholder="搜索角色..."
      allow-clear
      style="margin-top: 16px; width: 100%"
    />

    <a-spin :spinning="loading">
      <a-row v-if="filtered.length" :gutter="[16, 16]" style="margin-top: 16px">
        <a-col v-for="r in filtered" :key="r.id" :xs="24" :sm="12" :lg="8" :xl="6">
          <a-card size="small" :bordered="true">
            <div style="display: flex; align-items: center; justify-content: space-between; gap: 8px">
              <div style="display: flex; align-items: center; gap: 8px; min-width: 0">
                <span style="font-size: 20px; flex-shrink: 0">{{ r.icon || '🎯' }}</span>
                <span style="font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap">
                  {{ r.name }}
                </span>
              </div>
              <div style="display: flex; gap: 4px; flex-shrink: 0; align-items: center">
                <a-tag>主对话角色</a-tag>
                <a-tag v-if="r.is_configured_default" color="blue">当前平台默认</a-tag>
                <a-tag v-else-if="r.is_default" color="cyan">系统兜底</a-tag>
                <a-tag v-else-if="r.tenant_id === 0" color="purple">预设</a-tag>
                <a-tag :color="r.enabled ? 'green' : 'default'">{{ r.enabled ? '已启用' : '已禁用' }}</a-tag>
              </div>
            </div>
            <div
              style="
                margin-top: 8px;
                font-size: 12px;
                color: var(--ant-color-text-tertiary);
                min-height: 36px;
                overflow: hidden;
                display: -webkit-box;
                -webkit-line-clamp: 2;
                -webkit-box-orient: vertical;
              "
            >
              {{ r.description || '无描述' }}
            </div>
            <div style="margin-top: 8px; font-size: 12px">
              <span style="color: var(--ant-color-text-tertiary)">角色 ID：</span>
              <code>{{ r.id }}</code>
            </div>
            <div style="margin-top: 8px; font-size: 12px">
              <span style="color: var(--ant-color-text-tertiary)">工具：</span>
              <span>{{ toolCount(r) }}</span>
            </div>
            <div style="margin-top: 4px; font-size: 12px; color: var(--ant-color-text-secondary)">
              显式对话 {{ r.explicit_session_count || 0 }} · 项目 {{ r.project_count || 0 }} · 历史运行 {{ r.historical_run_count || 0 }}
            </div>
            <div style="margin-top: 8px; border-top: 1px solid var(--ant-color-border-secondary); padding-top: 8px">
              <a-button size="small" type="link" @click="openEdit(r)">编辑</a-button>
              <a-button v-if="r.tenant_id !== 0" size="small" type="link" @click="toggleEnabled(r)">
                {{ r.enabled ? '禁用' : '启用' }}
              </a-button>
              <a-button v-if="r.tenant_id !== 0 && !r.is_default" size="small" type="link" danger @click="doDelete(r)">
                删除
              </a-button>
            </div>
          </a-card>
        </a-col>
      </a-row>
      <div
        v-else-if="!loading"
        style="
          margin-top: 16px;
          border: 1px dashed var(--ant-color-border);
          border-radius: 8px;
          padding: 48px 24px;
          text-align: center;
          color: var(--ant-color-text-tertiary);
          max-width: 360px;
        "
      >
        {{ searchText ? '没有找到匹配的角色' : '暂无角色' }}
      </div>
    </a-spin>

    <a-modal
      v-model:open="showModal"
      :title="editing ? '编辑角色' : '添加角色'"
      width="720px"
      :confirm-loading="saving"
      ok-text="保存"
      cancel-text="取消"
      @ok="doSave"
    >
      <a-form layout="vertical" :model="form">
        <a-form-item label="角色名称" required>
          <a-input v-model:value="form.name" placeholder="输入角色名称" />
        </a-form-item>
        <a-form-item label="角色描述">
          <a-input v-model:value="form.description" placeholder="输入角色描述" />
        </a-form-item>
        <a-form-item label="角色图标">
          <a-input v-model:value="form.icon" placeholder="输入emoji图标，例如: 🎯" :maxlength="10" style="width: 240px" />
        </a-form-item>
        <a-form-item label="提示词预设">
          <a-textarea
            v-model:value="form.user_prompt"
            :rows="6"
            placeholder="输入角色提示词，会作为角色设定注入系统指令，引导AI采用特定的测试方法"
          />
        </a-form-item>
        <a-form-item label="工具白名单">
          <a-select
            v-model:value="form.tool_whitelist"
            mode="multiple"
            :options="toolOptions"
            show-search
            option-filter-prop="label"
            allow-clear
            placeholder="留空 = 使用全部可用工具"
            style="width: 100%"
          />
          <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-top: 4px">
            只在该角色下暴露所选工具；留空则暴露全部已启用工具。
          </div>
        </a-form-item>
        <a-form-item>
          <a-checkbox v-model:checked="form.enabled">启用此角色</a-checkbox>
        </a-form-item>
      </a-form>
    </a-modal>
  </div>
</template>
