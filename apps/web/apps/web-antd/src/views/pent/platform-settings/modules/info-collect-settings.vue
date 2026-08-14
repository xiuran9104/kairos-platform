<script lang="ts" setup>
import type { InfoCollectProviderConfig } from '#/api/pent';

import { onMounted, ref } from 'vue';
import { message } from 'ant-design-vue';

import { getInfoCollectConfig, updateInfoCollectConfig } from '#/api/pent';

const loading = ref(false);
const saving = ref(false);
const providers = ref<InfoCollectProviderConfig[]>([]);
const names: Record<string, string> = { fofa: 'FOFA', quake: '360 Quake', shodan: 'Shodan', zoomeye: 'ZoomEye' };

async function load() {
  loading.value = true;
  try { providers.value = (await getInfoCollectConfig()).map((item) => ({ ...item, api_key: '' })); }
  catch (error: any) { message.error(error?.message || '加载数据源配置失败'); }
  finally { loading.value = false; }
}
async function save() {
  saving.value = true;
  try { providers.value = (await updateInfoCollectConfig(providers.value)).map((item) => ({ ...item, api_key: '' })); message.success('信息收集配置已保存'); }
  catch (error: any) { message.error(error?.message || '保存失败'); }
  finally { saving.value = false; }
}
onMounted(load);
</script>

<template>
  <a-spin :spinning="loading">
    <div class="section-heading"><div><h2>资产管理</h2><p>配置互联网资产测绘数据源；密钥加密保存在服务器端，页面不会回显明文。</p></div><a-button type="primary" :loading="saving" @click="save">保存更改</a-button></div>
    <a-alert class="security-note" type="info" show-icon message="留空 API Key 会保留服务器中已有密钥；环境变量仍可作为未配置时的后备凭据。" />
    <section v-for="provider in providers" :key="provider.id" class="provider-block">
      <div class="provider-heading"><div><h3>{{ names[provider.id] }}</h3><a-tag :color="provider.configured ? 'green' : 'default'">{{ provider.configured ? '已配置' : '未配置' }}</a-tag></div><a-switch v-model:checked="provider.enabled" /></div>
      <a-form layout="vertical"><a-row :gutter="20">
        <a-col :xs="24" :md="12"><a-form-item label="Base URL"><a-input v-model:value="provider.base_url" /></a-form-item></a-col>
        <a-col :xs="24" :md="12"><a-form-item :label="provider.id === 'quake' ? 'API Token' : 'API Key'"><a-input-password v-model:value="provider.api_key" autocomplete="new-password" :placeholder="provider.configured ? '已保存；留空则不修改' : '输入密钥'" /></a-form-item></a-col>
      </a-row></a-form>
    </section>
  </a-spin>
</template>

<style scoped>
.section-heading,.provider-heading{display:flex;align-items:flex-start;justify-content:space-between;gap:20px}.section-heading{margin-bottom:20px}.section-heading h2{margin:0;font-size:24px}.section-heading p{margin:6px 0 0;color:var(--ant-color-text-secondary)}.security-note{margin-bottom:8px}.provider-block{padding:22px 0 8px;border-bottom:1px solid var(--ant-color-border-secondary)}.provider-block:last-child{border-bottom:0}.provider-heading{margin-bottom:16px}.provider-heading>div{display:flex;align-items:center;gap:10px}.provider-heading h3{margin:0;font-size:16px}
</style>
