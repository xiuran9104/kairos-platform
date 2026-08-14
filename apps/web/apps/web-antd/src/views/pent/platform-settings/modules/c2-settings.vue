<script lang="ts" setup>
import { onMounted, ref } from 'vue';
import { message } from 'ant-design-vue';
import { getPentPlatformSection, updatePentPlatformSection } from '#/api/pent';

const loading = ref(false);
const saving = ref(false);
const enabled = ref(true);

async function load() {
  loading.value = true;
  try { enabled.value = (await getPentPlatformSection('c2')).enabled !== false; }
  catch (error: any) { message.error(error?.message || '加载 C2 设置失败'); }
  finally { loading.value = false; }
}

async function save() {
  saving.value = true;
  try { await updatePentPlatformSection('c2', { enabled: enabled.value }); message.success('C2 设置已保存'); }
  catch (error: any) { message.error(error?.message || '保存失败'); }
  finally { saving.value = false; }
}

onMounted(load);
</script>

<template>
  <a-spin :spinning="loading">
    <div class="heading"><div><h2>C2 设置</h2><p>控制 C2 模块是否在平台中启用。</p></div><a-button type="primary" :loading="saving" @click="save">保存更改</a-button></div>
    <section class="block"><div><h3>启用 C2 模块</h3><p>关闭后隐藏并停止使用 C2 监听器、会话、任务和载荷功能。</p></div><a-switch v-model:checked="enabled" /></section>
  </a-spin>
</template>

<style scoped>.heading,.block{display:flex;justify-content:space-between;align-items:flex-start;gap:20px}.heading{margin-bottom:28px}.heading h2{margin:0;font-size:24px}.heading p,.block p{margin:6px 0 0;color:var(--ant-color-text-secondary)}.block{padding:22px 0;border-top:1px solid var(--ant-color-border-secondary)}.block h3{margin:0;font-size:16px}</style>
