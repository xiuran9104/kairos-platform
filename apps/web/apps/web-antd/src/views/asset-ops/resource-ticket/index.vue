<script lang="ts" setup>
import { ref, onMounted } from 'vue';
import { Page } from '@vben/common-ui';
import { message } from 'ant-design-vue';
import { getResourceTicketList, createResourceTicket, deleteResourceTicket, approveTicket, deliverTicket } from '#/api/scan/resource-ticket';

interface Ticket { id?:number; ecs_name:string; resource_type:string; ticket_status:string; customer_name?:string; cloud_region?:string; cpu_cores:number; memory_gb:number; created_at:string; approver?:string; deliverer?:string; ecs_type?:string; ecs_os?:string; ip_address?:string; }

const data = ref<Ticket[]>([]);
const loading = ref(false);
const detailVisible = ref(false);
const detailTicket = ref<Ticket|null>(null);
const createVisible = ref(false);
const createForm = ref({ ecs_name:'',resource_type:'cloud',cloud_region:'',customer_name:'',cpu_cores:4,memory_gb:8 });

const columns = [
  { title:'工单名称', dataIndex:'ecs_name', width:160 },
  { title:'类型', dataIndex:'resource_type', width:70 },
  { title:'状态', dataIndex:'ticket_status', width:90 },
  { title:'客户', dataIndex:'customer_name', width:100 },
  { title:'区域', dataIndex:'cloud_region', width:100 },
  { title:'规格', key:'spec', width:100 },
  { title:'创建时间', dataIndex:'created_at', width:160 },
  { title:'操作', key:'actions', width:240, fixed:'right' },
];

const statusMap:Record<string,{color:string;label:string;icon:string}> = {
  pending_approval:{color:'orange',label:'待审批',icon:'lucide:clock'},
  approved:{color:'blue',label:'已批准',icon:'lucide:check'},
  rejected:{color:'red',label:'已拒绝',icon:'lucide:x'},
  pending_provision:{color:'cyan',label:'待配置',icon:'lucide:settings'},
  provisioning:{color:'processing',label:'配置中',icon:'lucide:loader'},
  pending_delivery:{color:'purple',label:'待交付',icon:'lucide:package'},
  delivered:{color:'green',label:'已交付',icon:'lucide:truck'},
  archived:{color:'default',label:'已归档',icon:'lucide:archive'},
};
const stepIndex:Record<string,number> = { pending_approval:0,approved:1,pending_provision:2,provisioning:2,pending_delivery:3,delivered:4 };

async function fetchData() { loading.value=true; try { data.value=(await getResourceTicketList()) as any; } catch { message.error('加载失败'); } finally { loading.value=false; } }
onMounted(fetchData);

function showDetail(t:Ticket) { detailTicket.value=t; detailVisible.value=true; }
async function handleCreate() { await createResourceTicket(createForm.value); message.success('工单已创建'); createVisible.value=false; fetchData(); }
async function handleApprove(t:Ticket) { await approveTicket(t.id!,{approved:true,comment:'审批通过'}); message.success('已批准'); fetchData(); }
async function handleReject(t:Ticket) { await approveTicket(t.id!,{approved:false,comment:'审批拒绝'}); message.success('已拒绝'); fetchData(); }
async function handleDeliver(t:Ticket) { await deliverTicket(t.id!,{comment:'交付完成'}); message.success('已交付'); fetchData(); }

const typeLabel:Record<string,string> = { cloud:'云资源', physical:'物理机', network:'网络策略' };
</script>

