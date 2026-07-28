<script lang="ts" setup>
import { ref, onMounted } from 'vue';
import { Page } from '@vben/common-ui';
import { message } from 'ant-design-vue';
import {
  createScanner,
  deleteScanner,
  getScannerList,
  updateScanner,
} from '#/api/scan/scanner';
import type { ScanScannerApi } from '#/api/scan/scanner';
import { executeScan, getScanResults } from '#/api/scan/task';

type Scanner = ScanScannerApi.ScannerConfig;
const data = ref<Scanner[]>([]);
const results = ref<any[]>([]);
const loading = ref(false);
const scanning = ref(false);
const scanVisible = ref(false);
const configVisible = ref(false);
const editingId = ref<string>();
const form = ref<Scanner>({ id:'',name:'',scanner_type:'basic_tcp',enabled:true,config:{} });
const scanForm = ref({ target_ip:'', ports:[22,80,443,3306,3389,8080,8443] });

const columns = [
  { title:'名称', dataIndex:'name', key:'name' },
  { title:'扫描引擎', dataIndex:'scanner_type', key:'scanner_type', width:120 },
  { title:'状态', dataIndex:'enabled', key:'enabled', width:80 },
  { title:'更新时间', dataIndex:'updated_at', key:'updated_at', width:180 },
  { title:'操作', key:'actions', width:140 },
];

async function fetchData() { loading.value=true; try { data.value=(await getScannerList()) as any; } catch { message.error('加载失败'); } finally { loading.value=false; } }
onMounted(fetchData);

function openConfig(edit?:Scanner) {
  editingId.value = edit?.id;
  form.value = edit ? {...edit} : { id:'',name:'',scanner_type:'basic_tcp',enabled:true,config:{} };
  configVisible.value = true;
}
async function handleConfig() {
  if (editingId.value) { await updateScanner(editingId.value,form.value as any); message.success('更新成功'); }
  else { await createScanner(form.value as any); message.success('创建成功'); }
  configVisible.value=false; fetchData();
}

async function handleScan() {
  scanVisible.value=false; scanning.value=true;
  try { await executeScan(scanForm.value); const r:any = await getScanResults(); results.value=r||[]; message.success('扫描完成'); }
  catch { message.error('扫描失败'); }
  finally { scanning.value=false; }
}

const typeLabel:Record<string,string> = { basic_tcp:'TCP 连接', rustscan:'RustScan', nmap:'Nmap', hybrid:'混合引擎' };
</script>

<template>
  <Page auto-content-height>
    <div style="padding:24px;background:var(--ant-color-bg-layout);min-height:100%">
      <a-page-header title="扫描器管理" sub-title="配置扫描引擎 · 执行端口扫描 · 查看结果" style="background:var(--ant-color-bg-container);border-radius:8px;margin-bottom:16px;padding:16px 24px">
        <template #extra>
          <a-space>
            <a-button @click="scanVisible=true" type="primary" ghost><Icon icon="lucide:scan-search" /> 执行扫描</a-button>
            <a-button type="primary" @click="openConfig()"><Icon icon="lucide:plus" /> 新建扫描器</a-button>
          </a-space>
        </template>
      </a-page-header>

      <a-card title="扫描器配置" size="small" style="border-radius:8px;margin-bottom:16px">
        <a-table :columns="columns" :data-source="data" :loading="loading" row-key="id" size="middle" :pagination="false">
          <template #bodyCell="{ column, record }">
            <template v-if="column.key==='scanner_type'"><a-tag>{{ typeLabel[record.scanner_type]||record.scanner_type }}</a-tag></template>
            <template v-if="column.key==='enabled'"><a-badge :status="record.enabled?'success':'default'" :text="record.enabled?'启用':'禁用'" /></template>
            <template v-if="column.key==='actions'">
              <a-space>
                <a-button type="link" size="small" @click="openConfig(record)">编辑</a-button>
                <a-popconfirm title="确认删除?" @confirm="deleteScanner(record.id).then(fetchData)"><a-button type="link" size="small" danger>删除</a-button></a-popconfirm>
              </a-space>
            </template>
          </template>
        </a-table>
      </a-card>

      <a-card title="扫描结果" size="small" style="border-radius:8px" v-if="results.length">
        <a-table :columns="[{title:'目标IP',dataIndex:'ip',width:150},{title:'状态',dataIndex:'is_alive',width:80},{title:'开放端口',key:'ports'},{title:'扫描时间',dataIndex:'scanned_at',width:200}]" :data-source="results" row-key="ip" size="middle" :pagination="false">
          <template #bodyCell="{ column, record }">
            <template v-if="column.key==='is_alive'"><a-tag :color="record.is_alive?'green':'red'">{{ record.is_alive?'存活':'离线' }}</a-tag></template>
            <template v-if="column.key==='ports'">
              <a-tag v-for="p in (record.open_ports||[])" :key="p.port" :color="p.is_open?'blue':'default'" style="margin:2px">
                {{ p.port }}<span v-if="p.service" style="color:var(--ant-color-text-tertiary)">:{{ p.service }}</span>
              </a-tag>
            </template>
          </template>
        </a-table>
      </a-card>
      <a-empty v-else description="暂无扫描结果 · 点击“执行扫描”开始" style="margin-top:32px" />

      <!-- Scan Modal -->
      <a-modal v-model:open="scanVisible" title="执行端口扫描" @ok="handleScan" width="480px">
        <a-form layout="vertical">
          <a-form-item label="目标 IP" required><a-input v-model:value="scanForm.target_ip" placeholder="192.168.1.1 或 10.0.0.0/24" size="large" /></a-form-item>
          <a-form-item label="端口">
            <a-select v-model:value="scanForm.ports" mode="multiple" placeholder="选择端口或输入自定义端口" :options="[22,23,25,53,80,110,143,443,445,993,995,1433,1521,3306,3389,5432,6379,8080,8443,27017].map(p=>({value:p,label:p+' ('+{22:'SSH',80:'HTTP',443:'HTTPS',3306:'MySQL',3389:'RDP',5432:'PostgreSQL',6379:'Redis',8080:'HTTP-Alt',8443:'HTTPS-Alt',27017:'MongoDB'}[p]||''+')'}))" />
          </a-form-item>
        </a-form>
      </a-modal>

      <!-- Config Modal -->
      <a-modal v-model:open="configVisible" :title="editingId?'编辑扫描器':'新建扫描器'" @ok="handleConfig" width="500px">
        <a-form layout="vertical">
          <a-form-item label="名称" required><a-input v-model:value="form.name" placeholder="例如：生产环境扫描器" /></a-form-item>
          <a-form-item label="扫描引擎"><a-select v-model:value="form.scanner_type"><a-select-option value="basic_tcp">TCP 连接扫描</a-select-option><a-select-option value="rustscan">RustScan 高速扫描</a-select-option><a-select-option value="nmap">Nmap 深度扫描</a-select-option><a-select-option value="hybrid">混合引擎</a-select-option></a-select></a-form-item>
          <a-form-item label="启用"><a-switch v-model:checked="form.enabled" /></a-form-item>
        </a-form>
      </a-modal>
    </div>
  </Page>
</template>
