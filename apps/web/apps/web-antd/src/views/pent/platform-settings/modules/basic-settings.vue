<script lang="ts" setup>
import type { AiModelModelApi } from '#/api/ai/model/model';
import type { PentModelBinding } from '#/api/pent';

import { onMounted, ref } from 'vue';
import { useRouter } from 'vue-router';
import { AiModelTypeEnum } from '@vben/constants';
import { message } from 'ant-design-vue';

import { getModelSimpleList, testModel } from '#/api/ai/model/model';
import {
  getPentAgentSettings,
  getPentModelBindings,
  getPentRuntimePolicy,
  updatePentAgentSettings,
  updatePentModelBindings,
  updatePentRuntimePolicy,
} from '#/api/pent';

const router = useRouter();
const loading = ref(false);
const saving = ref(false);
const testing = ref(false);
const models = ref<AiModelModelApi.Model[]>([]);
const defaultModelMissing = ref(false);
const runtimePolicy = ref<Record<string, any>>({});
const agentSettings = ref<Record<string, any>>({});
const binding = ref<PentModelBinding>({
  enabled: true,
  max_output_tokens: 4096,
  model_config_id: 0,
  model_role: 'conversation',
  project_id: '',
  temperature: 0.7,
  timeout_seconds: 120,
});
const visionBinding = ref<PentModelBinding>({
  enabled: false,
  max_output_tokens: 4096,
  model_config_id: 0,
  model_role: 'vision',
  project_id: '',
  temperature: 0.2,
  timeout_seconds: 120,
});

const agentDefaults: Record<string, any> = {
  external_mcp_circuit_cooldown_seconds: 60,
  external_mcp_circuit_failure_threshold: 3,
  external_mcp_max_concurrent_per_server: 2,
  external_mcp_max_concurrent_total: 16,
  latest_user_message_head_runes: 24000,
  latest_user_message_max_runes: 48000,
  latest_user_message_tail_runes: 24000,
  max_total_tokens: 120000,
  multi_agent_enabled: false,
  plan_execute_loop_max_iterations: 0,
  reasoning_allow_client_override: true,
  reasoning_effort: '',
  reasoning_mode: 'auto',
  reasoning_profile: 'auto',
  robot_default_agent_mode: 'eino_single',
  summarization_user_intent_ledger_entry_max_runes: 16000,
  summarization_user_intent_ledger_max_runes: 96000,
  tool_wait_timeout_seconds: 60,
  vision_detail: 'auto',
  vision_enabled: false,
  vision_jpeg_quality: 85,
  vision_max_dimension: 2048,
  vision_max_image_bytes: 20971520,
  vision_max_payload_bytes: 10485760,
  vision_skip_preprocess_bytes: 1048576,
  vision_timeout_seconds: 120,
};

async function load() {
  loading.value = true;
  try {
    const [modelList, bindings, policy, agent] = await Promise.all([
      getModelSimpleList(AiModelTypeEnum.CHAT),
      getPentModelBindings(),
      getPentRuntimePolicy(),
      getPentAgentSettings(),
    ]);
    models.value = modelList || [];
    const current = (bindings || []).find(
      (item) =>
        item.model_role === 'conversation' &&
        (!item.project_id || item.project_id === '0'),
    );
    defaultModelMissing.value = !current;
    if (current) binding.value = { ...current, project_id: '' };
    else binding.value.model_config_id = 0;
    const currentVision = (bindings || []).find(
      (item) =>
        item.model_role === 'vision' &&
        (!item.project_id || item.project_id === '0'),
    );
    if (currentVision) visionBinding.value = { ...currentVision, project_id: '' };
    runtimePolicy.value = policy || {};
    agentSettings.value = { ...agentDefaults, ...(agent || {}) };
  } catch (error: any) {
    message.error(error?.message || '加载基本设置失败');
  } finally {
    loading.value = false;
  }
}

async function testCurrentModel() {
  if (!binding.value.model_config_id) return;
  testing.value = true;
  try {
    const result = await testModel(binding.value.model_config_id, '回复 OK');
    message.success(`连接成功，耗时 ${result.latencyMs}ms`);
  } catch (error: any) {
    message.error(error?.message || '模型连接失败');
  } finally {
    testing.value = false;
  }
}

