<script lang="ts" setup>
import { onMounted, ref } from 'vue';
import { Page } from '@vben/common-ui';
import { message } from 'ant-design-vue';
import { getTaskList, createTask, deleteTask } from '#/api/scan/task';
import { getScannerList } from '#/api/scan/scanner';
import { useCrudList } from '../composables/useCrudList';

interface Task { id:string; scanner_id?:string; name:string; target:string; status:string; found_assets:number; found_risks:number; port_policy:string; service_detection:boolean; domain_brute:boolean; os_detection:boolean; site_identify:boolean; start_time?:string; }
const scanners=ref<any[]>([]);
const { loading, modalVisible, searchText, form, filtered, fetchData, openCreate, handleSubmit, handleDelete } = useCrudList<Task>({
  api: { list: getTaskList, create: createTask, del: (id)=>deleteTask(String(id)) },
  defaultForm: () => ({ id:'',scanner_id:undefined,name:'',target:'',status:'pending',found_assets:0,found_risks:0,port_policy:'TOP100',service_detection:true,domain_brute:false,os_detection:false,site_identify:false }),
  searchKeys: ['name', 'target'],
});
onMounted(async()=>{try{scanners.value=await getScannerList() as any;}catch{message.error('扫描节点加载失败');}});
function openTaskCreate(){const online=scanners.value.find(item=>item.enabled&&item.agent_status==='online');if(!online){message.warning('请先注册一个在线的扫描节点');return;}openCreate();form.value!.scanner_id=online.id;}
const columns = [
  { title:'名称', dataIndex:'name', width:180 },{ title:'扫描节点', key:'scanner', width:160 },{ title:'目标', dataIndex:'target', ellipsis:true },
  { title:'状态', dataIndex:'status', width:90 },{ title:'策略', dataIndex:'port_policy', width:80 },
  { title:'资产', dataIndex:'found_assets', width:60 },{ title:'风险', dataIndex:'found_risks', width:60 },
  { title:'时间', dataIndex:'start_time', width:160 },{ title:'操作', key:'actions', width:80, fixed:'right' },
];
const stC:Record<string,string>={pending:'default',running:'processing',completed:'green',failed:'red'};
const stL:Record<string,string>={pending:'待执行',running:'运行中',completed:'已完成',failed:'失败'};
</script>

<template>
  <Page auto-content-height>
    <div style="padding:16px">
      <a-page-header title="扫描任务" sub-title="选择扫描节点，配置目标和扫描策略" style="margin-bottom:16px;padding:0" />
      <a-space :size="20" style="margin-bottom:20px" wrap><a-button type="primary" @click="openTaskCreate"><Icon icon="lucide:plus" /> 创建扫描任务</a-button><a-input-search v-model:value="searchText" placeholder="搜索任务或目标" style="width:260px" allow-clear/><a-button @click="fetchData"><Icon icon="lucide:refresh-cw" /> 刷新</a-button></a-space>
      <a-card style="border-radius:8px" size="small">
        <a-table :columns="columns" :data-source="filtered" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:15}">
          <template #bodyCell="{ column, record }">
            <template v-if="column.key==='scanner'">{{scanners.find(item=>item.id===record.scanner_id)?.name||'未指定'}}</template>
            <template v-if="column.key==='status'"><a-badge :status="record.status==='running'?'processing':record.status==='completed'?'success':record.status==='failed'?'error':'default'" /><a-tag :color="stC[record.status]" style="margin-left:4px">{{ stL[record.status]||record.status }}</a-tag></template>
            <template v-if="column.key==='actions'"><a-popconfirm title="确认删除?" @confirm="handleDelete(record.id)"><a-button type="link" size="small" danger>删除</a-button></a-popconfirm></template>
          </template>
        </a-table>
      </a-card>
      <a-modal v-model:open="modalVisible" title="新建扫描任务" @ok="handleSubmit" width="500px">
        <a-form v-if="form" layout="vertical">
          <a-form-item label="任务名称" required><a-input v-model:value="form.name" placeholder="例如：核心网段扫描" /></a-form-item>
          <a-form-item label="扫描节点" required><a-select v-model:value="form.scanner_id" placeholder="请选择在线节点" :options="scanners.filter(item=>item.enabled&&item.agent_status==='online').map(item=>({value:item.id,label:`${item.name} / RustScan`}))" /></a-form-item>
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
