<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue';
import { requestClient } from '#/api/request';

const data = ref<any[]>([]);
const loading = ref(false);
const keyword = ref('');
const zoneFilter = ref('all');

async function fetch() {
  loading.value = true;
  try { data.value = (await requestClient.get('/asset-ops/assets')) as any[]; } catch { /* */ } finally { loading.value = false; }
}

const filtered = computed(() => {
  let list = data.value;
  if (keyword.value) {
    const k = keyword.value.toLowerCase();
    list = list.filter((a: any) => (a.name||a.ip||'').toLowerCase().includes(k));
  }
  if (zoneFilter.value !== 'all') list = list.filter((a: any) => a.zone === zoneFilter.value);
  return list;
});

const riskColor = (weight: number) => weight >= 80 ? 'red' : weight >= 60 ? 'orange' : 'green';

onMounted(fetch);
</script>

<template>
  <div style="padding:24px;min-height:100vh">
    <a-page-header title="资产中心" sub-title="扫描发现的网络资产 · IP、端口、服务">
      <template #extra>
        <a-input-search v-model:value="keyword" placeholder="搜索IP或名称..." style="width:240px" allow-clear />
        <a-select v-model:value="zoneFilter" style="width:120px;margin-left:8px">
          <a-select-option value="all">全部区域</a-select-option>
          <a-select-option value="Intranet">内网</a-select-option>
          <a-select-option value="DMZ">DMZ</a-select-option>
          <a-select-option value="Internet">互联网</a-select-option>
        </a-select>
        <a-button @click="fetch" :loading="loading" style="margin-left:8px">刷新</a-button>
      </template>
    </a-page-header>

    <a-spin :spinning="loading">
      <a-empty v-if="!filtered.length" description="暂无资产 · 执行扫描任务后自动发现" />

      <!-- Summary -->
      <a-row :gutter="[16,16]" v-if="filtered.length" style="margin-bottom:16px">
        <a-col :span="6" v-for="s in [
          { label:'总资产', val: data.length, color: '#1677ff' },
          { label:'内网', val: data.filter((a:any)=>a.zone==='Intranet').length, color: '#52c41a' },
          { label:'DMZ', val: data.filter((a:any)=>a.zone==='DMZ').length, color: '#fa8c16' },
          { label:'互联网', val: data.filter((a:any)=>a.zone==='Internet').length, color: '#ff4d4f' },
        ]" :key="s.label">
          <a-card size="small">
            <a-statistic :title="s.label" :value="s.val" :value-style="{color:s.color,fontSize:'28px'}" />
          </a-card>
        </a-col>
      </a-row>

      <!-- Table -->
      <a-table v-if="filtered.length" :columns="[
        { title:'名称', dataIndex:'name', key:'name' },
        { title:'IP', dataIndex:'ip', key:'ip' },
        { title:'区域', dataIndex:'zone', key:'zone', width:100 },
        { title:'权重', dataIndex:'weight', key:'weight', width:70 },
        { title:'OS', dataIndex:'os', key:'os', width:140 },
        { title:'类型', dataIndex:'device_type', key:'type', width:100 },
        { title:'负责人', dataIndex:'owner', key:'owner', width:100 },
        { title:'最近扫描', dataIndex:'last_scanned', key:'scan', width:160 },
      ]" :data-source="filtered" row-key="id" size="middle" :pagination="{pageSize:20}">
        <template #bodyCell="{ column, record }">
          <template v-if="column.key==='zone'"><a-tag :color="record.zone==='Intranet'?'green':record.zone==='DMZ'?'orange':'red'">{{ record.zone }}</a-tag></template>
          <template v-if="column.key==='weight'"><a-tag :color="riskColor(record.weight)">{{ record.weight }}</a-tag></template>
          <template v-if="column.key==='scan'">{{ record.last_scanned ? new Date(record.last_scanned).toLocaleString() : '未扫描' }}</template>
        </template>
        <!-- Expand: ports -->
        <template #expandedRowRender="{ record }">
          <div style="margin:0">
            <div v-if="!record.ports?.length" style="color:#8c8c8c;font-size:13px">暂无端口数据 · 执行扫描任务后自动填充</div>
            <a-tag v-for="p in record.ports" :key="p.port" :color="p.is_open?'green':'default'" style="margin:2px">
              {{ p.port }}{{ p.service ? '/'+p.service : '' }}
            </a-tag>
          </div>
        </template>
      </a-table>
    </a-spin>
  </div>
</template>
