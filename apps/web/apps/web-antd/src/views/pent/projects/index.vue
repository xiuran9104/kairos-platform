<script lang="ts" setup>
import { ref, onMounted } from 'vue';

import { message } from 'ant-design-vue';
import { getPentProjects, createPentProject } from '#/api/pent';
const data = ref<any[]>([]); const loading = ref(false); const modalVisible = ref(false);
const form = ref({ name: '', description: '', asset_scope: '[]' });
const columns = [{ title:'名称',dataIndex:'name'},{ title:'描述',dataIndex:'description',ellipsis:true},{ title:'创建时间',dataIndex:'created_at',width:180},{ title:'操作',key:'actions',width:100 }];
async function fetch() { loading.value=true; try { data.value=(await getPentProjects()) as any[]; } catch { message.error('加载失败'); } finally { loading.value=false; } }
onMounted(fetch);
async function handleCreate() { await createPentProject({...form.value,asset_scope:JSON.parse(form.value.asset_scope||'[]')}); message.success('创建成功'); modalVisible.value=false; fetch(); }
</script>
<template>
  <div style="padding:24px;background:#f5f5f5;min-height:100%">
    <div style="background:#fff;border-radius:8px;margin-bottom:16px;padding:16px 24px;display:flex;justify-content:space-between;align-items:center">
      <div><h2 style="margin:0">项目管理</h2><span style="color:#8c8c8c;font-size:13px">管理安全测试项目与资产范围</span></div>
      <a-space><a-button @click="fetch">刷新</a-button><a-button type="primary" @click="modalVisible=true">新建项目</a-button></a-space>
    </div>

    <a-table :columns="columns" :data-source="data" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:15}"><template #bodyCell="{ column }"><template v-if="column.key==='actions'"><a-button type="link" size="small">详情</a-button></template></template></a-table>
    <a-modal v-model:open="modalVisible" title="新建项目" @ok="handleCreate" width="500px">
      <a-form layout="vertical">
        <a-form-item label="名称" required><a-input v-model:value="form.name" /></a-form-item>
        <a-form-item label="描述"><a-textarea v-model:value="form.description" /></a-form-item>
        <a-form-item label="资产范围(JSON)"><a-textarea v-model:value="form.asset_scope" placeholder='["10.0.0.0/24"]' /></a-form-item>
      </a-form>
    </a-modal>
  </div>
</template>
