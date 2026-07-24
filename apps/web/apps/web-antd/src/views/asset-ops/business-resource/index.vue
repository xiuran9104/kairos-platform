<script lang="ts" setup>
import { ref, onMounted } from 'vue';
import { Page } from '@vben/common-ui';
import { message } from 'ant-design-vue';
import { getBusinessResourceList, createBusinessResource, updateBusinessResource, deleteBusinessResource } from '#/api/scan/business-resource';
interface Resource { id?: number; resource_type: string; ecs_name: string; ecs_status: string; cloud_region: string; cloud_category: string; customer_name: string; cpu_cores: number; memory_gb: number; ip_address: string; application_status: string; delivery_status: string; }
const data = ref<Resource[]>([]); const loading = ref(false); const modalVisible = ref(false); const editingId = ref<number | undefined>();
const form = ref<Resource>({ resource_type: 'cloud', ecs_name: '', ecs_status: '运行中', cloud_region: '', cloud_category: '', customer_name: '', cpu_cores: 4, memory_gb: 8, ip_address: '', application_status: '待审核', delivery_status: '待交付' });
const columns = [{ title: '名称', dataIndex: 'ecs_name' }, { title: '类型', dataIndex: 'resource_type' }, { title: '状态', dataIndex: 'ecs_status' }, { title: '区域', dataIndex: 'cloud_region' }, { title: '客户', dataIndex: 'customer_name' }, { title: '规格', key: 'spec' }, { title: 'IP', dataIndex: 'ip_address' }, { title: '申请状态', dataIndex: 'application_status' }, { title: '交付状态', dataIndex: 'delivery_status' }, { title: '操作', key: 'actions', width: 200 }];
const typeLabel: Record<string, string> = { cloud: '云资源', physical: '物理机' };
async function fetchData() { loading.value = true; try { data.value = (await getBusinessResourceList()) as any; } catch { message.error('加载失败'); } finally { loading.value = false; } }
onMounted(fetchData);
function openCreate() { editingId.value = undefined; form.value = { resource_type: 'cloud', ecs_name: '', ecs_status: '运行中', cloud_region: '', cloud_category: '', customer_name: '', cpu_cores: 4, memory_gb: 8, ip_address: '', application_status: '待审核', delivery_status: '待交付' }; modalVisible.value = true; }
function openEdit(r: Resource) { editingId.value = r.id; form.value = { ...r }; modalVisible.value = true; }
async function handleSubmit() { if (editingId.value) { await updateBusinessResource(editingId.value, form.value as any); message.success('更新成功'); } else { await createBusinessResource(form.value as any); message.success('创建成功'); } modalVisible.value = false; fetchData(); }
</script>
<template>
  <Page auto-content-height>
    <div style="padding:16px">
      <a-space style="margin-bottom:16px"><a-button type="primary" @click="openCreate">新建业务资源</a-button><a-button @click="fetchData">刷新</a-button></a-space>
      <a-table :columns="columns" :data-source="data" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:20}">
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'resource_type'">{{ typeLabel[record.resource_type] || record.resource_type }}</template>
          <template v-if="column.key === 'ecs_status'"><a-tag :color="record.ecs_status === '运行中' ? 'green' : 'default'">{{ record.ecs_status }}</a-tag></template>
          <template v-if="column.key === 'spec'">{{ record.cpu_cores }}核 / {{ record.memory_gb }}GB</template>
          <template v-if="column.key === 'application_status'"><a-tag :color="record.application_status === '已批准' ? 'green' : record.application_status === '待审核' ? 'orange' : 'default'">{{ record.application_status }}</a-tag></template>
          <template v-if="column.key === 'actions'">
            <a-space><a-button size="small" @click="openEdit(record)">编辑</a-button><a-popconfirm title="确认删除?" @confirm="deleteBusinessResource(record.id!).then(fetchData)"><a-button size="small" danger>删除</a-button></a-popconfirm></a-space>
          </template>
        </template>
      </a-table>
      <a-modal v-model:open="modalVisible" :title="editingId ? '编辑业务资源' : '新建业务资源'" @ok="handleSubmit" width="600px">
        <a-form layout="vertical">
          <a-row :gutter="16">
            <a-col :span="12"><a-form-item label="名称" required><a-input v-model:value="form.ecs_name" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="资源类型"><a-select v-model:value="form.resource_type"><a-select-option value="cloud">云资源</a-select-option><a-select-option value="physical">物理机</a-select-option></a-select></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="状态" required><a-input v-model:value="form.ecs_status" /></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="CPU核数"><a-input-number v-model:value="form.cpu_cores" :min="1" style="width:100%" /></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="内存(GB)"><a-input-number v-model:value="form.memory_gb" :min="1" style="width:100%" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="区域" required><a-input v-model:value="form.cloud_region" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="云类别/机房" required><a-input v-model:value="form.cloud_category" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="客户" required><a-input v-model:value="form.customer_name" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="IP地址" required><a-input v-model:value="form.ip_address" /></a-form-item></a-col>
          </a-row>
        </a-form>
      </a-modal>
    </div>
  </Page>
</template>
