<script lang="ts" setup>
import type { AiModelModelApi } from '#/api/ai/model/model';
import type { HitlSettings } from '#/api/pent';

import { onMounted, ref } from 'vue';
import { AiModelTypeEnum } from '@vben/constants';
import { message } from 'ant-design-vue';

import { getModelSimpleList, testModel } from '#/api/ai/model/model';
import { getHitlSettings, updateHitlSettings } from '#/api/pent';

const loading = ref(false);
const saving = ref(false);
const testing = ref(false);
const models = ref<AiModelModelApi.Model[]>([]);
const whitelistText = ref('');
const settings = ref<HitlSettings>({
  approval_ttl_seconds: 900,
  hitl_audit_system_prompt: '',
  hitl_audit_user_prompt: '',
  hitl_enabled: false,
  hitl_mode: 'approval',
  hitl_reviewer: 'human',
  hitl_tool_whitelist: [],
  retention_days: 90,
});

function normalizeSettings(data?: Partial<HitlSettings>): HitlSettings {
  return {
    approval_ttl_seconds: data?.approval_ttl_seconds ?? 1800,
    hitl_audit_model_id: data?.hitl_audit_model_id,
    hitl_audit_system_prompt: data?.hitl_audit_system_prompt ?? '',
    hitl_audit_user_prompt: data?.hitl_audit_user_prompt ?? '',
    hitl_enabled: data?.hitl_enabled ?? false,
    hitl_mode: data?.hitl_mode ?? 'approval',
    hitl_reviewer: data?.hitl_reviewer ?? 'human',
    hitl_tool_whitelist: Array.isArray(data?.hitl_tool_whitelist)
      ? data.hitl_tool_whitelist
      : [],
    inherited: data?.inherited,
    retention_days: data?.retention_days ?? 90,
  };
}

async function load() {
  loading.value = true;
  try {
    const [data, modelList] = await Promise.all([
      getHitlSettings(),
      getModelSimpleList(AiModelTypeEnum.CHAT),
    ]);
    settings.value = normalizeSettings(data);
    models.value = modelList || [];
    whitelistText.value = settings.value.hitl_tool_whitelist.join('\n');
  } catch (error: any) {
    message.error(error?.message || '加载 AI 围栏设置失败');
  } finally {
    loading.value = false;
  }
}

async function save() {
  saving.value = true;
  try {
    await updateHitlSettings({
      ...settings.value,
      hitl_tool_whitelist: whitelistText.value
        .split(/[\n,]/)
        .map((item) => item.trim())
        .filter(Boolean),
    });
    const refreshed = await getHitlSettings();
    settings.value = normalizeSettings(refreshed);
    whitelistText.value = settings.value.hitl_tool_whitelist.join('\n');
    message.success('AI 围栏设置已保存');
  } catch (error: any) {
    message.error(error?.message || '保存失败');
  } finally {
    saving.value = false;
  }
}

async function testAuditModel() {
  if (!settings.value.hitl_audit_model_id) return;
  testing.value = true;
  try {
    const result = await testModel(settings.value.hitl_audit_model_id, 'AI 围栏审计模型连接测试');
    message.success(`连接成功，耗时 ${result.latencyMs}ms`);
  } catch (error: any) {
    message.error(error?.message || '审计模型连接失败');
  } finally {
    testing.value = false;
  }
}

onMounted(load);
</script>

<template>
  <a-spin :spinning="loading">
    <div class="section-heading">
      <div><h2>AI 围栏</h2><p>高风险工具在执行前进入围栏，由人工或审计 Agent 审批。</p></div>
      <a-button type="primary" :loading="saving" @click="save">保存更改</a-button>
    </div>
    <section class="settings-block">
      <div class="switch-row">
        <div><h3>启用 AI 围栏</h3><p>开启后，命中围栏规则的工具调用会暂停，审批通过后才能继续执行。</p></div>
        <a-switch v-model:checked="settings.hitl_enabled" />
      </div>
    </section>
    <section class="settings-block">
      <a-form layout="vertical">
        <a-row :gutter="20">
          <a-col :xs="24" :md="12"><a-form-item label="围栏模式"><a-select v-model:value="settings.hitl_mode" :options="[{label:'审批模式（通过 / 拒绝）',value:'approval'},{label:'审查编辑（可修改参数）',value:'review_edit'}]" /></a-form-item></a-col>
          <a-col :xs="24" :md="12"><a-form-item label="审批方"><a-select v-model:value="settings.hitl_reviewer" :options="[{label:'人工审批',value:'human'},{label:'审计 Agent',value:'audit_agent'}]" /></a-form-item></a-col>
          <a-col :xs="24" :md="12"><a-form-item label="审批有效期（秒）"><a-input-number v-model:value="settings.approval_ttl_seconds" :min="60" :max="86400" /></a-form-item></a-col>
          <a-col :xs="24" :md="12"><a-form-item label="已决策日志保留天数" extra="默认 90 天；0 表示永久保留。"><a-input-number v-model:value="settings.retention_days" :min="0" :max="3650" /></a-form-item></a-col>
          <a-col v-if="settings.hitl_reviewer === 'audit_agent'" :xs="24" :md="12"><a-form-item label="审计模型" extra="留空时继承当前会话使用的 AI 大模型。"><a-input-group compact><a-select v-model:value="settings.hitl_audit_model_id" allow-clear :options="models.map((model) => ({label: `${model.name} · ${model.model}`, value: model.id}))" placeholder="从 AI 大模型中选择" style="width:calc(100% - 96px)" /><a-button :disabled="!settings.hitl_audit_model_id" :loading="testing" style="width:96px" @click="testAuditModel">测试连接</a-button></a-input-group></a-form-item></a-col>
          <a-col :span="24"><a-form-item label="白名单工具（免审批）" extra="每行一个或逗号分隔；白名单内的工具不会触发 AI 围栏审批。"><a-textarea v-model:value="whitelistText" :rows="5" placeholder="例如：read_file, grep" /></a-form-item></a-col>
          <a-col v-if="settings.hitl_reviewer === 'audit_agent'" :span="24"><a-form-item label="审批模式审计提示词" extra="留空时直接复用 desredteam-v2 的内置默认提示词。"><a-textarea v-model:value="settings.hitl_audit_system_prompt" :rows="7" placeholder="留空使用内置默认提示词" /></a-form-item></a-col>
          <a-col v-if="settings.hitl_reviewer === 'audit_agent'" :span="24"><a-form-item label="审查编辑模式审计提示词" extra="留空时直接复用 desredteam-v2 的内置审查编辑提示词。"><a-textarea v-model:value="settings.hitl_audit_user_prompt" :rows="7" placeholder="留空使用内置默认提示词" /></a-form-item></a-col>
        </a-row>
      </a-form>
    </section>
  </a-spin>
</template>

<style scoped>
.section-heading,.switch-row{display:flex;align-items:flex-start;justify-content:space-between;gap:20px}.section-heading{margin-bottom:28px}.section-heading h2{margin:0;font-size:24px}.section-heading p,.switch-row p{margin:6px 0 0;color:var(--ant-color-text-secondary)}.settings-block{padding:22px 0;border-top:1px solid var(--ant-color-border-secondary)}.settings-block:first-of-type{border-top:0}.switch-row h3{margin:0;font-size:16px}.settings-block :deep(.ant-input-number){width:100%}
</style>
