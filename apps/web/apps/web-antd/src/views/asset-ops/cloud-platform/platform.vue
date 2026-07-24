<script lang="ts" setup>
import { ref, onMounted } from 'vue';
import { Page } from '@vben/common-ui';
import { message } from 'ant-design-vue';
import { getCloudPlatformList, createCloudPlatform, updateCloudPlatform, deleteCloudPlatform } from '#/api/scan/cloud-platform';
interface Platform { id?: number; zone_id: number; platform_name: string; platform_code: string; description?: string; }
const data = ref<Platform[]>([]); const loading = ref(false); const modalVisible = ref(false); const editingId = ref<number | undefined>();
const form = ref<Platform>({ zone_id: 0, platform_name: '', platform_code: '', description: '' });
const columns = [{ title: '平台名称', dataIndex: 'platform_name' }, { title: '编码', dataIndex: 'platform_code' }, { title: '云区ID', dataIndex: 'zone_id' }, { title: '描述', dataIndex: 'description' }, { title: '操作', key: 'actions', width: 200 }];
async function fetchData() { loading.value = true; try { data.value = (await getCloudPlatformList()) as any; } catch { message.error('加载失败'); } finally { loading.value = false; } }
onMounted(fetchData);
function openCreate() { editingId.value = undefined; form.value = { zone_id: 0, platform_name: '', platform_code: '', description: '' }; modalVisible.value = true; }
function openEdit(r: Platform) { editingId.value = r.id; form.value = { ...r }; modalVisible.value = true; }
async function handleSubmit() { if (editingId.value) { await updateCloudPlatform(editingId.value, form.value as any); message.success('更新成功'); } else { await createCloudPlatform(form.value as any); message.success('创建成功'); } modalVisible.value = false; fetchData(); }
</script>
<template>
  <Page auto-content-height>
    <div style="padding:16px">
      <a-space style="margin-bottom:16px"><a-button type="primary" @click="openCreate">新建云平台</a-button><a-button @click="fetchData">刷新</a-button></a-space>
      <a-table :columns="columns" :data-source="data" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:20}">
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'actions'">
            <a-space><a-button size="small" @click="openEdit(record)">编辑</a-button><a-popconfirm title="确认删除?" @confirm="deleteCloudPlatform(record.id!).then(fetchData)"><a-button size="small" danger>删除</a-button></a-popconfirm></a-space>
          </template>
        </template>
      </a-table>
      <a-modal v-model:open="modalVisible" :title="editingId ? '编辑云平台' : '新建云平台'" @ok="handleSubmit" width="500px">
        <a-form layout="vertical">
          <a-form-item label="平台名称" required><a-input v-model:value="form.platform_name" placeholder="公众云" /></a-form-item>
          <a-form-item label="编码" required><a-input v-model:value="form.platform_code" placeholder="public" /></a-form-item>
          <a-form-item label="所属云区ID" required><a-input-number v-model:value="form.zone_id" :min="1" style="width:100%" /></a-form-item>
          <a-form-item label="描述"><a-textarea v-model:value="form.description" /></a-form-item>
        </a-form>
      </a-modal>
    </div>
  </Page>
</template>
