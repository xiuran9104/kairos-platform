<script lang="ts" setup>
import { ref, onMounted } from 'vue';

import { message } from 'ant-design-vue';
import { getPentTasks, createPentTask, cancelPentTask, deletePentTask, createPentRun } from '#/api/pent';
const data = ref<any[]>([]); const loading = ref(false); const modalVisible = ref(false);
const form = ref({ name: '', target_scope: '["10.0.0.0/24"]', priority: 'medium', authorization_confirmed: false });
const columns = [{ title:'名称',dataIndex:'name'},{ title:'状态',dataIndex:'status',width:90},{ title:'优先级',dataIndex:'priority',width:80},{ title:'创建时间',dataIndex:'created_at',width:180},{ title:'操作',key:'actions',width:200 }];
const priorityColor:Record<string,string>={low:'green',medium:'orange',high:'red',critical:'#f5222d'};
async function fetch() { loading.value=true; try { data.value=(await getPentTasks()) as any[]; } catch { message.error('加载失败'); } finally { loading.value=false; } }
onMounted(fetch);
async function handleCreate() {
  if (!form.value.authorization_confirmed) { message.warning('请先确认目标测试授权'); return; }
  await createPentTask({...form.value,target_scope:JSON.parse(form.value.target_scope||'[]')});
  message.success('创建成功'); modalVisible.value=false; fetch();
}
async function handleRun(task:any) { await createPentRun({task_id:task.id}); message.success('Run 已创建'); }
async function handleDelete(task:any) { await deletePentTask(task.id); message.success('已删除'); fetch(); }
</script>
<template>
  <div style="padding:24px;background:#f5f5f5;min-height:100%">
    <div style="background:#fff;border-radius:8px;margin-bottom:16px;padding:16px 24px;display:flex;justify-content:space-between;align-items:center">
      <div><h2 style="margin:0">任务管理</h2><span style="color:#8c8c8c;font-size:13px">创建、执行与取消安全测试任务</span></div>
      <a-space><a-button @click="fetch">刷新</a-button><a-button type="primary" @click="modalVisible=true">新建任务</a-button></a-space>
    </div>

    <a-table :columns="columns" :data-source="data" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:15}">
      <template #bodyCell="{ column, record }">
        <template v-if="column.key==='priority'"><a-tag :color="priorityColor[record.priority]||'default'">{{ record.priority }}</a-tag></template>
        <template v-if="column.key==='actions'"><a-space><a-button type="link" size="small" @click="handleRun(record)">执行</a-button><a-button type="link" size="small" danger @click="cancelPentTask(record.id).then(fetch)">取消</a-button><a-popconfirm title="确定删除？" @confirm="handleDelete(record)"><a-button type="link" size="small" danger>删除</a-button></a-popconfirm></a-space></template>
      </template>
    </a-table>
    <a-modal v-model:open="modalVisible" title="新建任务" @ok="handleCreate" width="500px">
      <a-form layout="vertical">
        <a-form-item label="名称" required><a-input v-model:value="form.name" /></a-form-item>
        <a-form-item label="目标范围" required><a-textarea v-model:value="form.target_scope" /></a-form-item>
        <a-form-item label="优先级"><a-select v-model:value="form.priority"><a-select-option value="low">低</a-select-option><a-select-option value="medium">中</a-select-option><a-select-option value="high">高</a-select-option><a-select-option value="critical">紧急</a-select-option></a-select></a-form-item>
        <a-form-item required><a-checkbox v-model:checked="form.authorization_confirmed">我确认已获得上述目标的安全测试授权</a-checkbox></a-form-item>
      </a-form>
    </a-modal>
  </div>
</template>
