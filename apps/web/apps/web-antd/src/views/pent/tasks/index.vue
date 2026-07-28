<script lang="ts" setup>
import { reactive, ref, onMounted } from 'vue';

import { Form, message } from 'ant-design-vue';
import type { Rule } from 'ant-design-vue/es/form';
import { getPentTasks, createPentTask, cancelPentTask, deletePentTask, createPentRun } from '#/api/pent';
import PageHeader from '../components/PageHeader.vue';

const data = ref<any[]>([]); const loading = ref(false); const modalVisible = ref(false);
const formRef = ref();
const form = reactive({ name: '', target_scope: '["10.0.0.0/24"]', priority: 'medium', authorization_confirmed: false });
const rules: Record<string, Rule[]> = {
  name: [{ required: true, message: '请输入任务名称', trigger: 'blur' }],
  target_scope: [{ required: true, message: '请输入目标范围', trigger: 'blur' }],
  authorization_confirmed: [{ validator: (_rule, value) => value ? Promise.resolve() : Promise.reject('请确认测试授权'), trigger: 'change' }],
};
const columns = [{ title:'名称',dataIndex:'name'},{ title:'状态',dataIndex:'status',width:90},{ title:'优先级',dataIndex:'priority',width:80},{ title:'创建时间',dataIndex:'created_at',width:180},{ title:'操作',key:'actions',width:200 }];
const priorityColor:Record<string,string>={low:'green',medium:'orange',high:'red',critical:'var(--ant-color-error)'};
async function fetch() { loading.value=true; try { data.value=(await getPentTasks()) as any[]; } catch { message.error('加载失败'); } finally { loading.value=false; } }
onMounted(fetch);

function openCreate() { formRef.value?.resetFields(); form.name = ''; form.target_scope = '["10.0.0.0/24"]'; form.priority = 'medium'; form.authorization_confirmed = false; modalVisible.value = true; }

async function handleCreate() {
  try { await formRef.value?.validate(); } catch { return; }
  let targetScope: string[];
  try { targetScope = JSON.parse(form.target_scope || '[]'); } catch { message.error('目标范围 JSON 格式错误'); return; }
  try { await createPentTask({name: form.name, target_scope: targetScope, priority: form.priority, authorization_confirmed: form.authorization_confirmed}); message.success('创建成功'); modalVisible.value=false; fetch(); } catch { message.error('创建失败'); }
}
async function handleRun(task:any) { try { await createPentRun({task_id:task.id}); message.success('Run 已创建'); } catch { message.error('创建 Run 失败'); } }
async function handleDelete(task:any) { try { await deletePentTask(task.id); message.success('已删除'); fetch(); } catch { message.error('删除失败'); } }
</script>
<template>
  <div style="padding:24px;background:var(--ant-color-bg-layout);min-height:100%">
    <PageHeader title="任务管理" subtitle="创建、执行与取消安全测试任务">
      <template #extra>
        <a-space><a-button @click="fetch">刷新</a-button><a-button type="primary" @click="openCreate">新建任务</a-button></a-space>
      </template>
    </PageHeader>

    <a-table :columns="columns" :data-source="data" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:15}">
      <template #bodyCell="{ column, record }">
        <template v-if="column.key==='priority'"><a-tag :color="priorityColor[record.priority]||'default'">{{ record.priority }}</a-tag></template>
        <template v-if="column.key==='actions'"><a-space><a-button type="link" size="small" @click="handleRun(record)">执行</a-button><a-button type="link" size="small" danger @click="cancelPentTask(record.id).then(fetch)">取消</a-button><a-popconfirm title="确定删除？" @confirm="handleDelete(record)"><a-button type="link" size="small" danger>删除</a-button></a-popconfirm></a-space></template>
      </template>
    </a-table>
    <a-modal v-model:open="modalVisible" title="新建任务" @ok="handleCreate" width="500px">
      <a-form ref="formRef" layout="vertical" :model="form" :rules="rules">
        <a-form-item label="名称" name="name"><a-input v-model:value="form.name" /></a-form-item>
        <a-form-item label="目标范围" name="target_scope"><a-textarea v-model:value="form.target_scope" /></a-form-item>
        <a-form-item label="优先级" name="priority"><a-select v-model:value="form.priority"><a-select-option value="low">低</a-select-option><a-select-option value="medium">中</a-select-option><a-select-option value="high">高</a-select-option><a-select-option value="critical">紧急</a-select-option></a-select></a-form-item>
        <a-form-item name="authorization_confirmed"><a-checkbox v-model:checked="form.authorization_confirmed">我确认已获得上述目标的安全测试授权</a-checkbox></a-form-item>
      </a-form>
    </a-modal>
  </div>
</template>
