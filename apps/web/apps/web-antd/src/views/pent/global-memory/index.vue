<script lang="ts" setup>
import { ref } from 'vue';
import { requestClient } from '#/api/request';
defineOptions({ name: 'PentGlobalMemory' });
const query = ref('');
const results = ref<any[]>([]);
const loading = ref(false);
async function search() {
  if (!query.value.trim()) return;
  loading.value = true;
  try { results.value = (await requestClient.get('/pent/rmos/global-search', { params: { q: query.value, limit: 30 } }))?.data ?? []; }
  finally { loading.value = false; }
}
</script>
<template>
  <div class="p-4">
    <a-page-header title="全局记忆检索" sub-title="跨项目搜索 TTP、指纹、攻击模式" />
    <a-card :bordered="false">
      <a-input-search v-model:value="query" placeholder="搜索历史渗透记忆，如：SQL Injection、nmap、CSP bypass" enter-button="搜索" @search="search" size="large" :loading="loading" />
      <a-divider />
      <a-list :data-source="results" :loading="loading" size="small">
        <template #renderItem="{ item }">
          <a-list-item>
            <a-list-item-meta>
              <template #title>{{ item.title || item.source || '记忆条目' }}</template>
              <template #description>{{ (item.content || '').slice(0, 300) }}</template>
            </a-list-item-meta>
          </a-list-item>
        </template>
        <template #empty>
          <a-empty v-if="!loading" description="输入关键词搜索跨项目记忆" />
        </template>
      </a-list>
    </a-card>
  </div>
</template>
