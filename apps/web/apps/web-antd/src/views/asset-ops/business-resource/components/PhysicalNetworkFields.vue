<script lang="ts" setup>
import { computed, ref, watch } from 'vue';
interface RoomOption { id: number; room_name: string; platform_id: number }
interface ConfigOption { id: number; platform_id: number; account_name: string; region_name: string; status: string }
interface RowOption { id?: number; machine_room_id: number; row_name: string; status: string }
interface CabinetOption { id?: number; row_id: number; cabinet_name: string; total_u: number; status: string }
interface PhysicalForm {
  deployment_type?: string; machine_room_id?: number; management_ip?: string;
  business_ip?: string; network_cidr?: string; gateway?: string; vlan_id?: string;
  dns_servers?: string; mac_address?: string; switch_name?: string; switch_port?: string;
  cloud_provider_config_id?: number;
  cabinet_id?: number;
  cabinet_name?: string; chassis_name?: string; chassis_slot?: string;
  rack_start_u?: number; rack_end_u?: number;
}

const form = defineModel<PhysicalForm>('form', { required: true });
const props = defineProps<{ rooms: RoomOption[]; configs: ConfigOption[]; cabinetRows: RowOption[]; cabinets: CabinetOption[] }>();
const selectedRowId = ref<number>();
const roomRows = computed(() => props.cabinetRows.filter(item => item.machine_room_id === form.value.machine_room_id && item.status === 'active'));
const rowCabinets = computed(() => props.cabinets.filter(item => item.row_id === selectedRowId.value && item.status === 'active'));
const selectedCabinet = computed(() => props.cabinets.find(item => item.id === form.value.cabinet_id));
watch(() => form.value.cabinet_id, (id) => { const cabinet = props.cabinets.find(item => item.id === id); if (cabinet) selectedRowId.value = cabinet.row_id; }, { immediate: true });

function changeRoom(value: number) { form.value.machine_room_id = value; selectedRowId.value = undefined; form.value.cabinet_id = undefined; }
function changeRow(value: number) { selectedRowId.value = value; form.value.cabinet_id = undefined; }

function changeConfig(value: number) {
  form.value.cloud_provider_config_id = value;
  const platformId = props.configs.find(item => item.id === value)?.platform_id;
  if (!props.rooms.some(room => room.id === form.value.machine_room_id && room.platform_id === platformId)) form.value.machine_room_id = undefined;
}

function changeDeployment(value: string) {
  form.value.deployment_type = value;
  if (value !== 'cloud_managed') form.value.cloud_provider_config_id = undefined;
  if (value === 'standalone') {
    form.value.machine_room_id = undefined;
    form.value.cabinet_name = ''; form.value.chassis_name = ''; form.value.chassis_slot = '';
    form.value.rack_start_u = undefined; form.value.rack_end_u = undefined;
  }
  if (value !== 'datacenter_managed') form.value.cabinet_id = undefined;
  if (value === 'cloud_managed') {
    for (const key of ['management_ip', 'business_ip', 'network_cidr', 'gateway', 'vlan_id', 'dns_servers', 'mac_address', 'switch_name', 'switch_port'] as const) {
      form.value[key] = '';
    }
  }
}
</script>

