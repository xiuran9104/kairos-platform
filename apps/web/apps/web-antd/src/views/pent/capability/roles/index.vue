<script lang="ts" setup>
import type { AiModelChatRoleApi } from '#/api/ai/model/chatRole';

import { computed, onMounted, ref } from 'vue';
import { message, Modal } from 'ant-design-vue';

import {
  createChatRole,
  deleteChatRole,
  getChatRolePage,
  updateChatRole,
} from '#/api/ai/model/chatRole';

type ChatRole = AiModelChatRoleApi.ChatRole;

// ── state ──────────────────────────────────────────────────────────────────
const loading = ref(false);
const roles = ref<ChatRole[]>([]);
const searchText = ref('');

async function fetchRoles(notify = false) {
  loading.value = true;
  try {
    const res = await getChatRolePage({ pageNo: 1, pageSize: 200 });
    roles.value = res.list || [];
    if (notify) message.success('已刷新');
  } catch {
    message.error('加载角色失败');
  } finally {
    loading.value = false;
  }
}

// 与原版一致：名为「默认」的角色排在第一个，其余按名称排序
const DEFAULT_ROLE_NAME = '默认';
function sortRoles(list: ChatRole[]) {
  const sorted = [...list];
  const defaultRole = sorted.find((r) => r.name === DEFAULT_ROLE_NAME);
  const others = sorted.filter((r) => r.name !== DEFAULT_ROLE_NAME);
  others.sort((a, b) => (a.name || '').localeCompare(b.name || '', 'zh-CN'));
  return defaultRole ? [defaultRole, ...others] : others;
}

const filtered = computed(() => {
  let list = roles.value;
  const q = searchText.value.trim().toLowerCase();
  if (q) {
    list = list.filter(
      (r) =>
        (r.name || '').toLowerCase().includes(q) ||
        (r.description || '').toLowerCase().includes(q),
    );
  }
  return sortRoles(list);
});

// 图标：兼容 Unicode 转义格式（\U0001F3C6），无配置时用默认图标
function roleIcon(role: ChatRole) {
  let icon = role.avatar || '👤';
  const m = icon.match(/^"?\\U([\dA-F]{8})"?$/i);
  if (m) {
    try {
      icon = String.fromCodePoint(Number.parseInt(m[1]!, 16));
    } catch {
      icon = '👤';
    }
  }
  return icon;
}

function roleDescription(role: ChatRole) {
  const raw = (role.description || '').trim();
  return raw || '无描述';
}

// 卡片上的工具摘要：默认角色 / 未配置工具清单时视为「使用所有工具」
function toolsDisplay(role: ChatRole) {
  if (role.name === DEFAULT_ROLE_NAME) return '使用所有工具';
  const count = role.toolIds?.length || 0;
  return count > 0 ? `等 ${count} 个` : '使用所有工具';
}

const isEnabled = (role: ChatRole) => role.status === 0;

// ── actions ────────────────────────────────────────────────────────────────
function doDelete(role: ChatRole) {
  if (role.name === DEFAULT_ROLE_NAME) {
    message.error('不能删除默认角色');
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
        await deleteChatRole(role.id);
        message.success('角色已删除');
        fetchRoles();
      } catch {
        message.error('删除角色失败');
      }
    },
  });
}

// ── modal ──────────────────────────────────────────────────────────────────
const showModal = ref(false);
const editing = ref<ChatRole | null>(null);
const saving = ref(false);
const form = ref({
  name: '',
  description: '',
  avatar: '',
  systemMessage: '',
  status: true,
});

function openCreate() {
  editing.value = null;
  form.value = {
    name: '',
    description: '',
    avatar: '',
    systemMessage: '',
    status: true,
  };
  showModal.value = true;
}

function openEdit(role: ChatRole) {
  editing.value = role;
  form.value = {
    name: role.name,
    description: role.description || '',
    avatar: roleIcon(role) === '👤' && !role.avatar ? '' : roleIcon(role),
    systemMessage: role.systemMessage || '',
    status: isEnabled(role),
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
      avatar: form.value.avatar.trim(),
      systemMessage: form.value.systemMessage.trim(),
      status: form.value.status ? 0 : 1,
    } as unknown as ChatRole;
    if (editing.value) {
      await updateChatRole({ ...editing.value, ...payload });
      message.success('角色已更新');
    } else {
      await createChatRole(payload);
      message.success('角色已创建');
    }
    showModal.value = false;
    fetchRoles();
  } catch {
    message.error('保存角色失败');
  } finally {
    saving.value = false;
  }
}

