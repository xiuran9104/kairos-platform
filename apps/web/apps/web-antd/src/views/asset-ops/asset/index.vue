<script lang="ts" setup>
import { ref, onMounted, computed } from 'vue';
import { Page } from '@vben/common-ui';
import { message } from 'ant-design-vue';
import { createAsset, getAssetList, updateAsset } from '#/api/scan/asset';
import type { ScanAssetApi } from '#/api/scan/asset';

type Asset = ScanAssetApi.Asset;
type UnifiedAsset = Omit<Asset, 'id'> & { id:string;record_id:number;source_type:'scan'|'cloud_platform'|'physical_inventory';source_label:string;editable:boolean;status:string;deployment_type?:string };
const data = ref<UnifiedAsset[]>([]); const loading = ref(false); const modalVisible = ref(false); const searchText = ref(''); const editingId = ref<number>();
const form = ref<Asset>({ name:'',ip:'',zone:'Intranet',ports:[],weight:50,labels:[] });

const filtered = computed(() => {
  const kw = searchText.value.toLowerCase();
  return kw ? data.value.filter(a=>a.name.toLowerCase().includes(kw)||a.ip.includes(kw)||(a.owner||'').toLowerCase().includes(kw)) : data.value;
});

const zoneInfo:Record<string,{color:string;bg:string;label:string}> = { Internet:{color:'var(--ant-color-error)',bg:'#fff1f0',label:'外网'}, DMZ:{color:'var(--ant-color-warning)',bg:'#fff7e6',label:'DMZ'}, Intranet:{color:'var(--ant-color-success)',bg:'#f6ffed',label:'内网'} };

async function fetchData() { loading.value=true; try { data.value=(await getAssetList()) as any; } catch { message.error('加载失败'); } finally { loading.value=false; } }
onMounted(fetchData);
function openCreate() { editingId.value=undefined; form.value={ name:'',ip:'',zone:'Intranet',ports:[],weight:50,labels:[] }; modalVisible.value=true; }
function openEdit(r:UnifiedAsset) { if (!r.editable) return; editingId.value=r.record_id; form.value={name:r.name,ip:r.ip,zone:r.zone,ports:r.ports,weight:r.weight,labels:r.labels,os:r.os,device_type:r.device_type,owner:r.owner,contact_person:r.contact_person,contact_phone:r.contact_phone}; modalVisible.value=true; }
async function handleSubmit() {
  if (editingId.value) { await updateAsset(editingId.value,form.value as any); message.success('已更新'); } else { await createAsset(form.value as any); message.success('已创建'); }
  modalVisible.value=false; fetchData();
}

const viewMode = ref<'grid'|'table'>('grid');
const statCards = [
  { title:'资产总数', value:()=>data.value.length, color:'var(--ant-color-primary)', bg:'#e6f4ff', icon:'lucide:hard-drive' },
  { title:'云平台资源', value:()=>data.value.filter(a=>a.source_type==='cloud_platform').length, color:'#1677ff', bg:'#e6f4ff', icon:'lucide:cloud' },
  { title:'物理基础设施', value:()=>data.value.filter(a=>a.source_type==='physical_inventory').length, color:'#722ed1', bg:'#f9f0ff', icon:'lucide:server' },
  { title:'扫描发现资产', value:()=>data.value.filter(a=>a.source_type==='scan').length, color:'var(--ant-color-success)', bg:'#f6ffed', icon:'lucide:scan-search' },
];
</script>

