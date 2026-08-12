<script lang="ts" setup>
import { computed, ref, onMounted } from 'vue';
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
import { useLocalTableSearch } from '../composables/useLocalTableSearch';

type Scanner = ScanScannerApi.ScannerConfig;
const data = ref<Scanner[]>([]);
const { filteredRows, searchText } = useLocalTableSearch(data, ['name', 'scanner_type']);
const results = ref<any[]>([]);
const loading = ref(false);
const scanning = ref(false);
const scanVisible = ref(false);
const configVisible = ref(false);
const editingId = ref<string>();
const form = ref<Scanner>({ id:'',name:'',scanner_type:'rustscan',enabled:true,config:{} });
const registrationVisible = ref(false);
const registeredNode = ref<Scanner>();
const registrationCommand = computed(()=>registeredNode.value?.registration_token ? `kairos-scan-agent register --server ${window.location.origin}/api --token ${registeredNode.value.registration_token} --name ${registeredNode.value.name}` : '');
const scanForm = ref({ target_ip:'', ports:[22,80,443,3306,3389,8080,8443] });

const columns = [
  { title:'名称', dataIndex:'name', key:'name' },
  { title:'引擎', dataIndex:'scanner_type', key:'scanner_type', width:100 },
  { title:'注册状态', key:'agent_status', width:100 },
  { title:'Agent / 地址', key:'agent', width:180 },
  { title:'最后心跳', dataIndex:'last_heartbeat', key:'last_heartbeat', width:180 },
  { title:'启用', dataIndex:'enabled', key:'enabled', width:70 },
  { title:'操作', key:'actions', width:140 },
];

async function fetchData() { loading.value=true; try { data.value=(await getScannerList()) as any; } catch { message.error('加载失败'); } finally { loading.value=false; } }
onMounted(fetchData);

function openConfig(edit?:Scanner) {
  editingId.value = edit?.id;
  form.value = edit ? {...edit} : { id:'',name:'',scanner_type:'rustscan',enabled:true,config:{} };
  configVisible.value = true;
}
async function handleConfig() {
  if (editingId.value) { await updateScanner(editingId.value,form.value as any); message.success('更新成功'); }
  else { registeredNode.value=await createScanner(form.value as any) as any; registrationVisible.value=true; message.success('注册凭证已生成'); }
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
    <div style="padding:16px">
      <a-page-header title="扫描节点" sub-title="维护执行扫描任务的节点与扫描引擎" style="margin-bottom:16px;padding:0" />
      <a-space :size="20" style="margin-bottom:20px" wrap><a-button type="primary" @click="openConfig()"><Icon icon="lucide:plus" /> 创建扫描节点</a-button><a-button @click="scanVisible=true" type="primary" ghost><Icon icon="lucide:scan-search" /> 快速扫描</a-button><a-input-search v-model:value="searchText" allow-clear placeholder="搜索节点名称或引擎" style="width:260px" /><a-button :loading="loading" @click="fetchData"><Icon icon="lucide:refresh-cw" /> 刷新</a-button></a-space>

      <a-card title="扫描节点列表" size="small" style="border-radius:8px;margin-bottom:16px">
        <a-table :columns="columns" :data-source="filteredRows" :loading="loading" row-key="id" size="middle" :pagination="false">
          <template #bodyCell="{ column, record }">
            <template v-if="column.key==='scanner_type'"><a-tag>{{ typeLabel[record.scanner_type]||record.scanner_type }}</a-tag></template>
            <template v-if="column.key==='agent_status'"><a-badge :status="record.agent_status==='online'?'success':record.agent_status==='pending'?'processing':'default'" :text="record.agent_status==='online'?'在线':record.agent_status==='pending'?'待注册':'离线'" /></template>
            <template v-if="column.key==='agent'"><div>{{record.agent_hostname||'-'}}</div><div style="font-size:12px;color:var(--ant-color-text-secondary)">{{record.agent_version||'未注册'}}<span v-if="record.agent_ip"> / {{record.agent_ip}}</span></div></template>
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
      <a-modal v-model:open="configVisible" :title="editingId?'编辑扫描节点':'创建扫描节点'" @ok="handleConfig" width="500px">
        <a-form layout="vertical">
          <a-form-item label="名称" required><a-input v-model:value="form.name" placeholder="例如：生产环境扫描器" /></a-form-item>
          <a-form-item label="扫描引擎"><a-input value="RustScan（IP 存活与端口发现）" disabled /></a-form-item>
          <a-form-item label="启用"><a-switch v-model:checked="form.enabled" /></a-form-item>
        </a-form>
      </a-modal>
      <a-modal v-model:open="registrationVisible" title="部署并注册扫描 Agent" :footer="null" width="680px">
        <a-alert type="warning" show-icon message="注册令牌只在本次创建后展示，有效期 24 小时，请立即复制到目标网络执行。" style="margin-bottom:16px" />
        <a-typography-title :level="5">注册命令</a-typography-title>
        <a-typography-paragraph copyable><pre class="registration-command">{{registrationCommand}}</pre></a-typography-paragraph>
        <a-steps direction="vertical" size="small" :current="0" :items="[{title:'安装 Agent 与 RustScan',description:'在能够访问目标网段的 Linux 主机上安装'},{title:'执行注册命令',description:'Agent 主动连接平台并完成注册'},{title:'等待节点上线',description:'节点每 30 秒发送一次心跳'}]" />
      </a-modal>
    </div>
  </Page>
</template>

<style scoped>.registration-command{padding:12px;white-space:pre-wrap;word-break:break-all;background:var(--ant-color-fill-tertiary);border-radius:6px}</style>
