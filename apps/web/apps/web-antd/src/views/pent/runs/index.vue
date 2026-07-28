<script lang="ts" setup>
import { ref, onMounted, onUnmounted, watch } from 'vue';
import { message } from 'ant-design-vue';
import { getPentRuns, createPentRun, cancelPentRun, getPentRunEvents, getPentTasks, createPentTask } from '#/api/pent';
import PageHeader from '../components/PageHeader.vue';

const runs = ref<any[]>([]); const tasks = ref<any[]>([]); const loading = ref(false);
const createTaskVisible = ref(false); const createRunVisible = ref(false);
const taskForm = ref({ name: '', target_scope: '["10.0.0.0/24"]', priority: 'medium', authorization_confirmed: false });
const selectedTaskId = ref(''); const detailVisible = ref(false); const detailRun = ref<any>(null);
const events = ref<any[]>([]); let eventTimer: any = null;

async function fetchRuns() { loading.value=true; try { runs.value=(await getPentRuns()) as any[]; } catch { message.error('加载失败'); } finally { loading.value=false; } }
onMounted(async () => { await fetchRuns(); tasks.value=(await getPentTasks()) as any[]; });
onUnmounted(() => { if (eventTimer) clearInterval(eventTimer); });
watch(detailVisible, (v) => { if (!v && eventTimer) { clearInterval(eventTimer); eventTimer = null; } });

async function handleCreateTask() {
  if (!taskForm.value.authorization_confirmed) { message.warning('请先确认目标测试授权'); return; }
  await createPentTask({...taskForm.value, target_scope: JSON.parse(taskForm.value.target_scope || '[]')});
  message.success('任务创建成功'); createTaskVisible.value=false; tasks.value=(await getPentTasks()) as any[];
}
async function handleCreateRun() { if (selectedTaskId.value) { await createPentRun({task_id:selectedTaskId.value}); message.success('Run 已创建'); createRunVisible.value=false; fetchRuns(); } }

async function showDetail(run: any) {
  detailRun.value = run; detailVisible.value = true;
  if (eventTimer) clearInterval(eventTimer);
  events.value = (await getPentRunEvents(run.id)) as any[];
  eventTimer = setInterval(async () => {
    events.value = (await getPentRunEvents(run.id)) as any[];
  }, 3000);
}

const statusColor: Record<string,string> = { queued:'default', running:'processing', completed:'green', failed:'red', cancelled:'orange' };
const columns = [{ title:'Run ID',dataIndex:'id',width:120,ellipsis:true },{ title:'状态',dataIndex:'status',width:90 },{ title:'Worker',dataIndex:'worker_id',width:130 },{ title:'开始',dataIndex:'started_at',width:170 },{ title:'操作',key:'actions',width:180 }];
</script>

<template>
  <div style="padding:24px;background:var(--ant-color-bg-layout);min-height:100%">
    <PageHeader title="运行管理" subtitle="查看和管理 Agent 执行运行">
      <template #extra>
        <a-space><a-button @click="createTaskVisible=true">新建任务</a-button><a-button type="primary" @click="createRunVisible=true">创建 Run</a-button><a-button @click="fetchRuns">刷新</a-button></a-space>
      </template>
    </PageHeader>
    <a-card size="small" style="border-radius:8px">
      <a-table :columns="columns" :data-source="runs" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:15}">
        <template #bodyCell="{ column, record }">
          <template v-if="column.key==='status'"><a-tag :color="statusColor[record.status]">{{ record.status }}</a-tag></template>
          <template v-if="column.key==='actions'">
            <a-space>
              <a-button type="link" size="small" @click="showDetail(record)">详情</a-button>
              <a-button v-if="record.status==='running'" type="link" size="small" danger @click="cancelPentRun(record.id).then(fetchRuns)">取消</a-button>
            </a-space>
          </template>
        </template>
      </a-table>
    </a-card>

    <a-modal v-model:open="detailVisible" title="Run 详情" width="800px" :footer="null">
      <template v-if="detailRun">
        <a-descriptions bordered size="small" :column="2" style="margin-bottom:16px">
          <a-descriptions-item label="Run ID">{{ detailRun.id?.slice(0,16) }}...</a-descriptions-item>
          <a-descriptions-item label="状态"><a-tag :color="statusColor[detailRun.status]">{{ detailRun.status }}</a-tag></a-descriptions-item>
          <a-descriptions-item label="Worker">{{ detailRun.worker_id || '-' }}</a-descriptions-item>
          <a-descriptions-item label="开始">{{ detailRun.started_at || '-' }}</a-descriptions-item>
        </a-descriptions>
        <a-divider>事件时间线 ({{ events.length }})</a-divider>
        <a-timeline>
          <a-timeline-item v-for="evt in events" :key="evt.event_id" :color="evt.event_type.includes('failed')?'red':evt.event_type.includes('completed')?'green':'blue'">
            <a-tag size="small" style="margin-right:6px">{{ evt.event_type }}</a-tag>
            <span style="font-size:11px;color:var(--ant-color-text-tertiary)">{{ JSON.stringify(evt.payload)?.slice(0,80) }}</span>
          </a-timeline-item>
        </a-timeline>
      </template>
    </a-modal>

    <a-modal v-model:open="createTaskVisible" title="新建任务" @ok="handleCreateTask" width="500px">
      <a-form layout="vertical">
        <a-form-item label="任务名称" required><a-input v-model:value="taskForm.name" /></a-form-item>
        <a-form-item label="目标范围" required><a-input v-model:value="taskForm.target_scope" /></a-form-item>
        <a-form-item label="优先级"><a-select v-model:value="taskForm.priority"><a-select-option value="low">低</a-select-option><a-select-option value="medium">中</a-select-option><a-select-option value="high">高</a-select-option><a-select-option value="critical">紧急</a-select-option></a-select></a-form-item>
        <a-form-item required><a-checkbox v-model:checked="taskForm.authorization_confirmed">我确认已获得上述目标的安全测试授权</a-checkbox></a-form-item>
      </a-form>
    </a-modal>
    <a-modal v-model:open="createRunVisible" title="创建 Run" @ok="handleCreateRun" width="500px">
      <a-form layout="vertical">
        <a-form-item label="选择任务" required><a-select v-model:value="selectedTaskId" placeholder="选择一个任务" :options="tasks.map((t:any)=>({value:t.id,label:t.name}))" style="width:100%" /></a-form-item>
      </a-form>
    </a-modal>
  </div>
</template>
