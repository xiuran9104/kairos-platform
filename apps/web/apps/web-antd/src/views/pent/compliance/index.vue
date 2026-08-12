<script lang="ts" setup>
import { onMounted, ref } from 'vue';
import { message } from 'ant-design-vue';
import { getPentRuntimePolicy, updatePentRuntimePolicy } from '#/api/pent';
defineOptions({ name: 'PentCompliance' });
const policy = ref<any>({ max_concurrent_runs: 5, lease_ttl_seconds: 120, max_tool_timeout_seconds: 900, max_retries_per_run: 3, retry_base_delay_seconds: 15 });
const loading = ref(false);
const saving = ref(false);
async function load() {
  loading.value = true;
  try { const d: any = await getPentRuntimePolicy(); policy.value = d?.data ?? d ?? policy.value; }
  finally { loading.value = false; }
}
async function save() {
  saving.value = true;
  try { await updatePentRuntimePolicy(policy.value); message.success('已保存'); }
  catch (e: any) { message.error(e?.message || '保存失败'); }
  finally { saving.value = false; }
}
onMounted(load);
</script>
<template>
  <div class="p-4 max-w-3xl mx-auto">
    <a-page-header title="合规与栅栏配置" sub-title="运行策略、AUP、高风险动作管控" />
    <a-card :bordered="false">
      <a-alert message="以下配置全局生效，修改后立即应用于新任务" type="info" show-icon class="mb-4" />
      <a-form layout="vertical">
        <a-row :gutter="16">
          <a-col :span="12"><a-form-item label="最大并发运行数"><a-input-number v-model:value="policy.max_concurrent_runs" :min="1" :max="100" style="width:100%" /></a-form-item></a-col>
          <a-col :span="12"><a-form-item label="Worker 租约 TTL (秒)"><a-input-number v-model:value="policy.lease_ttl_seconds" :min="30" :max="3600" style="width:100%" /></a-form-item></a-col>
          <a-col :span="12"><a-form-item label="工具超时 (秒)"><a-input-number v-model:value="policy.max_tool_timeout_seconds" :min="10" :max="7200" style="width:100%" /></a-form-item></a-col>
          <a-col :span="12"><a-form-item label="最大重试次数"><a-input-number v-model:value="policy.max_retries_per_run" :min="0" :max="20" style="width:100%" /></a-form-item></a-col>
        </a-row>
        <a-button type="primary" :loading="saving" @click="save">保存配置</a-button>
      </a-form>
    </a-card>
  </div>
</template>
