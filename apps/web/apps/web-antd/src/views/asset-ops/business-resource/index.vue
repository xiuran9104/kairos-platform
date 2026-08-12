<script lang="ts" setup>
import { ref, onMounted } from 'vue';
import { Page } from '@vben/common-ui';
import { message } from 'ant-design-vue';
import { getBusinessResourceList, createBusinessResource, updateBusinessResource, deleteBusinessResource } from '#/api/scan/business-resource';
import { getMachineRoomList } from '#/api/scan/machine-room';
import { getCloudProviderConfigList } from '#/api/scan/cloud-provider-config';
import { getCabinetList, getCabinetRowList } from '#/api/scan/cabinet';
import { useLocalTableSearch } from '../composables/useLocalTableSearch';
import PhysicalNetworkFields from './components/PhysicalNetworkFields.vue';
const props = defineProps<{ fixedResourceType?: 'physical' }>();
interface Resource { id?: number; resource_type: string; ecs_name: string; ecs_status: string; cloud_region: string; cloud_category: string; customer_name: string; cpu_cores: number; memory_gb: number; ip_address: string; application_status: string; delivery_status: string; machine_room_id?: number; cloud_provider_config_id?: number; cabinet_id?: number; cabinet_name?: string; chassis_name?: string; chassis_slot?: string; rack_start_u?: number; rack_end_u?: number; deployment_type?: string; management_ip?: string; business_ip?: string; network_cidr?: string; gateway?: string; vlan_id?: string; dns_servers?: string; mac_address?: string; switch_name?: string; switch_port?: string; ecs_type: string; ecs_os: string; system_disk: string; system_disk_size_gb: number; has_security_product: boolean; }
const data = ref<Resource[]>([]); const loading = ref(false); const modalVisible = ref(false); const editingId = ref<number | undefined>();
const rooms = ref<any[]>([]); const configs = ref<any[]>([]); const cabinetRows = ref<any[]>([]); const cabinets = ref<any[]>([]);
const { filteredRows, searchText } = useLocalTableSearch(data, ['ecs_name', 'cloud_region', 'customer_name', 'ip_address']);
const emptyForm = (): Resource => ({ resource_type: 'cloud', ecs_name: '', ecs_status: '运行中', cloud_region: '', cloud_category: '', customer_name: '', cpu_cores: 4, memory_gb: 8, ip_address: '', application_status: '待审核', delivery_status: '待交付', deployment_type: 'cloud_managed', ecs_type: '', ecs_os: '', system_disk: '', system_disk_size_gb: 0, has_security_product: false });
const form = ref<Resource>(emptyForm());
const columns = [{ title: '名称', dataIndex: 'ecs_name' }, { title: '类型', key: 'resource_type' }, { title: '部署方式', key: 'deployment_type' }, { title: '上架位置', key: 'rack_location' }, { title: '状态', key: 'ecs_status' }, { title: '客户', dataIndex: 'customer_name' }, { title: '规格', key: 'spec' }, { title: 'IP', key: 'ip' }, { title: '操作', key: 'actions', width: 200 }];
const typeLabel: Record<string, string> = { cloud: '云资源', physical: '物理机' };
const deploymentLabel: Record<string, string> = { cloud_managed: '云平台托管', datacenter_managed: '机房托管', standalone: '独立部署' };
const statusLabel: Record<string, string> = { available: '空闲', allocated: '已分配', maintenance: '维修中', offline: '已下线', retired: '已退役' };
async function fetchData() { loading.value = true; try { const [resources, roomRows, configRows, rowData, cabinetData] = await Promise.all([getBusinessResourceList(), getMachineRoomList(), getCloudProviderConfigList(), getCabinetRowList(), getCabinetList()]); data.value = props.fixedResourceType ? (resources as any[]).filter(item => item.resource_type === props.fixedResourceType) as any : resources as any; rooms.value = roomRows as any; configs.value = configRows as any; cabinetRows.value = rowData as any; cabinets.value = cabinetData as any; } catch { message.error('加载失败'); } finally { loading.value = false; } }
onMounted(fetchData);
function openCreate() { editingId.value = undefined; form.value = emptyForm(); if (props.fixedResourceType) { form.value.resource_type = props.fixedResourceType; form.value.deployment_type = 'standalone'; form.value.ecs_status = 'available'; } modalVisible.value = true; }
function openEdit(r: Resource) { editingId.value = r.id; form.value = { ...r }; modalVisible.value = true; }
async function handleSubmit() { if (form.value.resource_type === 'physical' && form.value.deployment_type === 'cloud_managed' && !form.value.cloud_provider_config_id) { message.warning('请选择已认证的云平台对接配置'); return; } if (form.value.resource_type === 'physical' && form.value.deployment_type !== 'standalone' && !form.value.machine_room_id) { message.warning('请选择物理机房'); return; } if (form.value.resource_type === 'physical' && form.value.deployment_type === 'datacenter_managed' && (!form.value.cabinet_id || !form.value.rack_start_u || !form.value.rack_end_u || form.value.rack_end_u < form.value.rack_start_u)) { message.warning('请选择机柜并填写有效的起止 U 位'); return; } if (form.value.resource_type === 'physical' && form.value.deployment_type !== 'cloud_managed' && (!(form.value.management_ip || form.value.business_ip) || !form.value.network_cidr || !form.value.gateway)) { message.warning('请填写管理/业务 IP、网段和网关'); return; } if (editingId.value) { await updateBusinessResource(editingId.value, form.value as any); message.success('更新成功'); } else { await createBusinessResource(form.value as any); message.success('创建成功'); } modalVisible.value = false; fetchData(); }
</script>
<template>
  <Page auto-content-height>
    <div style="padding:16px">
      <a-page-header v-if="fixedResourceType" title="物理服务器" sub-title="维护物理服务器资产台账、部署归属和网络配置" style="padding:0;margin-bottom:16px" />
      <a-space :size="24" style="margin-bottom:20px"><a-button type="primary" @click="openCreate">{{ fixedResourceType ? '新建物理服务器' : '新建业务资源' }}</a-button><a-input-search v-model:value="searchText" allow-clear placeholder="搜索名称、区域、客户或 IP" style="width:260px" /><a-button @click="fetchData">刷新</a-button></a-space>
      <a-table :columns="columns" :data-source="filteredRows" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:20}">
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'resource_type'">{{ typeLabel[record.resource_type] || record.resource_type }}</template>
          <template v-if="column.key === 'deployment_type'">{{ record.resource_type === 'physical' ? deploymentLabel[record.deployment_type] || '-' : '-' }}</template>
          <template v-if="column.key === 'rack_location'">{{ rooms.find(room=>room.id===record.machine_room_id)?.room_name || (record.deployment_type==='standalone'?'独立部署':'-') }}<template v-if="cabinets.find(item=>item.id===record.cabinet_id)"> / {{ cabinetRows.find(row=>row.id===cabinets.find(item=>item.id===record.cabinet_id)?.row_id)?.row_name }} / {{ cabinets.find(item=>item.id===record.cabinet_id)?.cabinet_name }}（{{ cabinets.find(item=>item.id===record.cabinet_id)?.total_u }}U）</template><span v-else-if="record.cabinet_name"> / {{ record.cabinet_name }}</span><span v-if="record.rack_start_u"> / U{{ record.rack_start_u }}-U{{ record.rack_end_u }}</span><span v-if="record.chassis_name"> / {{ record.chassis_name }}<template v-if="record.chassis_slot">-{{ record.chassis_slot }}</template></span></template>
          <template v-if="column.key === 'ecs_status'"><a-tag :color="['运行中','available','allocated'].includes(record.ecs_status) ? 'green' : record.ecs_status === 'maintenance' ? 'orange' : 'default'">{{ statusLabel[record.ecs_status] || record.ecs_status }}</a-tag></template>
          <template v-if="column.key === 'spec'">{{ record.cpu_cores }}核 / {{ record.memory_gb }}GB</template>
          <template v-if="column.key === 'ip'">{{ record.resource_type === 'physical' ? record.management_ip || record.business_ip || '-' : record.ip_address || '-' }}</template>
          <template v-if="column.key === 'application_status'"><a-tag :color="record.application_status === '已批准' ? 'green' : record.application_status === '待审核' ? 'orange' : 'default'">{{ record.application_status }}</a-tag></template>
          <template v-if="column.key === 'actions'">
            <a-space :size="12"><a-button size="small" @click="openEdit(record)">编辑</a-button><a-popconfirm title="确认删除?" @confirm="deleteBusinessResource(record.id!).then(fetchData)"><a-button size="small" danger>删除</a-button></a-popconfirm></a-space>
          </template>
        </template>
      </a-table>
      <a-modal v-model:open="modalVisible" :title="editingId ? '编辑业务资源' : '新建业务资源'" @ok="handleSubmit" width="760px">
        <a-form layout="vertical">
          <a-row :gutter="16">
            <a-col :span="12"><a-form-item label="名称" required><a-input v-model:value="form.ecs_name" /></a-form-item></a-col>
            <a-col v-if="!fixedResourceType" :span="12"><a-form-item label="资源类型"><a-select v-model:value="form.resource_type"><a-select-option value="cloud">云资源</a-select-option><a-select-option value="physical">物理机</a-select-option></a-select></a-form-item></a-col>
            <PhysicalNetworkFields v-if="form.resource_type==='physical'" v-model:form="form" :rooms="rooms" :configs="configs" :cabinet-rows="cabinetRows" :cabinets="cabinets" />
            <a-col :span="8"><a-form-item label="状态" required><a-select v-if="form.resource_type==='physical'" v-model:value="form.ecs_status"><a-select-option value="available">空闲</a-select-option><a-select-option value="allocated">已分配</a-select-option><a-select-option value="maintenance">维修中</a-select-option><a-select-option value="offline">已下线</a-select-option><a-select-option value="retired">已退役</a-select-option></a-select><a-select v-else v-model:value="form.ecs_status"><a-select-option value="运行中">运行中</a-select-option><a-select-option value="已停止">已停止</a-select-option><a-select-option value="异常">异常</a-select-option></a-select></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="CPU核数"><a-input-number v-model:value="form.cpu_cores" :min="1" style="width:100%" /></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="内存(GB)"><a-input-number v-model:value="form.memory_gb" :min="1" style="width:100%" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="区域" required><a-input v-model:value="form.cloud_region" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="云类别/机房" required><a-input v-model:value="form.cloud_category" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="客户" required><a-input v-model:value="form.customer_name" /></a-form-item></a-col>
            <a-col v-if="form.resource_type==='cloud'" :span="12"><a-form-item label="IP地址" required><a-input v-model:value="form.ip_address" /></a-form-item></a-col>
          </a-row>
        </a-form>
      </a-modal>
    </div>
  </Page>
</template>
