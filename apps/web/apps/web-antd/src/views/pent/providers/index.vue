<script lang="ts" setup>
import { onMounted, ref } from 'vue';
import { requestClient } from '#/api/request';
defineOptions({ name: 'PentProviders' });
const bindings = ref<any[]>([]);
const loading = ref(false);
async function load() {
  loading.value = true;
  try { bindings.value = (await requestClient.get('/pent/settings/runtime-policy')).data ?? []; }
  catch { bindings.value = []; }
  finally { loading.value = false; }
}
onMounted(load);
</script>
<template>
  <div class="p-4">
    <a-page-header title="LLM 供应商管理" sub-title="按任务/角色配置 AI 模型，记录 Token 消耗与成本" />
    <a-card :bordered="false">
      <a-alert message="模型绑定在 AI 模型管理中配置，此处展示已绑定的会话模型" type="info" show-icon class="mb-4" />
      <a-empty description="前往「AI 大模型 → 模型管理」配置可用模型" />
    </a-card>
  </div>
</template>
