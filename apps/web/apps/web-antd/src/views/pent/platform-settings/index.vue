<script lang="ts" setup>
import { onMounted, ref } from 'vue';

import { message } from 'ant-design-vue';

import {
  getPentRuntimePolicy,
  updatePentRuntimePolicy,
  getPentAgentSettings,
  updatePentAgentSettings,
} from '#/api/pent';

const loading = ref(false);
const saving = ref(false);
const runtimePolicy = ref<Record<string, any>>({});
const agentSettings = ref<Record<string, any>>({});

async function fetchPolicy() {
  loading.value = true;
  try {
    [runtimePolicy.value, agentSettings.value] = await Promise.all([getPentRuntimePolicy(), getPentAgentSettings()]);
  } catch (error: any) {
    message.error(error?.message || '加载运行策略失败');
  } finally {
    loading.value = false;
  }
}

async function saveAgentSettings() {
  saving.value = true;
  try { await updatePentAgentSettings(agentSettings.value); message.success('Agent 设置已保存'); await fetchPolicy(); }
  catch (error: any) { message.error(error?.message || '保存 Agent 设置失败'); }
  finally { saving.value = false; }
}

async function savePolicy() {
  saving.value = true;
  try {
    runtimePolicy.value = await updatePentRuntimePolicy({
      lease_ttl_seconds: runtimePolicy.value.lease_ttl_seconds,
      max_concurrent_runs: runtimePolicy.value.max_concurrent_runs,
      max_retries_per_run: runtimePolicy.value.max_retries_per_run,
      max_tool_timeout_seconds: runtimePolicy.value.max_tool_timeout_seconds,
      retry_base_delay_seconds: runtimePolicy.value.retry_base_delay_seconds,
    });
    message.success('运行策略已保存');
  } catch (error: any) {
    message.error(error?.message || '保存运行策略失败');
  } finally {
    saving.value = false;
  }
}

onMounted(fetchPolicy);
</script>

<template>
  <div class="platform-settings">
    <div class="page-header">
      <div>
        <h1>运行策略</h1>
        <p>配置 Pent Agent 的并发、超时和失败重试行为</p>
      </div>
      <a-button :loading="loading" @click="fetchPolicy">刷新</a-button>
    </div>

    <a-spin :spinning="loading">
      <a-card title="Agent 调度与容错">
        <template #extra>
          <a-space>
            <a-tag v-if="runtimePolicy.inherited" color="gold">继承默认策略</a-tag>
            <a-button type="primary" :loading="saving" @click="savePolicy">
              保存策略
            </a-button>
          </a-space>
        </template>

        <a-alert
          class="policy-alert"
          type="info"
          show-icon
          message="模型统一在 AI 模型配置中管理。这里的策略仅影响新启动的 Run，运行中的任务继续使用启动时快照。"
        />

        <a-form layout="vertical" class="policy-form">
          <a-row :gutter="24">
            <a-col :xs="24" :md="12">
              <a-form-item label="最大并发 Run 数">
                <a-input-number
                  v-model:value="runtimePolicy.max_concurrent_runs"
                  :min="1"
                  :max="100"
                  class="number-input"
                />
              </a-form-item>
            </a-col>
            <a-col :xs="24" :md="12">
              <a-form-item label="运行租约有效期（秒）">
                <a-input-number
                  v-model:value="runtimePolicy.lease_ttl_seconds"
                  :min="30"
                  :max="3600"
                  class="number-input"
                />
              </a-form-item>
            </a-col>
            <a-col :xs="24" :md="12">
              <a-form-item label="单个工具最大超时（秒）">
                <a-input-number
                  v-model:value="runtimePolicy.max_tool_timeout_seconds"
                  :min="10"
                  :max="7200"
                  class="number-input"
                />
              </a-form-item>
            </a-col>
            <a-col :xs="24" :md="12">
              <a-form-item label="单次 Run 最大重试次数">
                <a-input-number
                  v-model:value="runtimePolicy.max_retries_per_run"
                  :min="0"
                  :max="20"
                  class="number-input"
                />
              </a-form-item>
            </a-col>
            <a-col :xs="24" :md="12">
              <a-form-item label="重试基础延迟（秒）">
                <a-input-number
                  v-model:value="runtimePolicy.retry_base_delay_seconds"
                  :min="1"
                  :max="3600"
                  class="number-input"
                />
              </a-form-item>
            </a-col>
          </a-row>
        </a-form>
      </a-card>
    </a-spin>
    <a-card title="DesRed Agent 运行设置" style="margin-top:16px">
      <template #extra><a-button type="primary" :loading="saving" @click="saveAgentSettings">保存 Agent 设置</a-button></template>
      <a-form layout="vertical">
        <a-row :gutter="24">
          <a-col :span="8"><a-form-item label="默认编排"><a-select v-model:value="agentSettings.orchestration" :options="[{label:'Deep',value:'deep'},{label:'Plan Execute',value:'plan_execute'},{label:'Supervisor',value:'supervisor'}]" /></a-form-item></a-col>
          <a-col :span="8"><a-form-item label="最大迭代"><a-input-number v-model:value="agentSettings.max_iterations" :min="1" :max="10000" class="number-input" /></a-form-item></a-col>
          <a-col :span="8"><a-form-item label="上下文预算 Tokens"><a-input-number v-model:value="agentSettings.context_budget_tokens" :min="2000" :max="256000" class="number-input" /></a-form-item></a-col>
          <a-col :span="8"><a-form-item label="动态工具检索"><a-switch v-model:checked="agentSettings.tool_search_enabled" /></a-form-item></a-col>
          <a-col :span="8"><a-form-item label="工具检索阈值"><a-input-number v-model:value="agentSettings.tool_search_min_tools" :min="1" :max="1000" class="number-input" /></a-form-item></a-col>
          <a-col :span="8"><a-form-item label="动态工具数量"><a-input-number v-model:value="agentSettings.tool_search_top_k" :min="1" :max="200" class="number-input" /></a-form-item></a-col>
          <a-col :span="8"><a-form-item label="模型重试次数"><a-input-number v-model:value="agentSettings.run_retry_max_attempts" :min="0" :max="20" class="number-input" /></a-form-item></a-col>
          <a-col :span="8"><a-form-item label="最大退避秒数"><a-input-number v-model:value="agentSettings.run_retry_max_backoff_seconds" :min="1" :max="300" class="number-input" /></a-form-item></a-col>
          <a-col :span="8"><a-form-item label="子代理用户上下文字符"><a-input-number v-model:value="agentSettings.sub_agent_user_context_max_chars" :min="0" class="number-input" /></a-form-item></a-col>
        </a-row>
      </a-form>
    </a-card>
  </div>
</template>

<style scoped>
.platform-settings {
  min-height: 100%;
  padding: 16px;
  background: var(--ant-color-bg-layout);
}

.page-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  margin-bottom: 16px;
}

.page-header h1 {
  margin: 0;
  font-size: 24px;
  font-weight: 650;
}

.page-header p {
  margin: 6px 0 0;
  color: var(--ant-color-text-secondary);
}

.policy-alert {
  margin-bottom: 24px;
}

.policy-form {
  max-width: 920px;
}

.number-input {
  width: 100%;
}
</style>