<template>
  <Page auto-content-height>
    <div style="padding:24px;background:#f5f5f5;min-height:100%">
      <a-page-header title="资源工单" sub-title="申请 · 审批 · 配置 · 交付 全流程管理" style="background:#fff;border-radius:8px;margin-bottom:16px;padding:16px 24px">
        <template #extra>
          <a-space>
            <a-button @click="fetchData"><Icon icon="lucide:refresh-cw" /> 刷新</a-button>
            <a-button type="primary" @click="createVisible=true"><Icon icon="lucide:plus" /> 新建工单</a-button>
          </a-space>
        </template>
      </a-page-header>

      <a-row :gutter="16" style="margin-bottom:16px">
        <a-col :span="6" v-for="(s,key) in {pending_approval:'待审批',approved:'已批准',provisioning:'配置中',pending_delivery:'待交付'}" :key="key">
          <a-card size="small" style="border-radius:8px">
            <a-statistic :title="s as string" :value="data.filter(t=>t.ticket_status===key).length" :value-style="{color:statusMap[key]?.color||'#666'}" />
          </a-card>
        </a-col>
      </a-row>

      <a-card style="border-radius:8px" size="small">
        <a-table :columns="columns" :data-source="data" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:15}">
          <template #bodyCell="{ column, record }">
            <template v-if="column.key==='resource_type'"><a-tag>{{ typeLabel[record.resource_type]||record.resource_type }}</a-tag></template>
            <template v-if="column.key==='ticket_status'"><a-tag :color="statusMap[record.ticket_status]?.color"><Icon :icon="statusMap[record.ticket_status]?.icon" style="margin-right:4px" />{{ statusMap[record.ticket_status]?.label }}</a-tag></template>
            <template v-if="column.key==='spec'">{{ record.cpu_cores }}核 {{ record.memory_gb }}GB</template>
            <template v-if="column.key==='actions'">
              <a-space>
                <a-button type="link" size="small" @click="showDetail(record)">详情</a-button>
                <a-button v-if="record.ticket_status==='pending_approval'" type="link" size="small" style="color:#52c41a" @click="handleApprove(record)">批准</a-button>
                <a-button v-if="record.ticket_status==='pending_approval'" type="link" size="small" danger @click="handleReject(record)">拒绝</a-button>
                <a-button v-if="record.ticket_status==='pending_delivery'" type="link" size="small" style="color:#1677ff" @click="handleDeliver(record)">交付</a-button>
                <a-popconfirm title="确认删除?" @confirm="deleteResourceTicket(record.id!).then(fetchData)"><a-button type="link" size="small" danger>删除</a-button></a-popconfirm>
              </a-space>
            </template>
          </template>
        </a-table>
      </a-card>

      <a-modal v-model:open="detailVisible" title="工单详情" width="700px" :footer="null">
        <template v-if="detailTicket">
          <a-steps :current="stepIndex[detailTicket.ticket_status]||0" size="small" style="margin-bottom:20px">
            <a-step title="待审批" description="提交申请" />
            <a-step title="已批准" description="审批通过" />
            <a-step title="配置中" description="资源编排" />
            <a-step title="待交付" description="交付确认" />
            <a-step title="已交付" description="完成" />
          </a-steps>
          <a-descriptions bordered size="small" :column="2">
            <a-descriptions-item label="工单名称">{{ detailTicket.ecs_name }}</a-descriptions-item>
            <a-descriptions-item label="状态"><a-tag :color="statusMap[detailTicket.ticket_status]?.color">{{ statusMap[detailTicket.ticket_status]?.label }}</a-tag></a-descriptions-item>
            <a-descriptions-item label="类型">{{ typeLabel[detailTicket.resource_type]||detailTicket.resource_type }}</a-descriptions-item>
            <a-descriptions-item label="区域">{{ detailTicket.cloud_region||'-' }}</a-descriptions-item>
            <a-descriptions-item label="规格">{{ detailTicket.ecs_type||'-' }} / {{ detailTicket.ecs_os||'-' }}</a-descriptions-item>
            <a-descriptions-item label="IP">{{ detailTicket.ip_address||'-' }}</a-descriptions-item>
            <a-descriptions-item label="审批人">{{ detailTicket.approver||'-' }}</a-descriptions-item>
            <a-descriptions-item label="交付人">{{ detailTicket.deliverer||'-' }}</a-descriptions-item>
          </a-descriptions>
        </template>
      </a-modal>

      <a-modal v-model:open="createVisible" title="新建资源工单" @ok="handleCreate" width="500px">
        <a-form layout="vertical">
          <a-form-item label="工单名称" required><a-input v-model:value="createForm.ecs_name" placeholder="例如：生产环境Web服务器" /></a-form-item>
          <a-form-item label="资源类型"><a-select v-model:value="createForm.resource_type"><a-select-option value="cloud">云资源</a-select-option><a-select-option value="physical">物理机</a-select-option><a-select-option value="network">网络策略</a-select-option></a-select></a-form-item>
          <a-form-item label="区域"><a-input v-model:value="createForm.cloud_region" placeholder="华北区" /></a-form-item>
          <a-form-item label="客户"><a-input v-model:value="createForm.customer_name" /></a-form-item>
          <a-row :gutter="16">
            <a-col :span="12"><a-form-item label="CPU核数"><a-input-number v-model:value="createForm.cpu_cores" :min="1" :max="128" style="width:100%" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="内存(GB)"><a-input-number v-model:value="createForm.memory_gb" :min="1" :max="1024" style="width:100%" /></a-form-item></a-col>
          </a-row>
        </a-form>
      </a-modal>
    </div>
  </Page>
</template>
