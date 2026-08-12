<script lang="ts" setup>
import { onMounted, ref } from 'vue';
import { useRouter } from 'vue-router';
import { Page } from '@vben/common-ui';
import { message } from 'ant-design-vue';
import {
  createMachineRoom,
  deleteMachineRoom,
  getMachineRoomList,
  updateMachineRoom,
} from '#/api/scan/machine-room';
import type { ScanMachineRoomApi } from '#/api/scan/machine-room';
import { getCloudPlatformList } from '#/api/scan/cloud-platform';
import { getCabinetList, getCabinetRowList } from '#/api/scan/cabinet';
import { getBusinessResourceList } from '#/api/scan/business-resource';
import { useCrudList } from '../composables/useCrudList';

type MachineRoom = ScanMachineRoomApi.MachineRoom;
const platforms = ref<any[]>([]);
const cabinetRows = ref<any[]>([]);
const cabinets = ref<any[]>([]);
const resources = ref<any[]>([]);
const router = useRouter();

const {
  loading, modalVisible, editingId, searchText, form,
  filtered, fetchData, openCreate: openCreateForm, openEdit, handleSubmit, handleDelete,
} = useCrudList<MachineRoom>({
  api: {
    list: getMachineRoomList,
    create: (d) => createMachineRoom(d as any),
    update: (id, d) => updateMachineRoom(id, d as any),
    del: (id) => deleteMachineRoom(id),
  },
  defaultForm: () => ({ room_name:'',room_code:'',facility_type:'',address:'',platform_id:0,room_type:'核心机房',contact_person:'',contact_phone:'',status:'active' }),
  searchKeys: ['room_name', 'room_code', 'address'],
});
onMounted(async () => {
  const [platformData, rowData, cabinetData, resourceData] = await Promise.all([getCloudPlatformList(), getCabinetRowList(), getCabinetList(), getBusinessResourceList()]);
  platforms.value = platformData as any; cabinetRows.value = rowData as any; cabinets.value = cabinetData as any; resources.value = resourceData as any;
});
const roomCabinets = (roomId: number) => { const ids = cabinetRows.value.filter(row=>row.machine_room_id===roomId).map(row=>row.id); return cabinets.value.filter(item=>ids.includes(item.row_id)); };
const roomUsedU = (roomId: number) => { const ids = roomCabinets(roomId).map(item=>item.id); return resources.value.filter(item=>ids.includes(item.cabinet_id) && item.rack_start_u && item.rack_end_u).reduce((sum,item)=>sum+item.rack_end_u-item.rack_start_u+1,0); };
function openCreate() {
  if (!platforms.value.length) {
    message.warning('请先创建云平台');
    return;
  }
  openCreateForm();
  if (form.value) form.value.platform_id = platforms.value[0].id;
}

const columns = [
  { title:'机房名称', dataIndex:'room_name', key:'room_name', width:160 },
  { title:'编码', dataIndex:'room_code', key:'room_code', width:100 },
  { title:'类型', dataIndex:'room_type', key:'room_type', width:90 },
  { title:'设施', dataIndex:'facility_type', key:'facility_type', width:80 },
  { title:'云平台', key:'platform_id', width:120 },
  { title:'地址', dataIndex:'address', key:'address', ellipsis:true },
  { title:'联系人', dataIndex:'contact_person', key:'contact_person', width:90 },
  { title:'机柜列', key:'cabinet_rows', width:80 },
  { title:'机柜', key:'cabinet_count', width:70 },
  { title:'U 位容量', key:'u_capacity', width:180 },
  { title:'状态', dataIndex:'status', key:'status', width:65 },
  { title:'操作', key:'actions', width:130, fixed:'right' },
];

const statusMap:Record<string,{color:string;label:string}> = { active:{color:'green',label:'运营中'}, inactive:{color:'red',label:'停用'}, maintenance:{color:'orange',label:'维护中'} };
</script>

