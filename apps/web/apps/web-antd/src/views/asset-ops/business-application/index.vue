<script lang="ts" setup>
import { ref, onMounted } from 'vue';
import { Page } from '@vben/common-ui';
import { message } from 'ant-design-vue';
import { getBusinessApplicationList, createBusinessApplication, updateBusinessApplication, deleteBusinessApplication, getApplicationEndpoints, addApplicationEndpoint } from '#/api/scan/business-application';
import { useLocalTableSearch } from '../composables/useLocalTableSearch';
interface App { id?: number; name: string; description?: string; }
const data = ref<App[]>([]); const loading = ref(false); const modalVisible = ref(false); const editingId = ref<number | undefined>();
const { filteredRows, searchText } = useLocalTableSearch(data, ['name', 'description']);
const form = ref<App>({ name: '', description: '' });
const epVisible = ref(false); const epAppId = ref(0); const endpoints = ref<any[]>([]); const epForm = ref({ protocol: 'TCP', dest_ip: '', nat_ip: '', dest_port: '', domain: '' });
const columns = [{ title: '名称', dataIndex: 'name' }, { title: '描述', dataIndex: 'description' }, { title: '操作', key: 'actions', width: 280 }];
async function fetchData() { loading.value = true; try { data.value = (await getBusinessApplicationList()) as any; } catch { message.error('加载失败'); } finally { loading.value = false; } }
onMounted(fetchData);
function openCreate() { editingId.value = undefined; form.value = { name: '', description: '' }; modalVisible.value = true; }
function openEdit(r: App) { editingId.value = r.id; form.value = { ...r }; modalVisible.value = true; }
async function handleSubmit() { if (editingId.value) { await updateBusinessApplication(editingId.value, form.value as any); message.success('更新成功'); } else { await createBusinessApplication(form.value as any); message.success('创建成功'); } modalVisible.value = false; fetchData(); }
async function openEndpoints(app: App) { epAppId.value = app.id!; endpoints.value = (await getApplicationEndpoints(app.id!)) as any; epVisible.value = true; }
async function addEp() { await addApplicationEndpoint(epAppId.value, epForm.value as any); message.success('接口添加成功'); endpoints.value = (await getApplicationEndpoints(epAppId.value)) as any; }
</script>
<template>
  <Page auto-content-height>
    <div style="padding:16px">
      <a-space :size="24" style="margin-bottom:20px"><a-button type="primary" @click="openCreate">新建应用</a-button><a-input-search v-model:value="searchText" allow-clear placeholder="搜索应用名称或描述" style="width:260px" /><a-button @click="fetchData">刷新</a-button></a-space>
      <a-table :columns="columns" :data-source="filteredRows" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:20}">
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'actions'">
            <a-space :size="12">
              <a-button size="small" @click="openEndpoints(record)">接口</a-button>
              <a-button size="small" @click="openEdit(record)">编辑</a-button>
              <a-popconfirm title="确认删除?" @confirm="deleteBusinessApplication(record.id!).then(fetchData)"><a-button size="small" danger>删除</a-button></a-popconfirm>
            </a-space>
          </template>
        </template>
      </a-table>
      <a-modal v-model:open="modalVisible" :title="editingId ? '编辑应用' : '新建应用'" @ok="handleSubmit" width="500px">
        <a-form layout="vertical">
          <a-form-item label="名称" required><a-input v-model:value="form.name" /></a-form-item>
          <a-form-item label="描述"><a-textarea v-model:value="form.description" /></a-form-item>
        </a-form>
      </a-modal>
      <a-modal v-model:open="epVisible" title="应用接口" width="700px" :footer="null">
        <a-table :columns="[{title:'协议',dataIndex:'protocol'},{title:'目标IP',dataIndex:'dest_ip'},{title:'NAT IP',dataIndex:'nat_ip'},{title:'端口',dataIndex:'dest_port'},{title:'域名',dataIndex:'domain'}]" :data-source="endpoints" row-key="id" size="small" :pagination="false" style="margin-bottom:16px" />
        <a-divider>添加接口</a-divider>
        <a-form layout="inline">
          <a-form-item label="协议"><a-select v-model:value="epForm.protocol" style="width:100px"><a-select-option value="TCP">TCP</a-select-option><a-select-option value="UDP">UDP</a-select-option><a-select-option value="HTTP">HTTP</a-select-option></a-select></a-form-item>
          <a-form-item label="目标IP"><a-input v-model:value="epForm.dest_ip" style="width:130px" /></a-form-item>
          <a-form-item label="NAT IP"><a-input v-model:value="epForm.nat_ip" style="width:130px" /></a-form-item>
          <a-form-item label="端口"><a-input v-model:value="epForm.dest_port" style="width:80px" /></a-form-item>
          <a-form-item><a-button type="primary" @click="addEp">添加</a-button></a-form-item>
        </a-form>
      </a-modal>
    </div>
  </Page>
</template>
