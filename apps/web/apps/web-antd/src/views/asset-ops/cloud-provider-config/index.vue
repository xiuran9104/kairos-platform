<script lang="ts" setup>
import { ref, onMounted } from 'vue';
import { Page } from '@vben/common-ui';
import { message } from 'ant-design-vue';
import { getCloudProviderConfigList, createCloudProviderConfig, updateCloudProviderConfig, deleteCloudProviderConfig, testCloudConnection, syncCloudInstances, syncCloudFirewalls } from '#/api/scan/cloud-provider-config';

interface Config { id?: number; provider: string; region_id: string; region_name: string; account_name: string; access_key_id: string; access_key_secret?: string; status: string; available_zones?: string[]; }

const data = ref<Config[]>([]);
const loading = ref(false);
const modalVisible = ref(false);
const syncing = ref<number|null>(null);
const editingId = ref<number>();
const form = ref<Config>({ provider:'openstack',region_id:'',region_name:'',account_name:'',access_key_id:'',access_key_secret:'',status:'active' });

const providerMap:Record<string,{name:string;icon:string;color:string;bg:string}> = {
  openstack:{name:'OpenStack',icon:'lucide:cloud',color:'#ed1941',bg:'#fff1f0'},
  hcs:{name:'华为HCS',icon:'lucide:cloud-cog',color:'#cf0a2c',bg:'#fff1f0'},
  apsara:{name:'阿里Apsara',icon:'lucide:cloud-sun',color:'#ff6a00',bg:'#fff7e6'},
  tstack:{name:'TStack',icon:'lucide:cloud-lightning',color:'#0052d9',bg:'#e6f4ff'},
};

async function fetchData() { loading.value=true; try { data.value=(await getCloudProviderConfigList()) as any; } catch { message.error('加载失败'); } finally { loading.value=false; } }
onMounted(fetchData);

function openCreate() { editingId.value=undefined; form.value={ provider:'openstack',region_id:'',region_name:'',account_name:'',access_key_id:'',access_key_secret:'',status:'active' }; modalVisible.value=true; }
function openEdit(r:Config) { editingId.value=r.id; form.value={...r}; modalVisible.value=true; }
async function handleSubmit() {
  if (editingId.value) { await updateCloudProviderConfig(editingId.value,form.value as any); message.success('更新成功'); }
  else { await createCloudProviderConfig(form.value as any); message.success('创建成功'); }
  modalVisible.value=false; fetchData();
}
async function handleTest(r:Config) {
  const hide = message.loading({content:'测试连接中...',duration:0});
  try { const res:any = await testCloudConnection(r.id!); hide(); message.success(res?.message||'连接正常'); } catch { hide(); message.error('连接失败'); }
}
async function handleSync(r:Config) {
  syncing.value=r.id!; const hide = message.loading({content:'同步VPS中...',duration:0});
  try { await syncCloudInstances(r.id!); hide(); message.success('VPS同步完成'); } catch { hide(); message.error('VPS同步失败'); } finally { syncing.value=null; }
}
async function handleSyncFw(r:Config) {
  syncing.value=r.id!; const hide = message.loading({content:'同步防火墙中...',duration:0});
  try { await syncCloudFirewalls(r.id!); hide(); message.success('防火墙同步完成'); } catch { hide(); message.error('防火墙同步失败'); } finally { syncing.value=null; }
}
</script>

