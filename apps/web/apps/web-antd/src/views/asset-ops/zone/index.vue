<script lang="ts" setup>
import { ref, onMounted } from 'vue';
import { Page } from '@vben/common-ui';
import { message } from 'ant-design-vue';
import { getZoneList, createZone, updateZone, deleteZone } from '#/api/scan/zone';
interface Zone { id: string; name: string; cidr: string; priority: number; machine_room_name?: string; }
const data = ref<Zone[]>([]); const loading = ref(false); const modalVisible = ref(false); const editingId = ref<string | undefined>();
const form = ref<Zone>({ id: '', name: '', cidr: '', priority: 50 });
const columns = [{ title: '名称', dataIndex: 'name' }, { title: 'CIDR', dataIndex: 'cidr' }, { title: '优先级', dataIndex: 'priority' }, { title: '机房', dataIndex: 'machine_room_name' }, { title: '操作', key: 'actions', width: 200 }];
async function fetchData() { loading.value = true; try { data.value = (await getZoneList()) as any; } catch { message.error('加载失败'); } finally { loading.value = false; } }
onMounted(fetchData);
function openCreate() { editingId.value = undefined; form.value = { id: '', name: '', cidr: '', priority: 50 }; modalVisible.value = true; }
function openEdit(r: Zone) { editingId.value = r.id; form.value = { ...r }; modalVisible.value = true; }
async function handleSubmit() { if (editingId.value) { await updateZone(editingId.value, form.value as any); message.success('更新成功'); } else { await createZone(form.value as any); message.success('创建成功'); } modalVisible.value = false; fetchData(); }
</script>
<template>
  <Page auto-content-height>
    <div style="padding:16px">
      <a-space style="margin-bottom:16px"><a-button type="primary" @click="openCreate">新建区域</a-button><a-button @click="fetchData">刷新</a-button></a-space>
      <a-table :columns="columns" :data-source="data" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:20}">
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'actions'">
            <a-space><a-button size="small" @click="openEdit(record)">编辑</a-button><a-popconfirm title="确认删除?" @confirm="deleteZone(record.id).then(fetchData)"><a-button size="small" danger>删除</a-button></a-popconfirm></a-space>
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
