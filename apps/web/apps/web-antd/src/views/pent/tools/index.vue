<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue';
import { getPentTools } from '#/api/pent';

const tools = ref<any[]>([]);
const loading = ref(false);
const searchText = ref('');
const nsFilter = ref('all');

async function fetch() { loading.value = true; try { tools.value = (await getPentTools()) as any[]; } catch { /* */ } finally { loading.value = false; } }

const nsList = computed(() => {
  const ns = [...new Set(tools.value.map((t:any) => t.namespace).filter(Boolean))];
  return [{label:'全部',value:'all'}, ...ns.map((n:any)=>({label:n,value:n}))];
});

const filtered = computed(() => {
  let list = tools.value;
  if (searchText.value) {
    const q = searchText.value.toLowerCase();
    list = list.filter((t:any) => (t.name||'').toLowerCase().includes(q) || (t.description||'').toLowerCase().includes(q));
  }
  if (nsFilter.value !== 'all') list = list.filter((t:any) => t.namespace === nsFilter.value);
  return list;
});

onMounted(fetch);
</script>

<template>
  <div style="padding:24px;min-height:100vh">
    <a-page-header title="工具生态" sub-title="安全工具目录 · 63+ 可调用工具">
      <template #extra>
        <a-input-search v-model:value="searchText" placeholder="搜索..." style="width:200px" allow-clear />
        <a-select v-model:value="nsFilter" style="width:120px;margin-left:8px" :options="nsList" />
        <a-button @click="fetch" :loading="loading" style="margin-left:8px">刷新</a-button>
      </template>
    </a-page-header>

    <a-spin :spinning="loading">
      <a-row :gutter="[16,16]">
        <a-col :xs="24" :sm="12" :md="8" :lg="6" v-for="t in filtered" :key="t.tool_id+'-'+t.version">
          <a-card hoverable size="small">
            <template #title><span style="font-size:13px">{{ t.name }}</span></template>
            <template #extra>
              <a-tag :color="t.risk_level==='high'?'red':t.risk_level==='medium'?'orange':'green'" size="small">{{ t.risk_level || 'low' }}</a-tag>
            </template>
            <div style="font-size:12px;color:#8c8c8c;margin-bottom:8px;min-height:32px">{{ t.description }}</div>
            <div style="display:flex;flex-wrap:wrap;gap:4px">
              <a-tag size="small" color="blue">{{ t.namespace }}</a-tag>
              <a-tag v-for="cap in (t.capabilities||[]).slice(0,3)" :key="cap" size="small">{{ cap }}</a-tag>
            </div>
          </a-card>
        </a-col>
      </a-row>
      <a-empty v-if="!filtered.length && !loading" description="暂无工具" />
    </a-spin>
  </div>
</template>
