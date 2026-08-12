<script lang="ts" setup>
import type { PentAgent } from '#/api/pent';

import { onMounted, ref } from 'vue';
import { message } from 'ant-design-vue';

import {
  createPentAgent,
  deletePentAgent,
  getPentAgents,
  updatePentAgent,
} from '#/api/pent';

// ── state ──────────────────────────────────────────────────────────────────
const loading = ref(false);
const agents = ref<PentAgent[]>([]);
const agentsDir = ref('');

async function fetchAgents() {
  loading.value = true;
  try {
    const res = await getPentAgents();
    agents.value = res.agents || [];
    agentsDir.value = res.dir || '';
  } catch {
    /* 静默 */
  } finally {
    loading.value = false;
  }
}

async function doDelete(a: PentAgent) {
  try {
    await deletePentAgent(a.id);
    message.success('已删除');
    fetchAgents();
  } catch {
    message.error('删除失败');
  }
}

// ── modal ──────────────────────────────────────────────────────────────────
const showModal = ref(false);
const editingId = ref<string | null>(null);
const saving = ref(false);
const form = ref({ id: '', name: '', description: '', instruction: '' });

const ID_RE = /^[A-Za-z0-9_-]+$/;

function openCreate() {
  editingId.value = null;
  form.value = { id: '', name: '', description: '', instruction: '' };
  showModal.value = true;
}

function openEdit(a: PentAgent) {
  editingId.value = a.id;
  form.value = { id: a.id, name: a.name, description: a.description, instruction: a.instruction };
  showModal.value = true;
}

async function doSave() {
  if (!form.value.name) {
    message.warning('请填写显示名称');
    return;
  }
  if (!editingId.value) {
    if (!form.value.id || !ID_RE.test(form.value.id)) {
      message.error('ID 只能包含字母、数字、下划线与连字符');
      return;
    }
  }
  if (!form.value.instruction) {
    message.warning('请填写系统提示词正文');
    return;
  }
  saving.value = true;
  try {
    if (editingId.value) {
      await updatePentAgent(editingId.value, {
        name: form.value.name,
        description: form.value.description,
        instruction: form.value.instruction,
      });
      message.success('已保存');
    } else {
      await createPentAgent({
        id: form.value.id,
        name: form.value.name,
        description: form.value.description,
        instruction: form.value.instruction,
      });
      message.success('已创建');
    }
    showModal.value = false;
    fetchAgents();
  } catch {
    message.error('保存失败');
  } finally {
    saving.value = false;
  }
}

onMounted(fetchAgents);
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <a-page-header title="智能体管理">
      <template #extra>
        <a-button :loading="loading" @click="fetchAgents">刷新</a-button>
        <a-button type="primary" style="margin-left: 8px" @click="openCreate">新建智能体</a-button>
      </template>
    </a-page-header>

    <!-- 说明 -->
    <div style="margin-top: 16px; font-size: 13px; color: var(--ant-color-text-secondary)">
      智能体在 agents 目录下以 .md 维护：YAML front matter + 正文为系统提示词；主代理为 Deep 协调者，不参与
      task 子代理列表。
    </div>
    <div v-if="agentsDir" style="margin-top: 4px; font-size: 12px; color: var(--ant-color-text-tertiary)">
      目录：{{ agentsDir }}
    </div>

    <!-- 智能体列表 -->
    <a-spin :spinning="loading">
      <a-row :gutter="[16, 16]" style="margin-top: 16px">
        <a-col v-for="a in agents" :key="a.id" :xs="24" :sm="12" :lg="8" :xl="6">
          <a-card hoverable size="small">
            <template #title><span style="font-size: 13px">{{ a.name }}</span></template>
            <div style="font-size: 12px; color: var(--ant-color-text-tertiary); min-height: 54px">
              <div style="margin-bottom: 4px">
                <code>—</code>
                · id:
                <code>{{ a.id }}</code>
              </div>
              {{ (a.description || '').slice(0, 100) || '（暂无描述）' }}
            </div>
            <div style="margin-top: 8px; border-top: 1px solid var(--ant-color-border-secondary); padding-top: 8px">
              <a-button size="small" type="link" @click="openEdit(a)">编辑</a-button>
              <a-popconfirm :title="`确定删除 ${a.name} 吗？`" @confirm="doDelete(a)">
                <a-button size="small" type="link" danger>删除</a-button>
              </a-popconfirm>
            </div>
          </a-card>
        </a-col>
      </a-row>
      <a-empty
        v-if="!agents.length && !loading"
        description="暂无 Markdown 子智能体，点击「新建智能体」创建。"
        style="margin-top: 48px"
      />
    </a-spin>

    <!-- create/edit modal -->
    <a-modal
      v-model:open="showModal"
      :title="editingId ? '编辑智能体' : '新建智能体'"
      width="720px"
      :confirm-loading="saving"
      @ok="doSave"
    >
      <a-form layout="vertical" :model="form">
        <a-form-item label="Agent ID（留空则从名称生成）" required>
          <a-input v-model:value="form.id" :disabled="!!editingId" placeholder="code-reviewer" />
        </a-form-item>
        <a-form-item label="显示名称" required>
          <a-input v-model:value="form.name" placeholder="Code Reviewer" />
        </a-form-item>
        <a-form-item label="描述">
          <a-textarea v-model:value="form.description" :rows="2" placeholder="何时由协调者调度该子代理" />
        </a-form-item>
        <a-form-item label="系统提示词（Markdown 正文）" required>
          <a-textarea v-model:value="form.instruction" :rows="12" placeholder="You are a specialist agent..." />
        </a-form-item>
      </a-form>
    </a-modal>
  </div>
</template>
