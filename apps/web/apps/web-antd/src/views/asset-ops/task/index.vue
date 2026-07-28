<script lang="ts" setup>
import { Page } from '@vben/common-ui';
import { getTaskList, createTask, deleteTask } from '#/api/scan/task';
import { useCrudList } from '../composables/useCrudList';

interface Task { id:string; name:string; target:string; status:string; found_assets:number; found_risks:number; port_policy:string; start_time?:string; }
const { data, loading, modalVisible, searchText, form, filtered, fetchData, openCreate, handleSubmit, handleDelete } = useCrudList<Task>({
  api: { list: getTaskList, create: createTask, del: deleteTask },
  defaultForm: () => ({ name:'',target:'',port_policy:'TOP100',service_detection:true,domain_brute:false,os_detection:false,site_identify:false }),
  searchKeys: ['name', 'target'],
});
const columns = [
  { title:'名称', dataIndex:'name', width:180 },{ title:'目标', dataIndex:'target', ellipsis:true },
  { title:'状态', dataIndex:'status', width:90 },{ title:'策略', dataIndex:'port_policy', width:80 },
  { title:'资产', dataIndex:'found_assets', width:60 },{ title:'风险', dataIndex:'found_risks', width:60 },
  { title:'时间', dataIndex:'start_time', width:160 },{ title:'操作', key:'actions', width:80, fixed:'right' },
];
const stC:Record<string,string>={pending:'default',running:'processing',completed:'green',failed:'red'};
const stL:Record<string,string>={pending:'待执行',running:'运行中',completed:'已完成',failed:'失败'};
</script>

<template>
  <Page auto-content-height>
    <div style="padding:24px;background:var(--ant-color-bg-layout);min-height:100%">
      <a-page-header title="扫描任务" sub-title="管理扫描任务 · 配置扫描策略" style="background:var(--ant-color-bg-container);border-radius:8px;margin-bottom:16px;padding:16px 24px">
        <template #extra><a-space><a-input-search v-model:value="searchText" placeholder="搜索任务..." style="width:200px" /><a-button @click="fetchData"><Icon icon="lucide:refresh-cw" /> 刷新</a-button><a-button type="primary" @click="openCreate()"><Icon icon="lucide:plus" /> 新建任务</a-button></a-space></template>
      </a-page-header>
      <a-card style="border-radius:8px" size="small">
        <a-table :columns="columns" :data-source="filtered" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:15}">
          <template #bodyCell="{ column, record }">
            <template v-if="column.key==='status'"><a-badge :status="record.status==='running'?'processing':record.status==='completed'?'success':record.status==='failed'?'error':'default'" /><a-tag :color="stC[record.status]" style="margin-left:4px">{{ stL[record.status]||record.status }}</a-tag></template>
            <template v-if="column.key==='actions'"><a-popconfirm title="确认删除?" @confirm="handleDelete(record.id)"><a-button type="link" size="small" danger>删除</a-button></a-popconfirm></template>
          </template>
        </a-table>
      </a-card>
      <a-modal v-model:open="modalVisible" title="新建扫描任务" @ok="handleSubmit" width="500px">
        <a-form layout="vertical">
          <a-form-item label="任务名称" required><a-input v-model:value="form.name" placeholder="例如：核心网段扫描" /></a-form-item>
          <a-form-item label="目标" required><a-textarea v-model:value="form.target" placeholder="192.168.1.0/24, 10.0.0.1-10.0.0.255" :rows="2" /></a-form-item>
          <a-row :gutter="16">
            <a-col :span="12"><a-form-item label="端口策略"><a-select v-model:value="form.port_policy"><a-select-option value="TOP100">TOP 100</a-select-option><a-select-option value="TOP1000">TOP 1000</a-select-option><a-select-option value="ALL">全端口</a-select-option></a-select></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="服务识别"><a-switch v-model:checked="form.service_detection" /></a-form-item></a-col>
          </a-row>
        </a-form>
      </a-modal>
    </div>
  </Page>
</template>
