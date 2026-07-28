<script lang="ts" setup>
import { ref, onMounted } from 'vue';

import { message } from 'ant-design-vue';
import { getPentApprovals, decidePentApproval } from '#/api/pent';
import PageHeader from '../components/PageHeader.vue';
const data = ref<any[]>([]); const loading = ref(false);
const columns = [{ title:'工具',dataIndex:'tool_name',width:100},{ title:'风险',dataIndex:'risk_level',width:80},{ title:'描述',dataIndex:'description',ellipsis:true},{ title:'状态',dataIndex:'status',width:80},{ title:'操作',key:'actions',width:160 }];
async function fetch() { loading.value=true; try { data.value=(await getPentApprovals()) as any[]; } catch { message.error('加载失败'); } finally { loading.value=false; } }
onMounted(fetch);
async function handleDecide(id:string,decision:string) { await decidePentApproval(id,{decision}); message.success(decision==='approved'?'已批准':'已拒绝'); fetch(); }
const riskColor:Record<string,string>={low:'green',medium:'orange',high:'red',critical:'var(--ant-color-error)'};
</script>
<template>
  <div style="padding:24px;background:var(--ant-color-bg-layout);min-height:100%">
    <PageHeader title="审批管理" subtitle="审核高风险工具调用请求">
      <template #extra>
        <a-button @click="fetch">刷新</a-button>
      </template>
    </PageHeader>

    <a-table :columns="columns" :data-source="data" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:15}">
      <template #bodyCell="{ column, record }">
        <template v-if="column.key==='risk_level'"><a-tag :color="riskColor[record.risk_level]||'default'">{{ record.risk_level }}</a-tag></template>
        <template v-if="column.key==='actions'"><a-space><a-button v-if="record.status==='pending'" type="primary" size="small" @click="handleDecide(record.id,'approved')">批准</a-button><a-button v-if="record.status==='pending'" size="small" danger @click="handleDecide(record.id,'denied')">拒绝</a-button></a-space></template>
      </template>
    </a-table>
  </div>
</template>