onMounted(() => {
  fetchRoles();
});
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <!-- 页面标题区：标题 + 刷新/创建角色 -->
    <a-page-header title="角色管理">
      <template #extra>
        <a-button :loading="loading" @click="fetchRoles(true)">刷新</a-button>
        <a-button type="primary" style="margin-left: 8px" @click="openCreate">创建角色</a-button>
      </template>
    </a-page-header>

    <!-- 搜索栏 -->
    <div style="margin-top: 16px">
      <a-input
        v-model:value="searchText"
        placeholder="搜索角色..."
        allow-clear
        style="width: 100%"
      />
    </div>

    <!-- 角色卡片网格 -->
    <a-spin :spinning="loading">
      <a-row v-if="filtered.length" :gutter="[16, 16]" style="margin-top: 16px">
        <a-col v-for="r in filtered" :key="r.id" :xs="24" :sm="12" :lg="8" :xl="6">
          <a-card size="small" :bordered="true">
            <!-- 卡片头：图标 + 名称 + 启用徽标 -->
            <div style="display: flex; align-items: center; justify-content: space-between; gap: 8px">
              <div style="display: flex; align-items: center; gap: 8px; min-width: 0">
                <span style="font-size: 20px; flex-shrink: 0">{{ roleIcon(r) }}</span>
                <span style="font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap">
                  {{ r.name }}
                </span>
              </div>
              <a-tag :color="isEnabled(r) ? 'green' : 'default'" style="margin-inline-end: 0; flex-shrink: 0">
                {{ isEnabled(r) ? '已启用' : '已禁用' }}
              </a-tag>
            </div>
            <!-- 描述 -->
            <div
              style="margin-top: 8px; font-size: 12px; color: var(--ant-color-text-tertiary); min-height: 36px; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical"
            >
              {{ roleDescription(r) }}
            </div>
            <!-- 工具摘要 -->
            <div style="margin-top: 8px; font-size: 12px">
              <span style="color: var(--ant-color-text-tertiary)">工具：</span>
              <span>{{ toolsDisplay(r) }}</span>
            </div>
            <!-- 操作 -->
            <div style="margin-top: 8px; border-top: 1px solid var(--ant-color-border-secondary); padding-top: 8px">
              <a-button size="small" type="link" @click="openEdit(r)">编辑</a-button>
              <a-button
                v-if="r.name !== DEFAULT_ROLE_NAME"
                size="small"
                type="link"
                danger
                @click="doDelete(r)"
              >
                删除
              </a-button>
            </div>
          </a-card>
        </a-col>
      </a-row>
      <!-- 空态 -->
      <div
        v-else-if="!loading"
        style="margin-top: 16px; border: 1px dashed var(--ant-color-border); border-radius: 8px; padding: 48px 24px; text-align: center; color: var(--ant-color-text-tertiary); max-width: 360px"
      >
        {{ searchText ? '没有找到匹配的角色' : '暂无角色' }}
      </div>
    </a-spin>

    <!-- 添加/编辑角色弹窗 -->
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
          <a-input
            v-model:value="form.name"
            placeholder="输入角色名称"
            :disabled="!!editing"
          />
        </a-form-item>
        <a-form-item label="角色描述">
          <a-input v-model:value="form.description" placeholder="输入角色描述" />
        </a-form-item>
        <a-form-item label="角色图标">
          <a-input
            v-model:value="form.avatar"
            placeholder="输入emoji图标，例如: 🏆"
            :maxlength="10"
            style="width: 240px"
          />
          <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-top: 4px">
            输入一个emoji作为角色的图标，将显示在角色选择器中。
          </div>
        </a-form-item>
        <a-form-item label="用户提示词">
          <a-textarea
            v-model:value="form.systemMessage"
            :rows="8"
            placeholder="输入用户提示词，会在用户消息前追加此提示词..."
          />
          <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-top: 4px">
            此提示词会追加到用户消息前，用于指导AI的行为。注意：这不会修改系统提示词。
          </div>
        </a-form-item>
        <a-form-item>
          <a-checkbox v-model:checked="form.status">启用此角色</a-checkbox>
        </a-form-item>
      </a-form>
    </a-modal>
  </div>
</template>
