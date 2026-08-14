<script lang="ts" setup>
import type { AiModelModelApi } from '#/api/ai/model/model';

import { onMounted, ref, watch } from 'vue';
import { AiModelTypeEnum } from '@vben/constants';
import { message } from 'ant-design-vue';

import { getModelSimpleList } from '#/api/ai/model/model';
import { updatePentProject, updatePentProjectModel } from '#/api/pent';

const props = defineProps<{ project: Record<string, any> }>();
const emit = defineEmits<{ saved: [] }>();

const saving = ref(false);
const modelsLoading = ref(false);
const models = ref<AiModelModelApi.Model[]>([]);
const form = ref({
  assetScopeText: '',
  description: '',
  modelConfigId: undefined as number | undefined,
  name: '',
  status: 'active',
});

function resetForm() {
  const project = props.project;
  form.value = {
    assetScopeText: Array.isArray(project.asset_scope)
      ? project.asset_scope.join('\n')
      : '',
    description: project.description || '',
    modelConfigId: project.model_config_id || undefined,
    name: project.name || '',
    status: project.status || 'active',
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
    await updatePentProject(props.project.id, {
      asset_scope: assetScope,
      description: form.value.description.trim(),
      name,
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

watch(() => props.project, resetForm, { deep: true, immediate: true });
onMounted(loadModels);
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
    <a-space>
      <a-button type="primary" :loading="saving" @click="save">保存设置</a-button>
      <a-button @click="resetForm">撤销修改</a-button>
    </a-space>
  </a-form>
</template>
