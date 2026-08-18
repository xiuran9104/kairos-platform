<script lang="ts" setup>
import type { AiModelModelApi } from '#/api/ai/model/model';
import type { PentAgentRole } from '#/api/pent/roles';

import { onMounted, ref, watch } from 'vue';
import { AiModelTypeEnum } from '@vben/constants';
import { message } from 'ant-design-vue';

import { getModelSimpleList } from '#/api/ai/model/model';
import {
  getPentRoles,
  updatePentProject,
  updatePentProjectAuthorization,
  updatePentProjectModel,
} from '#/api/pent';

const props = defineProps<{ project: Record<string, any> }>();
const emit = defineEmits<{ saved: [] }>();

const saving = ref(false);
const modelsLoading = ref(false);
const models = ref<AiModelModelApi.Model[]>([]);
const rolesLoading = ref(false);
const roles = ref<PentAgentRole[]>([]);
const form = ref({
  assetScopeText: '',
  description: '',
  defaultRoleId: '',
  engagementMode: 'assessment',
  modelConfigId: undefined as number | undefined,
  name: '',
  status: 'active',
  authorizationExpiresAt: '',
  rulesOfEngagement: '{}',
});

function resetForm() {
  const project = props.project;
  form.value = {
    assetScopeText: Array.isArray(project.asset_scope)
      ? project.asset_scope.join('\n')
      : '',
    description: project.description || '',
    defaultRoleId: project.default_role_id || '',
    engagementMode: project.engagement_mode || 'assessment',
    modelConfigId: project.model_config_id || undefined,
    name: project.name || '',
    status: project.status || 'active',
    authorizationExpiresAt: project.authorization_expires_at
      ? project.authorization_expires_at.slice(0, 16)
      : '',
    rulesOfEngagement: JSON.stringify(project.rules_of_engagement || {}, null, 2),
  };
}

async function loadModels() {
  modelsLoading.value = true;
  try {
    models.value = (await getModelSimpleList(AiModelTypeEnum.CHAT)) || [];
  } catch {
    models.value = [];
  } finally {
    modelsLoading.value = false;
  }
}

async function loadRoles() {
  rolesLoading.value = true;
  try {
    roles.value = ((await getPentRoles()) || []).filter((role) => role.enabled);
  } catch {
    roles.value = [];
  } finally {
    rolesLoading.value = false;
  }
}

async function save() {
  const name = form.value.name.trim();
  if (!name) {
    message.warning('请输入项目名称');
    return;
  }
  const assetScope = form.value.assetScopeText
    .split(/[\n,]/)
    .map((item) => item.trim())
    .filter(Boolean);
  saving.value = true;
  try {
    let rulesOfEngagement: Record<string, any> = {};
    try {
      rulesOfEngagement = JSON.parse(form.value.rulesOfEngagement || '{}');
    } catch {
      message.warning('交战规则必须是合法 JSON');
      return;
    }
    await updatePentProject(props.project.id, {
      asset_scope: assetScope,
      description: form.value.description.trim(),
      default_role_id: form.value.defaultRoleId || null,
      engagement_mode: form.value.engagementMode,
      name,
      rules_of_engagement: rulesOfEngagement,
      status: form.value.status,
    });
    if (
      form.value.modelConfigId &&
      form.value.modelConfigId !== props.project.model_config_id
    ) {
      await updatePentProjectModel(props.project.id, form.value.modelConfigId);
    }
    message.success('项目设置已保存');
    emit('saved');
  } catch (error: any) {
    message.error(error?.message || '保存项目设置失败');
  } finally {
    saving.value = false;
  }
}

async function authorizeRedTeam(confirmed: boolean) {
  if (form.value.engagementMode !== 'red_team') return;
  if (confirmed && !form.value.authorizationExpiresAt) {
    message.warning('请设置授权截止时间');
    return;
  }
  let rulesOfEngagement: Record<string, any> = {};
  try {
    rulesOfEngagement = JSON.parse(form.value.rulesOfEngagement || '{}');
  } catch {
    message.warning('交战规则必须是合法 JSON');
    return;
  }
  saving.value = true;
  try {
    await updatePentProjectAuthorization(props.project.id, {
      confirmed,
      expires_at: confirmed
        ? new Date(form.value.authorizationExpiresAt).toISOString()
        : undefined,
      rules_of_engagement: rulesOfEngagement,
    });
    message.success(confirmed ? '红队 Engagement 已授权' : '红队授权已撤销');
    emit('saved');
  } catch (error: any) {
    message.error(error?.message || '更新授权失败');
  } finally {
    saving.value = false;
  }
}

