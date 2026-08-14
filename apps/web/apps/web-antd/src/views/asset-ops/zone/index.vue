<script lang="ts" setup>
import type { Ref } from 'vue';

import { Page } from '@vben/common-ui';
import { getZoneList, createZone, updateZone, deleteZone } from '#/api/scan/zone';
import { useCrudList } from '../composables/useCrudList';

interface Zone { id: string; name: string; cidr: string; priority: number; machine_room_name?: string; }

const {
  loading, modalVisible, editingId, searchText, form: crudForm,
  filtered, fetchData, openCreate, openEdit, handleSubmit, handleDelete,
} = useCrudList<Zone>({
  api: {
    list: getZoneList,
    create: (d) => createZone(d as any),
    update: (id, d) => updateZone(id as any, d as any),
    del: (id) => deleteZone(id as any),
  },
  defaultForm: () => ({ id: '', name: '', cidr: '', priority: 50 }),
  searchKeys: ['name', 'cidr'],
});
const form = crudForm as Ref<Zone>;

const columns = [
  { title: '名称', dataIndex: 'name' },
  { title: 'CIDR', dataIndex: 'cidr' },
  { title: '优先级', dataIndex: 'priority' },
  { title: '机房', dataIndex: 'machine_room_name' },
  { title: '操作', key: 'actions', width: 200 },
];
</script>

<template>
  <Page auto-content-height>
    <div style="padding:16px">
      <a-space :size="24" style="margin-bottom:20px">
        <a-input-search v-model:value="searchText" placeholder="搜索名称/CIDR" style="width:220px" allow-clear />
        <a-button type="primary" @click="openCreate">新建区域</a-button>
        <a-button @click="fetchData">刷新</a-button>
      </a-space>
      <a-table :columns="columns" :data-source="filtered" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:20}">
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'actions'">
            <a-space>
              <a-button size="small" @click="openEdit(record)">编辑</a-button>
              <a-popconfirm title="确认删除?" @confirm="handleDelete(record.id)">
                <a-button size="small" danger>删除</a-button>
              </a-popconfirm>
            </a-space>
          </template>
        </template>
      </a-table>
      <a-modal v-model:open="modalVisible" :title="editingId ? '编辑区域' : '新建区域'" @ok="handleSubmit" width="500px">
        <a-form layout="vertical">
          <a-form-item label="名称" required><a-input v-model:value="form.name" placeholder="Intranet" /></a-form-item>
          <a-form-item label="CIDR" required><a-input v-model:value="form.cidr" placeholder="192.168.0.0/16" /></a-form-item>
          <a-form-item label="优先级"><a-input-number v-model:value="form.priority" :min="1" :max="100" style="width:100%" /></a-form-item>
        </a-form>
      </a-modal>
    </div>
  </Page>
</template>