<template>
  <Page auto-content-height>
    <div style="padding:24px;background:var(--ant-color-bg-layout);min-height:100%">
      <a-page-header title="云厂商对接" sub-title="管理私有云平台接入与资源同步" style="background:var(--ant-color-bg-container);border-radius:8px;margin-bottom:16px;padding:16px 24px">
        <template #extra>
          <a-space>
            <a-button @click="fetchData" :loading="loading"><Icon icon="lucide:refresh-cw" /> 刷新</a-button>
            <a-button type="primary" @click="openCreate"><Icon icon="lucide:plus" /> 新建对接</a-button>
          </a-space>
        </template>
      </a-page-header>

      <a-row :gutter="[16,16]">
        <a-col :xs="24" :md="12" :lg="8" v-for="item in data" :key="item.id">
          <a-card :hoverable="true" style="border-radius:10px;border-top:3px solid" :style="{'border-top-color':providerMap[item.provider]?.color||'var(--ant-color-border-secondary)'}">
            <template #title>
              <div style="display:flex;align-items:center;gap:8px">
                <div :style="{width:36,height:36,borderRadius:8,background:providerMap[item.provider]?.bg||'var(--ant-color-fill-secondary)',display:'flex',alignItems:'center',justifyContent:'center'}">
                  <Icon :icon="providerMap[item.provider]?.icon||'lucide:cloud'" :style="{fontSize:'20px',color:providerMap[item.provider]?.color||'var(--ant-color-text-tertiary)'}" />
                </div>
                <div>
                  <span style="font-weight:600">{{ providerMap[item.provider]?.name||item.provider }}</span>
                  <div style="font-size:11px;color:var(--ant-color-text-tertiary)">{{ item.region_name }} / {{ item.account_name }}</div>
                </div>
              </div>
            </template>
            <template #extra>
              <a-tag :color="item.status==='active'?'green':item.status==='testing'?'blue':'red'">{{ item.status==='active'?'已启用':item.status==='testing'?'测试中':'已停用' }}</a-tag>
            </template>
            <a-descriptions :column="1" size="small" style="margin-bottom:12px">
              <a-descriptions-item label="端点">{{ item.region_id }}</a-descriptions-item>
              <a-descriptions-item label="可用区">{{ (item.available_zones||[]).join(', ') || '-' }}</a-descriptions-item>
            </a-descriptions>
            <a-space direction="vertical" style="width:100%">
              <a-row :gutter="8">
                <a-col :span="8"><a-button block size="small" @click="handleTest(item)">测试连接</a-button></a-col>
                <a-col :span="8"><a-button block size="small" type="primary" @click="handleSync(item)" :loading="syncing===item.id">同步VPS</a-button></a-col>
                <a-col :span="8"><a-button block size="small" @click="handleSyncFw(item)" :loading="syncing===item.id">同步防火墙</a-button></a-col>
              </a-row>
              <a-row :gutter="8">
                <a-col :span="12"><a-button block size="small" @click="openEdit(item)">编辑</a-button></a-col>
                <a-col :span="12"><a-popconfirm title="确认删除?" @confirm="deleteCloudProviderConfig(item.id!).then(fetchData)"><a-button block size="small" danger>删除</a-button></a-popconfirm></a-col>
              </a-row>
            </a-space>
          </a-card>
        </a-col>
      </a-row>
      <a-empty v-if="!loading && !data.length" description="暂无云厂商对接配置" style="margin-top:80px">
        <a-button type="primary" @click="openCreate">立即创建</a-button>
      </a-empty>

      <a-modal v-model:open="modalVisible" :title="editingId?'编辑云厂商对接':'新建云厂商对接'" @ok="handleSubmit" width="560px">
        <a-form layout="vertical">
          <a-form-item label="云厂商" required>
            <a-select v-model:value="form.provider">
              <a-select-option value="openstack">OpenStack 私有云</a-select-option>
              <a-select-option value="hcs">华为 HCS</a-select-option>
              <a-select-option value="apsara">阿里 Apsara Stack</a-select-option>
              <a-select-option value="tstack">TStack</a-select-option>
            </a-select>
          </a-form-item>
          <a-form-item label="端点地址" required><a-input v-model:value="form.region_id" placeholder="https://openstack.example.com:5000/v3" /></a-form-item>
          <a-form-item label="区域名称" required><a-input v-model:value="form.region_name" placeholder="华北区 / region-1" /></a-form-item>
          <a-form-item label="账号" required><a-input v-model:value="form.account_name" /></a-form-item>
          <a-form-item label="Access Key ID"><a-input v-model:value="form.access_key_id" /></a-form-item>
          <a-form-item label="Access Key Secret"><a-input-password v-model:value="form.access_key_secret" /></a-form-item>
        </a-form>
      </a-modal>
    </div>
  </Page>
</template>