watch(() => props.project, resetForm, { deep: true, immediate: true });
onMounted(() => Promise.all([loadModels(), loadRoles()]));
</script>

<template>
  <a-form layout="vertical" :model="form" style="max-width: 680px">
    <a-alert
      message="项目设置会作为新对话和 Agent 执行的默认上下文"
      type="info"
      show-icon
      style="margin-bottom: 16px"
    />
    <a-row :gutter="16">
      <a-col :xs="24" :md="16">
        <a-form-item label="项目名称" required>
          <a-input v-model:value="form.name" />
        </a-form-item>
      </a-col>
      <a-col :xs="24" :md="8">
        <a-form-item label="项目状态" required>
          <a-select
            v-model:value="form.status"
            :options="[
              { label: '进行中', value: 'active' },
              { label: '已暂停', value: 'paused' },
              { label: '已归档', value: 'archived' },
            ]"
          />
        </a-form-item>
      </a-col>
    </a-row>
    <a-form-item label="项目描述">
      <a-textarea v-model:value="form.description" :rows="3" />
    </a-form-item>
    <a-form-item label="授权资产范围" required extra="每行一个域名、IP、CIDR 或 URL；创建项目对话时应使用此范围">
      <a-textarea
        v-model:value="form.assetScopeText"
        :rows="5"
        placeholder="example.com&#10;10.0.0.0/24&#10;https://app.example.com"
      />
    </a-form-item>
    <a-form-item label="执行模式">
      <a-radio-group v-model:value="form.engagementMode">
        <a-radio-button value="assessment">安全评估</a-radio-button>
        <a-radio-button value="red_team">授权红队</a-radio-button>
      </a-radio-group>
    </a-form-item>
    <template v-if="form.engagementMode === 'red_team'">
      <a-alert
        :message="project.authorization_status === 'authorized' ? '红队授权生效中' : '红队模式尚未授权，Run 将被拒绝'"
        :type="project.authorization_status === 'authorized' ? 'success' : 'warning'"
        show-icon
        style="margin-bottom: 16px"
      />
      <a-form-item label="授权截止时间" required>
        <a-input v-model:value="form.authorizationExpiresAt" type="datetime-local" />
      </a-form-item>
      <a-form-item label="交战规则（JSON）" extra="记录时间窗口、禁止动作、速率限制和允许的验证深度">
        <a-textarea v-model:value="form.rulesOfEngagement" :rows="6" />
      </a-form-item>
    </template>
    <a-form-item label="默认对话模型" extra="未选择时由运行时使用平台默认聊天模型">
      <a-select
        v-model:value="form.modelConfigId"
        :loading="modelsLoading"
        show-search
        option-filter-prop="label"
        placeholder="选择已启用的聊天模型"
        :options="models.map((model) => ({
          label: `${model.name} · ${model.model}`,
          value: model.id,
        }))"
      />
    </a-form-item>
    <a-form-item label="默认 Agent 角色" extra="未选择时继承平台默认角色；对话仍可单独覆盖">
      <a-select
        v-model:value="form.defaultRoleId"
        :loading="rolesLoading"
        show-search
        option-filter-prop="label"
        :options="[
          { label: '继承平台默认角色', value: '' },
          ...roles.map((role) => ({
            label: `${role.icon || '🎯'} ${role.name} · ${role.id}`,
            value: role.id,
          })),
        ]"
      />
      <div style="margin-top: 4px; color: var(--ant-color-text-tertiary); font-size: 12px">
        当前实际生效：{{ project.resolved_role_icon || '🎯' }} {{ project.resolved_role_name || '—' }}
      </div>
    </a-form-item>
    <a-space>
      <a-button type="primary" :loading="saving" @click="save">保存设置</a-button>
      <a-button
        v-if="form.engagementMode === 'red_team'"
        type="primary"
        danger
        :loading="saving"
        @click="authorizeRedTeam(true)"
      >确认红队授权</a-button>
      <a-button
        v-if="form.engagementMode === 'red_team' && project.authorization_status === 'authorized'"
        :loading="saving"
        @click="authorizeRedTeam(false)"
      >撤销授权</a-button>
      <a-button @click="resetForm">撤销修改</a-button>
    </a-space>
  </a-form>
</template>