async function saveAll() {
  if (!binding.value.model_config_id) {
    message.warning('请选择默认 AI 模型');
    return;
  }
  saving.value = true;
  try {
    await Promise.all([
      updatePentModelBindings([
        { ...binding.value, project_id: '' },
        ...(agentSettings.value.vision_enabled && visionBinding.value.model_config_id
          ? [{ ...visionBinding.value, enabled: true, project_id: '' }]
          : []),
      ]),
      updatePentRuntimePolicy({
        lease_ttl_seconds: runtimePolicy.value.lease_ttl_seconds,
        max_concurrent_runs: runtimePolicy.value.max_concurrent_runs,
        max_retries_per_run: runtimePolicy.value.max_retries_per_run,
        max_tool_timeout_seconds: runtimePolicy.value.max_tool_timeout_seconds,
        retry_base_delay_seconds: runtimePolicy.value.retry_base_delay_seconds,
      }),
      updatePentAgentSettings(agentSettings.value),
    ]);
    message.success('基本设置已保存');
    await load();
  } catch (error: any) {
    message.error(error?.message || '保存失败');
  } finally {
    saving.value = false;
  }
}

onMounted(load);
</script>

<template>
  <a-spin :spinning="loading">
    <div class="section-heading">
      <div>
        <h2>基本设置</h2>
        <p>配置平台默认模型、Agent 执行参数与运行容错策略。</p>
      </div>
      <a-button type="primary" :loading="saving" @click="saveAll">保存更改</a-button>
    </div>

    <section class="settings-block">
      <div class="block-heading">
        <div>
          <h3>AI 模型</h3>
          <p>模型连接信息由“AI 大模型”统一维护，这里只选择 Pent 默认使用的模型。</p>
        </div>
        <a-space>
          <a-button @click="router.push('/ai/model')">管理 AI 大模型</a-button>
          <a-button :loading="testing" @click="testCurrentModel">测试连接</a-button>
        </a-space>
      </div>
      <a-form layout="vertical">
        <a-alert
          v-if="defaultModelMissing"
          class="mb-4"
          message="默认对话模型已删除或未配置，请重新选择并保存"
          show-icon
          type="warning"
        />
        <a-form-item label="默认对话模型" required>
          <a-select
            v-model:value="binding.model_config_id"
            show-search
            option-filter-prop="label"
            :options="models.map((model) => ({
              label: `${model.name} · ${model.platform} · ${model.model}`,
              value: model.id,
            }))"
            placeholder="从已配置的聊天模型中选择"
          />
        </a-form-item>
        <a-row :gutter="16">
          <a-col :xs="24" :md="8"><a-form-item label="最大上下文 Token 数"><a-input-number v-model:value="agentSettings.max_total_tokens" :min="1000" :max="1000000" :step="1000" /></a-form-item></a-col>
          <a-col :xs="24" :md="8">
            <a-form-item label="Temperature">
              <a-input-number v-model:value="binding.temperature" :min="0" :max="2" :step="0.1" />
            </a-form-item>
          </a-col>
          <a-col :xs="24" :md="8">
            <a-form-item label="最大输出 Token 数">
              <a-input-number v-model:value="binding.max_output_tokens" :min="256" :max="131072" />
            </a-form-item>
          </a-col>
          <a-col :xs="24" :md="8">
            <a-form-item label="请求超时（秒）">
              <a-input-number v-model:value="binding.timeout_seconds" :min="5" :max="3600" />
            </a-form-item>
          </a-col>
        </a-row>
      </a-form>
    </section>

    <section class="settings-block">
      <div class="block-heading"><div><h3>推理设置</h3><p>作为默认 AI 通道的推理参数；允许时可由对话页临时覆盖。</p></div></div>
      <a-form layout="vertical"><a-row :gutter="16">
        <a-col :xs="24" :md="8"><a-form-item label="推理模式"><a-select v-model:value="agentSettings.reasoning_mode" :options="[{label:'自动',value:'auto'},{label:'开启',value:'on'},{label:'关闭',value:'off'}]" /></a-form-item></a-col>
        <a-col :xs="24" :md="8"><a-form-item label="推理强度"><a-select v-model:value="agentSettings.reasoning_effort" :options="[{label:'不指定',value:''},{label:'low',value:'low'},{label:'medium',value:'medium'},{label:'high',value:'high'},{label:'xhigh',value:'xhigh'},{label:'max',value:'max'}]" /></a-form-item></a-col>
        <a-col :xs="24" :md="8"><a-form-item label="推理线路"><a-select v-model:value="agentSettings.reasoning_profile" :options="['auto','deepseek_compat','openai_compat','output_config_effort'].map(value=>({label:value,value}))" /></a-form-item></a-col>
        <a-col :span="24"><a-form-item label="允许对话页覆盖推理选项"><a-switch v-model:checked="agentSettings.reasoning_allow_client_override" /></a-form-item></a-col>
      </a-row></a-form>
    </section>

    <section class="settings-block">
      <div class="block-heading"><div><h3>视觉分析（analyze_image）</h3><p>视觉模型复用“AI 大模型”中已配置的聊天模型，不单独维护密钥。</p></div></div>
      <div class="switch-line"><div><b>启用视觉分析工具</b><p>图片只进入单次视觉调用，Agent 上下文保留文字摘要。</p></div><a-switch v-model:checked="agentSettings.vision_enabled" /></div>
      <a-form v-if="agentSettings.vision_enabled" layout="vertical" class="sub-form"><a-row :gutter="16">
        <a-col :span="24"><a-form-item label="视觉模型" required><a-select v-model:value="visionBinding.model_config_id" show-search option-filter-prop="label" :options="models.map(model=>({label:`${model.name} · ${model.platform} · ${model.model}`,value:model.id}))" placeholder="选择支持视觉输入的已配置模型" /></a-form-item></a-col>
        <a-col :xs="24" :md="8"><a-form-item label="原始文件上限（字节）"><a-input-number v-model:value="agentSettings.vision_max_image_bytes" :min="1" /></a-form-item></a-col>
        <a-col :xs="24" :md="8"><a-form-item label="长边缩放像素"><a-input-number v-model:value="agentSettings.vision_max_dimension" :min="256" :max="8192" /></a-form-item></a-col>
        <a-col :xs="24" :md="8"><a-form-item label="JPEG 质量"><a-input-number v-model:value="agentSettings.vision_jpeg_quality" :min="1" :max="100" /></a-form-item></a-col>
        <a-col :xs="24" :md="8"><a-form-item label="API 载荷上限（字节）"><a-input-number v-model:value="agentSettings.vision_max_payload_bytes" :min="1" /></a-form-item></a-col>
        <a-col :xs="24" :md="8"><a-form-item label="原图直传阈值（字节）"><a-input-number v-model:value="agentSettings.vision_skip_preprocess_bytes" :min="0" /></a-form-item></a-col>
        <a-col :xs="24" :md="8"><a-form-item label="视觉 Detail"><a-select v-model:value="agentSettings.vision_detail" :options="['auto','low','high'].map(value=>({label:value,value}))" /></a-form-item></a-col>
        <a-col :xs="24" :md="8"><a-form-item label="视觉请求超时（秒）"><a-input-number v-model:value="agentSettings.vision_timeout_seconds" :min="5" :max="3600" /></a-form-item></a-col>
      </a-row></a-form>
    </section>

    <section class="settings-block">
      <div class="block-heading"><div><h3>Agent 运行设置</h3><p>控制编排方式、迭代预算与动态工具检索。</p></div></div>
      <a-form layout="vertical">
        <a-row :gutter="16">
          <a-col :xs="24" :md="8"><a-form-item label="默认编排"><a-select v-model:value="agentSettings.orchestration" :options="[{label:'Deep',value:'deep'},{label:'Plan Execute',value:'plan_execute'},{label:'Supervisor',value:'supervisor'}]" /></a-form-item></a-col>
          <a-col :xs="24" :md="8"><a-form-item label="最大迭代"><a-input-number v-model:value="agentSettings.max_iterations" :min="1" :max="10000" /></a-form-item></a-col>
          <a-col :xs="24" :md="8"><a-form-item label="上下文预算 Tokens"><a-input-number v-model:value="agentSettings.context_budget_tokens" :min="2000" :max="256000" /></a-form-item></a-col>
          <a-col :xs="24" :md="8"><a-form-item label="动态工具检索"><a-switch v-model:checked="agentSettings.tool_search_enabled" /></a-form-item></a-col>
          <a-col :xs="24" :md="8"><a-form-item label="动态工具数量"><a-input-number v-model:value="agentSettings.tool_search_top_k" :min="6" :max="16" /></a-form-item></a-col>
          <a-col :xs="24" :md="8"><a-form-item label="模型重试次数"><a-input-number v-model:value="agentSettings.run_retry_max_attempts" :min="0" :max="20" /></a-form-item></a-col>
          <a-col :xs="24" :md="8"><a-form-item label="工具等待超时（秒）"><a-input-number v-model:value="agentSettings.tool_wait_timeout_seconds" :min="0" :max="7200" /></a-form-item></a-col>
          <a-col :xs="24" :md="8"><a-form-item label="单 MCP 最大并发"><a-input-number v-model:value="agentSettings.external_mcp_max_concurrent_per_server" :min="-1" :max="1000" /></a-form-item></a-col>
          <a-col :xs="24" :md="8"><a-form-item label="MCP 总最大并发"><a-input-number v-model:value="agentSettings.external_mcp_max_concurrent_total" :min="-1" :max="1000" /></a-form-item></a-col>
          <a-col :xs="24" :md="8"><a-form-item label="MCP 熔断失败阈值"><a-input-number v-model:value="agentSettings.external_mcp_circuit_failure_threshold" :min="-1" :max="1000" /></a-form-item></a-col>
          <a-col :xs="24" :md="8"><a-form-item label="MCP 熔断冷却（秒）"><a-input-number v-model:value="agentSettings.external_mcp_circuit_cooldown_seconds" :min="0" :max="86400" /></a-form-item></a-col>
        </a-row>
      </a-form>
    </section>

    <section class="settings-block">
      <div class="block-heading"><div><h3>Hermes 多代理</h3><p>控制多代理入口、Plan-Execute 外层循环及机器人默认执行模式。</p></div></div>
      <div class="switch-line"><div><b>启用 Hermes 多代理</b><p>开启后，对话页可以选择 Deep、Plan-Execute 或 Supervisor。</p></div><a-switch v-model:checked="agentSettings.multi_agent_enabled" /></div>
      <a-form layout="vertical" class="sub-form"><a-row :gutter="16">
        <a-col :xs="24" :md="12"><a-form-item label="Plan-Execute 外层循环上限"><a-input-number v-model:value="agentSettings.plan_execute_loop_max_iterations" :min="0" :max="10000" /></a-form-item></a-col>
        <a-col :xs="24" :md="12"><a-form-item label="机器人默认对话模式"><a-select v-model:value="agentSettings.robot_default_agent_mode" :options="[{label:'Hermes 单代理（ADK）',value:'eino_single'},{label:'Deep',value:'deep'},{label:'Plan-Execute',value:'plan_execute'},{label:'Supervisor',value:'supervisor'}]" /></a-form-item></a-col>
        <a-col :xs="24" :md="8"><a-form-item label="用户原文账本总字符上限"><a-input-number v-model:value="agentSettings.summarization_user_intent_ledger_max_runes" :min="0" /></a-form-item></a-col>
        <a-col :xs="24" :md="8"><a-form-item label="单条用户原文字符上限"><a-input-number v-model:value="agentSettings.summarization_user_intent_ledger_entry_max_runes" :min="0" /></a-form-item></a-col>
        <a-col :xs="24" :md="8"><a-form-item label="最新用户输入字符上限"><a-input-number v-model:value="agentSettings.latest_user_message_max_runes" :min="0" /></a-form-item></a-col>
        <a-col :xs="24" :md="8"><a-form-item label="最新输入头部预览"><a-input-number v-model:value="agentSettings.latest_user_message_head_runes" :min="0" /></a-form-item></a-col>
        <a-col :xs="24" :md="8"><a-form-item label="最新输入尾部预览"><a-input-number v-model:value="agentSettings.latest_user_message_tail_runes" :min="0" /></a-form-item></a-col>
      </a-row></a-form>
    </section>

    <section class="settings-block">
      <div class="block-heading"><div><h3>调度与容错</h3><p>只影响新启动的 Run，运行中任务继续使用启动时快照。</p></div></div>
      <a-form layout="vertical">
        <a-row :gutter="16">
          <a-col :xs="24" :md="8"><a-form-item label="最大并发 Run 数"><a-input-number v-model:value="runtimePolicy.max_concurrent_runs" :min="1" :max="100" /></a-form-item></a-col>
          <a-col :xs="24" :md="8"><a-form-item label="运行租约（秒）"><a-input-number v-model:value="runtimePolicy.lease_ttl_seconds" :min="30" :max="3600" /></a-form-item></a-col>
          <a-col :xs="24" :md="8"><a-form-item label="工具最大超时（秒）"><a-input-number v-model:value="runtimePolicy.max_tool_timeout_seconds" :min="10" :max="7200" /></a-form-item></a-col>
          <a-col :xs="24" :md="8"><a-form-item label="Run 最大重试"><a-input-number v-model:value="runtimePolicy.max_retries_per_run" :min="0" :max="20" /></a-form-item></a-col>
          <a-col :xs="24" :md="8"><a-form-item label="重试延迟（秒）"><a-input-number v-model:value="runtimePolicy.retry_base_delay_seconds" :min="1" :max="3600" /></a-form-item></a-col>
        </a-row>
      </a-form>
    </section>
  </a-spin>
</template>

<style scoped>
.section-heading,.block-heading,.switch-line{display:flex;align-items:flex-start;justify-content:space-between;gap:16px}.section-heading{margin-bottom:28px}.section-heading h2{margin:0;font-size:24px}.section-heading p,.block-heading p,.switch-line p{margin:6px 0 0;color:var(--ant-color-text-secondary)}.settings-block{padding:22px 0;border-top:1px solid var(--ant-color-border-secondary)}.settings-block:first-of-type{border-top:0}.block-heading{margin-bottom:18px}.block-heading h3{margin:0;font-size:16px}.switch-line{margin-bottom:18px}.sub-form{margin-top:18px}.settings-block :deep(.ant-input-number){width:100%}
</style>