<template>
  <Page auto-content-height>
    <div style="padding:16px">
      <a-page-header title="物理机房管理" sub-title="管理云平台下的物理机房" style="margin-bottom:16px;padding:0" />
      <a-space :size="24" style="margin-bottom:20px">
        <a-button type="primary" @click="openCreate">新建物理机房</a-button>
        <a-button @click="fetchData">刷新</a-button>
        <a-input-search v-model:value="searchText" placeholder="搜索机房" style="width:240px" allow-clear />
      </a-space>
        <a-table :columns="columns" :data-source="filtered" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:20}">
          <template #bodyCell="{ column, record }">
            <template v-if="column.key==='room_name'"><Icon icon="lucide:warehouse" style="color:var(--ant-color-primary);margin-right:6px" />{{ record.room_name }}</template>
            <template v-if="column.key==='status'"><a-tag :color="statusMap[record.status]?.color||'default'">{{ statusMap[record.status]?.label||record.status }}</a-tag></template>
            <template v-if="column.key==='cabinet_rows'">{{ cabinetRows.filter(row=>row.machine_room_id===record.id).length }}</template>
            <template v-if="column.key==='cabinet_count'">{{ roomCabinets(record.id).length }}</template>
            <template v-if="column.key==='u_capacity'"><a-progress :percent="roomCabinets(record.id).reduce((sum,item)=>sum+item.total_u,0) ? Math.round(roomUsedU(record.id)*100/roomCabinets(record.id).reduce((sum,item)=>sum+item.total_u,0)) : 0" size="small" :format="()=>`${roomUsedU(record.id)} / ${roomCabinets(record.id).reduce((sum,item)=>sum+item.total_u,0)}U`" /></template>
            <template v-if="column.key==='platform_id'">{{ platforms.find(p=>p.id===record.platform_id)?.platform_name || '-' }}</template>
            <template v-if="column.key==='actions'">
              <a-space>
                <a-button type="link" size="small" @click="router.push(`/asset-ops/infra/cabinet?room_id=${record.id}`)">机柜布局</a-button>
                <a-button type="link" size="small" @click="openEdit(record)">编辑</a-button>
                <a-popconfirm title="确认删除该机房?" ok-text="删除" ok-type="danger" cancel-text="取消" @confirm="handleDelete(record.id!)"><a-button type="link" size="small" danger>删除</a-button></a-popconfirm>
              </a-space>
            </template>
          </template>
        </a-table>
      <a-modal v-model:open="modalVisible" :title="editingId?'编辑机房':'新建机房'" @ok="handleSubmit" width="700px">
        <a-form v-if="form" layout="vertical">
          <a-row :gutter="16">
            <a-col :span="12"><a-form-item label="机房名称" required><a-input v-model:value="form.room_name" placeholder="例如：北京核心机房" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="机房编码" required><a-input v-model:value="form.room_code" placeholder="例如：BJ-CORE" /></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="设施类型"><a-select v-model:value="form.facility_type"><a-select-option value="IDC">IDC 数据中心</a-select-option><a-select-option value="EDC">EDC 边缘机房</a-select-option><a-select-option value="Cloud">云机房</a-select-option></a-select></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="机房类型"><a-select v-model:value="form.room_type"><a-select-option value="核心机房">核心机房</a-select-option><a-select-option value="汇聚机房">汇聚机房</a-select-option><a-select-option value="接入机房">接入机房</a-select-option></a-select></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="状态"><a-select v-model:value="form.status"><a-select-option value="active">运营中</a-select-option><a-select-option value="inactive">停用</a-select-option><a-select-option value="maintenance">维护中</a-select-option></a-select></a-form-item></a-col>
            <a-col :span="24"><a-form-item label="地址" required><a-input v-model:value="form.address" /></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="联系人" required><a-input v-model:value="form.contact_person" /></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="联系电话" required><a-input v-model:value="form.contact_phone" /></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="所属云平台" required><a-select v-model:value="form.platform_id" :options="platforms.map(p=>({value:p.id,label:p.platform_name}))" /></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="楼层"><a-input v-model:value="form.floor" /></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="机柜容量"><a-input value="保存机房后在机柜布局中维护" disabled /></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="面积"><a-input v-model:value="form.area_size" placeholder="m²" /></a-form-item></a-col>
            <a-col :span="24"><a-form-item label="备注"><a-textarea v-model:value="form.remarks" :rows="2" /></a-form-item></a-col>
          </a-row>
        </a-form>
      </a-modal>
    </div>
  </Page>
</template>
