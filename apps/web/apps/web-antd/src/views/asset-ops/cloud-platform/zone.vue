<script lang="ts" setup>
import { computed, ref, onMounted } from 'vue';
import { Page } from '@vben/common-ui';
import { message } from 'ant-design-vue';
import { getCloudZoneList, createCloudZone, updateCloudZone, deleteCloudZone } from '#/api/scan/cloud-platform';
import { getServiceProviderList } from '#/api/scan/service-provider';
import { useLocalTableSearch } from '../composables/useLocalTableSearch';
interface CloudZone { id?: number; provider_id: number; zone_name: string; zone_code: string; zone_type: 'private_cloud' | 'public_cloud'; description?: string; }
const data = ref<CloudZone[]>([]); const loading = ref(false); const modalVisible = ref(false); const editingId = ref<number | undefined>();
const { filteredRows, searchText } = useLocalTableSearch(data, ['zone_name', 'zone_code', 'description']);
const providers = ref<any[]>([]);
const zoneNameSuffix = ref('');
const form = ref<CloudZone>({ provider_id: 0, zone_name: '', zone_code: '', zone_type: 'public_cloud', description: '' });
const selectedProviderName = computed(() => providers.value.find(provider => provider.id === form.value.provider_id)?.provider_name || '服务商');
const columns = [{ title: '服务商', key: 'provider_id' }, { title: '资源区名称', dataIndex: 'zone_name' }, { title: '编码', dataIndex: 'zone_code' }, { title: '类型', key: 'zone_type' }, { title: '描述', dataIndex: 'description' }, { title: '操作', key: 'actions', width: 200 }];
async function fetchData() { loading.value = true; try { const [zoneRows, providerRows] = await Promise.all([getCloudZoneList(), getServiceProviderList()]); data.value = zoneRows as any; providers.value = providerRows as any; } catch { message.error('加载失败'); } finally { loading.value = false; } }
onMounted(fetchData);
function openCreate() { if (!providers.value.length) { message.warning('请先创建服务商'); return; } editingId.value = undefined; zoneNameSuffix.value = ''; form.value = { provider_id: providers.value[0].id, zone_name: '', zone_code: '', zone_type: 'public_cloud', description: '' }; modalVisible.value = true; }
function openEdit(r: CloudZone) { editingId.value = r.id; form.value = { ...r }; const providerName = providers.value.find(provider => provider.id === r.provider_id)?.provider_name || ''; const prefix = providerName ? `${providerName}-` : ''; zoneNameSuffix.value = prefix && r.zone_name.startsWith(prefix) ? r.zone_name.slice(prefix.length) : r.zone_name; modalVisible.value = true; }
async function handleSubmit() { if (!form.value.provider_id) { message.warning('请选择服务商'); return; } const suffix = zoneNameSuffix.value.trim().replace(/^-+/, ''); if (!suffix) { message.warning('请输入资源区名称'); return; } form.value.zone_name = `${selectedProviderName.value}-${suffix}`; if (editingId.value) { await updateCloudZone(editingId.value, form.value as any); message.success('更新成功'); } else { await createCloudZone(form.value as any); message.success('创建成功'); } modalVisible.value = false; fetchData(); }
</script>
<template>
  <Page auto-content-height>
    <div style="padding:16px">
      <a-space :size="24" style="margin-bottom:20px"><a-button type="primary" @click="openCreate">新建云资源区</a-button><a-input-search v-model:value="searchText" allow-clear placeholder="搜索名称、编码或描述" style="width:260px" /><a-button @click="fetchData">刷新</a-button></a-space>
      <a-table :columns="columns" :data-source="filteredRows" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:20}">
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'provider_id'">{{ providers.find(p=>p.id===record.provider_id)?.provider_name || '-' }}</template>
          <template v-if="column.key === 'zone_type'"><a-tag :color="record.zone_type === 'public_cloud' ? 'blue' : 'purple'">{{ record.zone_type === 'public_cloud' ? '公有云' : '私有云' }}</a-tag></template>
          <template v-if="column.key === 'actions'">
            <a-space :size="12"><a-button size="small" @click="openEdit(record)">编辑</a-button><a-popconfirm title="确认删除?" @confirm="deleteCloudZone(record.id!).then(fetchData)"><a-button size="small" danger>删除</a-button></a-popconfirm></a-space>
          </template>
        </template>
      </a-table>
      <a-modal v-model:open="modalVisible" :title="editingId ? '编辑云资源区' : '新建云资源区'" @ok="handleSubmit" width="500px">
        <a-form layout="vertical">
          <a-form-item label="服务商" required><a-select v-model:value="form.provider_id" :options="providers.map(p=>({value:p.id,label:p.provider_name}))" /></a-form-item>
          <a-form-item label="资源区名称" required><a-input v-model:value="zoneNameSuffix" :addon-before="`${selectedProviderName}-`" placeholder="公有云资源区" /></a-form-item>
          <a-form-item label="编码" required><a-input v-model:value="form.zone_code" placeholder="north" /></a-form-item>
          <a-form-item label="云区类型" required><a-radio-group v-model:value="form.zone_type"><a-radio-button value="public_cloud">公有云</a-radio-button><a-radio-button value="private_cloud">私有云</a-radio-button></a-radio-group></a-form-item>
          <a-form-item label="描述"><a-textarea v-model:value="form.description" /></a-form-item>
        </a-form>
      </a-modal>
    </div>
  </Page>
</template>
