<script lang="ts" setup>
import { reactive, ref, onMounted } from 'vue';

import { Form, message } from 'ant-design-vue';
import type { Rule } from 'ant-design-vue/es/form';
import { getPentProjects, createPentProject } from '#/api/pent';
import PageHeader from '../components/PageHeader.vue';

const data = ref<any[]>([]); const loading = ref(false); const modalVisible = ref(false);
const formRef = ref();
const form = reactive({ name: '', description: '', asset_scope: '[]' });
const rules: Record<string, Rule[]> = {
  name: [{ required: true, message: '请输入项目名称', trigger: 'blur' }],
  asset_scope: [{ required: true, message: '请输入资产范围', trigger: 'blur' }],
};
const columns = [{ title:'名称',dataIndex:'name'},{ title:'描述',dataIndex:'description',ellipsis:true},{ title:'创建时间',dataIndex:'created_at',width:180},{ title:'操作',key:'actions',width:100 }];
async function fetch() { loading.value=true; try { data.value=(await getPentProjects()) as any[]; } catch { message.error('加载失败'); } finally { loading.value=false; } }
onMounted(fetch);

function openCreate() { formRef.value?.resetFields(); form.name = ''; form.description = ''; form.asset_scope = '[]'; modalVisible.value = true; }

async function handleCreate() {
  try { await formRef.value?.validate(); } catch { return; }
  let assetScope: string[];
  try { assetScope = JSON.parse(form.asset_scope || '[]'); } catch { message.error('资产范围 JSON 格式错误'); return; }
  try { await createPentProject({name: form.name, description: form.description, asset_scope: assetScope}); message.success('创建成功'); modalVisible.value=false; fetch(); } catch { message.error('创建失败'); }
}
</script>
<template>
  <div style="padding:24px;background:var(--ant-color-bg-layout);min-height:100%">
    <PageHeader title="项目管理" subtitle="管理安全测试项目与资产范围">
      <template #extra>
        <a-space><a-button @click="fetch">刷新</a-button><a-button type="primary" @click="openCreate">新建项目</a-button></a-space>
      </template>
    </PageHeader>

    <a-table :columns="columns" :data-source="data" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:15}"><template #bodyCell="{ column }"><template v-if="column.key==='actions'"><a-button type="link" size="small">详情</a-button></template></template></a-table>
    <a-modal v-model:open="modalVisible" title="新建项目" @ok="handleCreate" width="500px">
      <a-form ref="formRef" layout="vertical" :model="form" :rules="rules">
        <a-form-item label="名称" name="name"><a-input v-model:value="form.name" /></a-form-item>
        <a-form-item label="描述" name="description"><a-textarea v-model:value="form.description" /></a-form-item>
        <a-form-item label="资产范围(JSON)" name="asset_scope"><a-textarea v-model:value="form.asset_scope" placeholder='["10.0.0.0/24"]' /></a-form-item>
      </a-form>
    </a-modal>
  </div>
</template>
