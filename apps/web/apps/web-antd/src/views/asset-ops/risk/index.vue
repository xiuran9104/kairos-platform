<script lang="ts" setup>
import { computed, ref, onMounted } from 'vue';
import { Page } from '@vben/common-ui';
import { message } from 'ant-design-vue';
import { getRiskList, updateRisk } from '#/api/scan/risk';

interface Risk { id:string; asset_ip:string; port:number; severity:string; description:string; solution?:string; status:string; assigned_to?:string; }
const data = ref<Risk[]>([]);
const loading = ref(false);
const detailVisible = ref(false);
const detailRisk = ref<Risk|null>(null);
const severityFilter = ref('');

const columns = [
  { title:'资产IP', dataIndex:'asset_ip', key:'asset_ip', width:140 },
  { title:'端口', dataIndex:'port', key:'port', width:70 },
  { title:'严重', dataIndex:'severity', key:'severity', width:80 },
  { title:'描述', dataIndex:'description', key:'description', ellipsis:true },
  { title:'状态', dataIndex:'status', key:'status', width:90 },
  { title:'负责人', dataIndex:'assigned_to', key:'assigned_to', width:90 },
  { title:'操作', key:'actions', width:150, fixed:'right' },
];

const sevColor:Record<string,string>={Critical:'#f5222d',High:'#fa8c16',Medium:'#faad14',Low:'#52c41a'};
const stColor:Record<string,string>={open:'red',verified:'orange',resolved:'green',ignored:'default',false_positive:'purple',pending_review:'blue'};
const stLabel:Record<string,string>={open:'未处理',verified:'已确认',resolved:'已解决',ignored:'已忽略',false_positive:'误报',pending_review:'待审核'};

const filtered = computed(() => severityFilter.value
  ? data.value.filter(r => r.severity === severityFilter.value)
  : data.value);

async function fetchData() { loading.value=true; try { data.value=(await getRiskList()) as any; } catch { message.error('加载失败'); } finally { loading.value=false; } }
onMounted(fetchData);
function showDetail(r:Risk) { detailRisk.value=r; detailVisible.value=true; }
async function handleStatus(r:Risk,status:string) { await updateRisk(r.id,{status}); message.success('状态已更新'); fetchData(); }
</script>

<template>
  <Page auto-content-height>
    <div style="padding:24px;background:#f5f5f5;min-height:100%">
      <a-page-header title="风险管理" sub-title="漏洞风险监控与处置" style="background:#fff;border-radius:8px;margin-bottom:16px;padding:16px 24px">
        <template #extra>
          <a-space>
            <a-select v-model:value="severityFilter" placeholder="严重程度筛选" allow-clear style="width:140px" @change="fetchData">
              <a-select-option value="Critical">严重</a-select-option><a-select-option value="High">高危</a-select-option><a-select-option value="Medium">中危</a-select-option><a-select-option value="Low">低危</a-select-option>
            </a-select>
            <a-button @click="fetchData"><Icon icon="lucide:refresh-cw" /> 刷新</a-button>
          </a-space>
        </template>
      </a-page-header>

      <a-row :gutter="16" style="margin-bottom:16px">
        <a-col :span="6"><a-card size="small" style="border-radius:8px;border-left:3px solid #f5222d"><a-statistic title="严重" :value="data.filter(r=>r.severity==='Critical').length" :value-style="{color:'#f5222d'}" /></a-card></a-col>
        <a-col :span="6"><a-card size="small" style="border-radius:8px;border-left:3px solid #fa8c16"><a-statistic title="高危" :value="data.filter(r=>r.severity==='High').length" :value-style="{color:'#fa8c16'}" /></a-card></a-col>
        <a-col :span="6"><a-card size="small" style="border-radius:8px;border-left:3px solid #faad14"><a-statistic title="中危" :value="data.filter(r=>r.severity==='Medium').length" :value-style="{color:'#faad14'}" /></a-card></a-col>
        <a-col :span="6"><a-card size="small" style="border-radius:8px;border-left:3px solid #52c41a"><a-statistic title="低危" :value="data.filter(r=>r.severity==='Low').length" :value-style="{color:'#52c41a'}" /></a-card></a-col>
      </a-row>

      <a-card style="border-radius:8px" size="small">
        <a-table :columns="columns" :data-source="filtered" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:20}">
          <template #bodyCell="{ column, record }">
            <template v-if="column.key==='severity'"><a-tag :color="sevColor[record.severity]||'default'">{{ record.severity }}</a-tag></template>
            <template v-if="column.key==='status'"><a-tag :color="stColor[record.status]">{{ stLabel[record.status]||record.status }}</a-tag></template>
            <template v-if="column.key==='actions'">
              <a-space>
                <a-button type="link" size="small" @click="showDetail(record)">详情</a-button>
                <a-dropdown>
                  <a-button type="link" size="small">处置 <Icon icon="lucide:chevron-down" /></a-button>
                  <template #overlay>
                    <a-menu @click="({key}:any)=>handleStatus(record,key)">
                      <a-menu-item key="verified">确认风险</a-menu-item>
                      <a-menu-item key="resolved">标记已解决</a-menu-item>
                      <a-menu-item key="ignored">忽略</a-menu-item>
                      <a-menu-item key="false_positive">误报</a-menu-item>
                    </a-menu>
                  </template>
                </a-dropdown>
              </a-space>
            </template>
          </template>
        </a-table>
      </a-card>

      <a-modal v-model:open="detailVisible" title="风险详情" width="560px" :footer="null">
        <template v-if="detailRisk">
          <a-descriptions bordered size="small" :column="1" style="margin-bottom:16px">
            <a-descriptions-item label="资产IP">{{ detailRisk.asset_ip }}</a-descriptions-item>
            <a-descriptions-item label="端口">{{ detailRisk.port }}</a-descriptions-item>
            <a-descriptions-item label="严重程度"><a-tag :color="sevColor[detailRisk.severity]">{{ detailRisk.severity }}</a-tag></a-descriptions-item>
            <a-descriptions-item label="状态"><a-tag :color="stColor[detailRisk.status]">{{ stLabel[detailRisk.status] }}</a-tag></a-descriptions-item>
            <a-descriptions-item label="负责人">{{ detailRisk.assigned_to||'未分配' }}</a-descriptions-item>
          </a-descriptions>
          <a-card title="风险描述" size="small" style="margin-bottom:12px">{{ detailRisk.description }}</a-card>
          <a-card title="解决方案" size="small">{{ detailRisk.solution||'暂无解决方案，建议人工评估' }}</a-card>
        </template>
      </a-modal>
    </div>
  </Page>
</template>
