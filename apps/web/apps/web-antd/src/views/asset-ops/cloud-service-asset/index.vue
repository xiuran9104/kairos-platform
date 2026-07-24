<script lang="ts" setup>
import { ref, onMounted } from 'vue';
import { Page } from '@vben/common-ui';
import { message } from 'ant-design-vue';
import { getCloudServiceAssetList } from '#/api/scan/cloud-service-asset';
interface Asset { id: string; name: string; asset_type: string; status: string; cloud_platform: string; cloud_zone: string; instance_type: string; cpu_cores: number; memory_gb: number; ip_address: string; os_name: string; customer_name: string; }
const data = ref<Asset[]>([]); const loading = ref(false);
const columns = [{ title: '名称', dataIndex: 'name' }, { title: '类型', dataIndex: 'asset_type' }, { title: '状态', dataIndex: 'status' }, { title: '云平台', dataIndex: 'cloud_platform' }, { title: '云区', dataIndex: 'cloud_zone' }, { title: '规格', key: 'spec' }, { title: 'IP', dataIndex: 'ip_address' }, { title: '系统', dataIndex: 'os_name' }, { title: '客户', dataIndex: 'customer_name' }];
const typeLabel: Record<string, string> = { cloud: '云虚拟机', physical: '物理机', virtual: '云虚拟机' };
async function fetchData() { loading.value = true; try { data.value = (await getCloudServiceAssetList()) as any; } catch { message.error('加载失败'); } finally { loading.value = false; } }
onMounted(fetchData);
</script>
<template>
  <Page auto-content-height>
    <div style="padding:16px">
      <a-space style="margin-bottom:16px"><a-button @click="fetchData">刷新</a-button></a-space>
      <a-table :columns="columns" :data-source="data" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:20}">
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'asset_type'">{{ typeLabel[record.asset_type] || record.asset_type }}</template>
          <template v-if="column.key === 'spec'">{{ record.cpu_cores }}核 / {{ record.memory_gb }}GB</template>
          <template v-if="column.key === 'status'"><a-tag :color="record.status === '运行中' ? 'green' : 'default'">{{ record.status }}</a-tag></template>
        </template>
      </a-table>
    </div>
  </Page>
</template>