<template>
  <Page auto-content-height>
    <div style="padding:24px;background:var(--ant-color-bg-layout);min-height:100%">
      <a-page-header title="资产管理" sub-title="统一汇总云平台资源、物理基础设施和扫描发现资产" style="background:var(--ant-color-bg-container);border-radius:8px;margin-bottom:16px;padding:16px 24px">
        <template #extra>
          <a-space :size="16">
            <a-input-search v-model:value="searchText" placeholder="搜索资产名称/IP/负责人..." style="width:260px" allow-clear />
            <a-segmented v-model:value="viewMode" :options="[{value:'grid',label:'卡片'},{value:'table',label:'列表'}]" />
            <a-button @click="fetchData"><Icon icon="lucide:refresh-cw" /> 刷新</a-button>
            <a-button type="primary" @click="openCreate"><Icon icon="lucide:plus" /> 新建资产</a-button>
          </a-space>
        </template>
      </a-page-header>

      <a-row :gutter="[16,16]">
        <a-col :span="6" v-for="s in statCards" :key="s.title">
          <a-card size="small" style="border-radius:10px">
            <div style="display:flex;align-items:center;gap:12px">
              <div :style="{width:40,height:40,borderRadius:10,background:s.bg,display:'flex',alignItems:'center',justifyContent:'center'}">
                <Icon :icon="s.icon" :style="{fontSize:'20px',color:s.color}" />
              </div>
              <div>
                <div style="font-size:12px;color:var(--ant-color-text-tertiary)">{{ s.title }}</div>
                <div style="font-size:22px;font-weight:700;color:var(--ant-color-text)">{{ typeof s.value==='function' ? s.value() : s.value }}</div>
              </div>
            </div>
          </a-card>
        </a-col>
      </a-row>

      <!-- Grid View -->
      <a-row :gutter="[16,16]" style="margin-top:16px" v-if="viewMode==='grid'">
        <a-col :xs="24" :sm="12" :lg="8" :xl="6" v-for="item in filtered" :key="item.id">
          <a-card :hoverable="true" size="small" style="border-radius:10px;overflow:hidden">
            <template #title>
              <div style="display:flex;align-items:center;gap:6px">
                <Icon icon="lucide:server" :style="{color:zoneInfo[item.zone]?.color||'#666'}" />
                <span style="font-weight:600">{{ item.name }}</span>
                <a-tag :color="zoneInfo[item.zone]?.color" size="small" style="margin-left:auto">{{ zoneInfo[item.zone]?.label||item.zone }}</a-tag>
              </div>
            </template>
            <div style="margin-bottom:8px"><a-tag color="blue">{{ item.source_label }}</a-tag><a-tag>{{ item.device_type }}</a-tag></div>
            <div style="font-size:13px;color:#666;margin-bottom:4px"><Icon icon="lucide:wifi" style="margin-right:4px;color:#999" />{{ item.ip }}</div>
            <div style="font-size:12px;color:#999;margin-bottom:8px">
              <span v-if="item.os"><Icon icon="lucide:monitor" style="margin-right:4px" />{{ item.os }}</span>
              <span v-if="item.owner" style="margin-left:12px"><Icon icon="lucide:user" style="margin-right:4px" />{{ item.owner }}</span>
            </div>
            <div style="margin-bottom:8px">
              <a-tag v-for="p in (item.ports||[]).slice(0,8)" :key="p.port" size="small" :color="p.is_open?'blue':'default'" style="margin:2px">
                {{ p.port }}<span v-if="p.service" style="opacity:0.7">:{{ p.service }}</span>
              </a-tag>
              <a-tag v-if="(item.ports||[]).length>8" size="small" style="margin:2px">+{{ item.ports.length-8 }}</a-tag>
            </div>
            <a-row :gutter="8">
              <a-col :span="16"><a-progress :percent="item.weight" :size="20" :show-info="false" :stroke-color="item.weight>70?'var(--ant-color-error)':item.weight>40?'var(--ant-color-warning)':'var(--ant-color-success)'" /></a-col>
              <a-col :span="8"><a-space size="0"><a-button v-if="item.editable" type="link" size="small" @click="openEdit(item)">编辑</a-button><a-tag v-else>来源只读</a-tag></a-space></a-col>
            </a-row>
          </a-card>
        </a-col>
      </a-row>
      <a-empty v-if="viewMode==='grid' && !loading && !filtered.length" description="暂无资产数据" style="margin-top:60px"><a-button type="primary" @click="openCreate">创建第一个资产</a-button></a-empty>

      <!-- Table View -->
      <a-card style="border-radius:10px;margin-top:16px" size="small" v-if="viewMode==='table'">
        <a-table :columns="[{title:'名称',dataIndex:'name',width:160},{title:'资产类型',dataIndex:'device_type',width:110},{title:'来源',dataIndex:'source_label',width:150},{title:'IP/URL',dataIndex:'ip',width:150},{title:'区域',key:'zone',width:90},{title:'语言/系统',key:'technology',width:140},{title:'端口指纹',key:'ports',width:90},{title:'漏洞',key:'findings',width:70},{title:'风险评分',key:'weight',width:110},{title:'操作',key:'actions',width:150}]" :data-source="filtered" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:15,showTotal:(t:number)=>`共 ${t} 个`}">
          <template #bodyCell="{ column, record }">
            <template v-if="column.key==='zone'"><a-tag :color="zoneInfo[record.zone]?.color">{{ zoneInfo[record.zone]?.label }}</a-tag></template>
            <template v-if="column.key==='technology'">{{ record.language || record.os || '-' }}</template>
            <template v-if="column.key==='ports'">{{ (record.ports||[]).length }}</template>
            <template v-if="column.key==='findings'">{{ record.finding_count || 0 }}</template>
            <template v-if="column.key==='weight'"><a-progress :percent="record.weight" :size="20" :show-info="false" :stroke-color="record.weight>70?'var(--ant-color-error)':record.weight>40?'var(--ant-color-warning)':'var(--ant-color-success)'" style="width:60px;display:inline-block" /><span style="font-size:11px;margin-left:4px">{{ record.weight }}</span></template>
            <template v-if="column.key==='actions'"><a-space><a-button v-if="record.editable" type="link" size="small" @click="openEdit(record)">编辑</a-button><a-tag v-else>来源只读</a-tag></a-space></template>
          </template>
        </a-table>
      </a-card>

      <!-- Create/Edit Modal -->
      <a-modal v-model:open="modalVisible" :title="editingId?'编辑资产':'新建资产'" @ok="handleSubmit" width="560px">
        <a-form layout="vertical">
          <a-row :gutter="16">
            <a-col :span="14"><a-form-item label="资产名称" required><a-input v-model:value="form.name" placeholder="例如：web-prod-01" /></a-form-item></a-col>
            <a-col :span="10"><a-form-item label="网络区域" required><a-select v-model:value="form.zone"><a-select-option value="Internet">Internet 外网</a-select-option><a-select-option value="DMZ">DMZ 隔离区</a-select-option><a-select-option value="Intranet">Intranet 内网</a-select-option></a-select></a-form-item></a-col>
            <a-col :span="14"><a-form-item label="IP 地址" required><a-input v-model:value="form.ip" placeholder="192.168.1.100" /></a-form-item></a-col>
            <a-col :span="10"><a-form-item label="设备类型"><a-select v-model:value="form.device_type" show-search><a-select-option value="VM">虚拟机</a-select-option><a-select-option value="BareMetal">物理机</a-select-option><a-select-option value="Container">容器</a-select-option><a-select-option value="Network">网络设备</a-select-option></a-select></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="操作系统"><a-input v-model:value="form.os" placeholder="Ubuntu 22.04 / Windows Server 2019" /></a-form-item></a-col>
            <a-col :span="6"><a-form-item label="负责人"><a-input v-model:value="form.owner" /></a-form-item></a-col>
            <a-col :span="6"><a-form-item label="联系人"><a-input v-model:value="form.contact_person" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="标签"><a-select v-model:value="form.labels" mode="tags" placeholder="输入标签后回车" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="资产权重"><a-row align="middle"><a-col flex="auto"><a-slider v-model:value="form.weight" :min="1" :max="100" :marks="{1:'低',50:'中',100:'高'}" /></a-col></a-row></a-form-item></a-col>
          </a-row>
        </a-form>
      </a-modal>
    </div>
  </Page>
</template>
