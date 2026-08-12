<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue';
import { useRoute } from 'vue-router';
import { Page } from '@vben/common-ui';
import { message } from 'ant-design-vue';
import {
  configureMachineRoomCabinets,
  getCabinetList,
  getCabinetRowList,
  type Cabinet,
  type CabinetRow,
} from '#/api/scan/cabinet';
import { getMachineRoomList } from '#/api/scan/machine-room';

const route = useRoute();
const loading = ref(false);
const configVisible = ref(false);
const rooms = ref<any[]>([]);
const rows = ref<CabinetRow[]>([]);
const cabinets = ref<Cabinet[]>([]);
const roomId = ref<number>();
const configForm = ref({ row_count: 1, cabinets_per_row: 10, cabinet_total_u: 42 });

const roomRows = computed(() => rows.value.filter((item) => item.machine_room_id === roomId.value));
const columns = [
  { title: '机柜列', dataIndex: 'row_name' },
  { title: '机柜数量', key: 'cabinet_count' },
  { title: '机柜编号范围', key: 'range' },
  { title: '单柜高度', key: 'height' },
  { title: '该列总容量', key: 'capacity' },
];

function rowCabinets(rowId?: number) {
  return cabinets.value.filter((item) => item.row_id === rowId);
}

async function fetchData() {
  loading.value = true;
  try {
    const [roomData, rowData, cabinetData] = await Promise.all([
      getMachineRoomList(), getCabinetRowList(), getCabinetList(),
    ]);
    rooms.value = roomData as any[];
    rows.value = rowData;
    cabinets.value = cabinetData;
    if (!roomId.value && rooms.value.length) roomId.value = rooms.value[0].id;
  } catch {
    message.error('机柜容量加载失败');
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  const value = Number(route.query.room_id);
  if (value) roomId.value = value;
  fetchData();
});

function openConfig() {
  if (!roomId.value) return message.warning('请先选择机房');
  const firstRow = roomRows.value[0];
  const firstCabinet = firstRow ? rowCabinets(firstRow.id)[0] : undefined;
  configForm.value = {
    row_count: roomRows.value.length || 1,
    cabinets_per_row: firstRow ? rowCabinets(firstRow.id).length : 10,
    cabinet_total_u: firstCabinet?.total_u || 42,
  };
  configVisible.value = true;
}

async function submitConfig() {
  if (!roomId.value) return;
  await configureMachineRoomCabinets({ machine_room_id: roomId.value, ...configForm.value });
  configVisible.value = false;
  message.success('机房机柜容量配置成功');
  fetchData();
}
</script>

<template>
  <Page auto-content-height>
    <div style="padding: 16px">
      <a-page-header title="机房机柜容量" sub-title="设置机柜列数量、每列机柜数量和统一机柜高度" style="padding: 0; margin-bottom: 16px" />
      <a-space :size="20" style="margin-bottom: 20px" wrap>
        <a-select v-model:value="roomId" style="width: 240px" placeholder="选择机房" :options="rooms.map(item => ({ value: item.id, label: item.room_name }))" />
        <a-button type="primary" @click="openConfig">配置机柜容量</a-button>
        <a-button @click="fetchData">刷新</a-button>
      </a-space>

      <a-table :columns="columns" :data-source="roomRows" :loading="loading" row-key="id" size="middle" :pagination="false">
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'cabinet_count'">{{ rowCabinets(record.id).length }} 个</template>
          <template v-else-if="column.key === 'range'">{{ rowCabinets(record.id).length ? `${rowCabinets(record.id)[0]?.cabinet_code} - ${rowCabinets(record.id).at(-1)?.cabinet_code}` : '-' }}</template>
          <template v-else-if="column.key === 'height'">{{ rowCabinets(record.id)[0]?.total_u || 0 }}U</template>
          <template v-else-if="column.key === 'capacity'">{{ rowCabinets(record.id).reduce((sum, item) => sum + item.total_u, 0) }}U</template>
        </template>
      </a-table>

      <a-modal v-model:open="configVisible" title="配置机房机柜容量" width="560px" @ok="submitConfig">
        <a-alert type="info" show-icon message="系统将自动生成 A列、B列……以及每列的 01、02……号机柜。" style="margin-bottom: 16px" />
        <a-form layout="vertical">
          <a-form-item label="机房"><a-input :value="rooms.find(item => item.id === roomId)?.room_name" disabled /></a-form-item>
          <a-row :gutter="16">
            <a-col :span="8"><a-form-item label="机柜列数量" required><a-input-number v-model:value="configForm.row_count" :min="1" :max="26" addon-after="列" style="width: 100%" /></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="每列机柜数量" required><a-input-number v-model:value="configForm.cabinets_per_row" :min="1" :max="100" addon-after="个" style="width: 100%" /></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="单柜高度" required><a-input-number v-model:value="configForm.cabinet_total_u" :min="1" :max="60" addon-after="U" style="width: 100%" /></a-form-item></a-col>
          </a-row>
          <a-descriptions bordered size="small" :column="1">
            <a-descriptions-item label="生成机柜">{{ configForm.row_count * configForm.cabinets_per_row }} 个</a-descriptions-item>
            <a-descriptions-item label="总 U 位">{{ configForm.row_count * configForm.cabinets_per_row * configForm.cabinet_total_u }}U</a-descriptions-item>
          </a-descriptions>
          <a-alert v-if="roomRows.length" type="warning" show-icon message="重新配置会替换当前未使用的机柜布局；已有服务器占用 U 位时系统会阻止修改。" style="margin-top: 16px" />
        </a-form>
      </a-modal>
    </div>
  </Page>
</template>
