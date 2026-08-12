<script lang="ts" setup>
import { onMounted, ref, watch } from 'vue';
import { getPentProjects } from '#/api/pent';
import { requestClient } from '#/api/request';
const projects = ref<any[]>([]); const selectedProjectId = ref<string>();
const rows = ref<any[]>([]); const loading = ref(false);

async function loadProjects() {
  try { projects.value = (await getPentProjects()) as any[]; }
  catch { projects.value = []; }
}
async function fetch() {
  if (!selectedProjectId.value) { rows.value = []; return; }
  loading.value = true;
  try {
    const r: any = await requestClient.get('/pent/traffic', { params: { project_id: selectedProjectId.value } });
    rows.value = r?.data ?? r ?? [];
  } catch { /* */ }
  finally { loading.value = false; }
}
watch(selectedProjectId, () => { fetch(); });
onMounted(async () => { await loadProjects(); if (projects.value.length > 0) selectedProjectId.value = projects.value[0].id; });
</script>
<template>
  <div style="padding:24px;min-height:100%;background-color: var(--ant-color-bg-layout)">
    <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:16px">
      <div style="display:flex;align-items:center;gap:12px">
        <span style="font-weight:600;font-size:16px">LLM 流量</span>
        <a-select v-model:value="selectedProjectId" :options="projects.map(p => ({ label: p.name, value: p.id }))" placeholder="选择项目" style="width:220px" allow-clear />
      </div>
      <a-button size="small" @click="fetch" :loading="loading">刷新</a-button>
    </div>
    <a-table :columns="[
      {title:'模型',dataIndex:'model',key:'model',width:160},
      {title:'角色',dataIndex:'model_role',key:'role',width:100},
      {title:'状态',key:'status',width:80},
      {title:'输入Token',dataIndex:'input_tokens',key:'in',width:100,align:'right'},
      {title:'输出Token',dataIndex:'output_tokens',key:'out',width:100,align:'right'},
      {title:'延迟(ms)',dataIndex:'latency_ms',key:'lat',width:100,align:'right'},
      {title:'时间',key:'time',width:180},
    ]" :data-source="rows" row-key="id" size="middle" :loading="loading" :pagination="{pageSize:20}">
      <template #bodyCell="{column,record}">
        <template v-if="column.key==='status'"><a-tag :color="record.status==='completed'?'green':'red'">{{record.status}}</a-tag></template>
        <template v-if="column.key==='time'">{{record.started_at?new Date(record.started_at).toLocaleString():'-'}}</template>
        <template v-if="column.key==='in'||column.key==='out'"><span style="font-family:monospace">{{(record[column.dataIndex]||0).toLocaleString()}}</span></template>
        <template v-if="column.key==='lat'"><span style="font-family:monospace">{{(record.latency_ms||0).toLocaleString()}}</span></template>
      </template>
    </a-table>
    <a-empty v-if="!rows.length&&!loading" description="暂无 LLM 调用记录" />
  </div>
</template>
