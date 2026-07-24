<script lang="ts" setup>
import { onMounted, ref } from 'vue';
import { requestClient } from '#/api/request';
const rows = ref<any[]>([]); const loading = ref(false);
async function fetch() { loading.value=true; try{const r:any=await requestClient.get('/pent/traffic');rows.value=r||[]}catch{/* */} finally{loading.value=false} }
onMounted(fetch);
</script>
<template>
  <div style="padding:24px;min-height:100vh">
    <a-page-header title="LLM 流量" sub-title="模型调用记录 · 令牌用量 · 延迟分析"><template #extra><a-button @click="fetch" :loading="loading">刷新</a-button></template></a-page-header>
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
    <a-empty v-if="!rows.length&&!loading" description="暂无 LLM 调用记录"/>
  </div>
</template>
