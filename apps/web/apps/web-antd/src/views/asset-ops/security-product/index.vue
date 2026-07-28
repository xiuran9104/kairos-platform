<script lang="ts" setup>
import { ref, onMounted } from 'vue';
import { Page } from '@vben/common-ui';
import { message } from 'ant-design-vue';
import { getSecurityProductList, createSecurityProduct, updateSecurityProduct, deleteSecurityProduct } from '#/api/scan/security-product';
interface Product { id?: number; name: string; category: string; vendor: string; model: string; version: string; license_type: string; license_expiry?: string; management_ip?: string; deployment_mode: string; status: string; contact_person: string; contact_phone: string; }
const data = ref<Product[]>([]); const loading = ref(false); const modalVisible = ref(false); const editingId = ref<number | undefined>();
const form = ref<Product>({ name: '', category: '', vendor: '', model: '', version: '', license_type: '', deployment_mode: '', status: 'active', contact_person: '', contact_phone: '' });
const columns = [{ title: '名称', dataIndex: 'name' }, { title: '类别', dataIndex: 'category' }, { title: '厂商', dataIndex: 'vendor' }, { title: '型号', dataIndex: 'model' }, { title: '版本', dataIndex: 'version' }, { title: '部署模式', dataIndex: 'deployment_mode' }, { title: '状态', dataIndex: 'status' }, { title: '操作', key: 'actions', width: 200 }];
async function fetchData() { loading.value = true; try { data.value = (await getSecurityProductList()) as any; } catch { message.error('加载失败'); } finally { loading.value = false; } }
onMounted(fetchData);
function openCreate() { editingId.value = undefined; form.value = { name: '', category: '', vendor: '', model: '', version: '', license_type: '', deployment_mode: '', status: 'active', contact_person: '', contact_phone: '' }; modalVisible.value = true; }
function openEdit(r: Product) { editingId.value = r.id; form.value = { ...r }; modalVisible.value = true; }
async function handleSubmit() {
  const payload = { ...form.value };
  // Convert Dayjs object from date-picker to string for API
  if (payload.license_expiry && typeof payload.license_expiry !== 'string') {
    payload.license_expiry = (payload.license_expiry as any).format?.('YYYY-MM-DD') ?? String(payload.license_expiry);
  }
  try {
    if (editingId.value) { await updateSecurityProduct(editingId.value, payload as any); message.success('更新成功'); }
    else { await createSecurityProduct(payload as any); message.success('创建成功'); }
    modalVisible.value = false; fetchData();
  } catch { message.error('保存失败'); }
}
</script>
<template>
  <Page auto-content-height>
    <div style="padding:16px">
      <a-space style="margin-bottom:16px"><a-button type="primary" @click="openCreate">新建安全产品</a-button><a-button @click="fetchData">刷新</a-button></a-space>
      <a-table :columns="columns" :data-source="data" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:20}">
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'status'"><a-tag :color="record.status === 'active' ? 'green' : 'red'">{{ record.status }}</a-tag></template>
          <template v-if="column.key === 'actions'">
            <a-space><a-button size="small" @click="openEdit(record)">编辑</a-button><a-popconfirm title="确认删除?" @confirm="deleteSecurityProduct(record.id!).then(fetchData)"><a-button size="small" danger>删除</a-button></a-popconfirm></a-space>
          </template>
        </template>
      </a-table>
      <a-modal v-model:open="modalVisible" :title="editingId ? '编辑安全产品' : '新建安全产品'" @ok="handleSubmit" width="600px">
        <a-form layout="vertical">
          <a-row :gutter="16">
            <a-col :span="12"><a-form-item label="名称" required><a-input v-model:value="form.name" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="类别" required><a-input v-model:value="form.category" /></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="厂商" required><a-input v-model:value="form.vendor" /></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="型号" required><a-input v-model:value="form.model" /></a-form-item></a-col>
            <a-col :span="8"><a-form-item label="版本" required><a-input v-model:value="form.version" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="许可类型" required><a-input v-model:value="form.license_type" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="许可到期"><a-date-picker v-model:value="form.license_expiry" style="width:100%" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="管理IP"><a-input v-model:value="form.management_ip" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="部署模式" required><a-input v-model:value="form.deployment_mode" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="联系人" required><a-input v-model:value="form.contact_person" /></a-form-item></a-col>
            <a-col :span="12"><a-form-item label="电话" required><a-input v-model:value="form.contact_phone" /></a-form-item></a-col>
          </a-row>
        </a-form>
      </a-modal>
    </div>
  </Page>
</template>