<template>
  <a-col :span="12">
    <a-form-item label="部署方式" required>
      <a-select :value="form.deployment_type" @update:value="changeDeployment">
        <a-select-option value="cloud_managed">云平台托管</a-select-option>
        <a-select-option value="datacenter_managed">机房托管</a-select-option>
        <a-select-option value="standalone">独立部署</a-select-option>
      </a-select>
    </a-form-item>
  </a-col>
  <a-col v-if="form.deployment_type === 'cloud_managed'" :span="12">
    <a-form-item label="云平台对接" required>
      <a-select :value="form.cloud_provider_config_id" placeholder="请选择已认证的对接配置" :options="configs.filter(item=>item.status==='active').map(item=>({value:item.id,label:`${item.account_name} / ${item.region_name}`}))" @update:value="changeConfig" />
    </a-form-item>
  </a-col>
  <a-col v-if="form.deployment_type !== 'standalone'" :span="12">
    <a-form-item label="部署机房" required>
      <a-select :value="form.machine_room_id" placeholder="请选择物理机房" :options="rooms.filter(room => form.deployment_type !== 'cloud_managed' || room.platform_id === configs.find(item => item.id === form.cloud_provider_config_id)?.platform_id).map(room => ({ value: room.id, label: room.room_name }))" @update:value="changeRoom" />
    </a-form-item>
  </a-col>
  <a-col :span="24">
    <a-alert v-if="form.deployment_type === 'cloud_managed'" type="info" show-icon message="平台归属和机房必须来自已认证的对接配置；网络信息由平台侧维护，不在此重复录入。" style="margin-bottom:16px" />
    <a-alert v-else type="warning" show-icon message="该设备的网络由资产侧维护，请至少填写管理 IP 或业务 IP，并填写网段和网关。" style="margin-bottom:16px" />
  </a-col>
  <template v-if="form.deployment_type === 'datacenter_managed'">
    <a-col :span="12"><a-form-item label="机柜列" required><a-select :value="selectedRowId" placeholder="例如：A列" :options="roomRows.map(item=>({value:item.id,label:item.row_name}))" @update:value="changeRow" /></a-form-item></a-col>
    <a-col :span="12"><a-form-item label="机柜" required><a-select v-model:value="form.cabinet_id" placeholder="例如：01（42U）" :options="rowCabinets.map(item=>({value:item.id,label:`${item.cabinet_name}（${item.total_u}U）`}))" /></a-form-item></a-col>
    <a-col :span="12"><a-form-item label="机箱/刀片框"><a-input v-model:value="form.chassis_name" placeholder="机架服务器可不填" /></a-form-item></a-col>
    <a-col :span="8"><a-form-item label="起始 U 位" required><a-input-number v-model:value="form.rack_start_u" :min="1" :max="selectedCabinet?.total_u||60" style="width:100%" /></a-form-item></a-col>
    <a-col :span="8"><a-form-item label="结束 U 位" required><a-input-number v-model:value="form.rack_end_u" :min="1" :max="selectedCabinet?.total_u||60" style="width:100%" /></a-form-item></a-col>
    <a-col :span="8"><a-form-item label="机箱槽位"><a-input v-model:value="form.chassis_slot" placeholder="如 Slot 03" /></a-form-item></a-col>
  </template>
  <template v-else-if="form.deployment_type === 'cloud_managed'">
    <a-col :span="12"><a-form-item label="平台同步机柜"><a-input v-model:value="form.cabinet_name" disabled placeholder="由云平台同步" /></a-form-item></a-col>
    <a-col :span="12"><a-form-item label="平台同步机箱"><a-input v-model:value="form.chassis_name" disabled placeholder="由云平台同步" /></a-form-item></a-col>
  </template>
  <template v-if="form.deployment_type !== 'cloud_managed'">
    <a-col :span="12"><a-form-item label="管理 IP"><a-input v-model:value="form.management_ip" placeholder="如 10.10.1.20" /></a-form-item></a-col>
    <a-col :span="12"><a-form-item label="业务 IP"><a-input v-model:value="form.business_ip" placeholder="如 172.16.1.20" /></a-form-item></a-col>
    <a-col :span="12"><a-form-item label="网段" required><a-input v-model:value="form.network_cidr" placeholder="如 10.10.1.0/24" /></a-form-item></a-col>
    <a-col :span="12"><a-form-item label="网关" required><a-input v-model:value="form.gateway" placeholder="如 10.10.1.1" /></a-form-item></a-col>
    <a-col :span="12"><a-form-item label="VLAN"><a-input v-model:value="form.vlan_id" /></a-form-item></a-col>
    <a-col :span="12"><a-form-item label="DNS"><a-input v-model:value="form.dns_servers" placeholder="多个地址用逗号分隔" /></a-form-item></a-col>
    <a-col :span="12"><a-form-item label="MAC 地址"><a-input v-model:value="form.mac_address" /></a-form-item></a-col>
    <a-col :span="12"><a-form-item label="交换机 / 端口"><a-input-group compact><a-input v-model:value="form.switch_name" placeholder="交换机" style="width:50%" /><a-input v-model:value="form.switch_port" placeholder="端口" style="width:50%" /></a-input-group></a-form-item></a-col>
  </template>
</template>
