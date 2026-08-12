<script lang="ts" setup>
import { ref, onMounted } from 'vue';
import { message } from 'ant-design-vue';
import { getPentRuns, createPentRun, cancelPentRun, pausePentRun, resumePentRun, deletePentRun, getPentRunEvents, getPentRunToolCalls, getPentRunTraffic, getPentRunPlan, getPentTasks, createPentTask } from '#/api/pent';
import PageHeader from '../components/PageHeader.vue';
import HttpExchangeCard from '../task-monitor/components/HttpExchangeCard.vue';
import { useRunStatusStreams } from '../composables/useRunStatusStreams';

const runs = ref<any[]>([]); const tasks = ref<any[]>([]); const loading = ref(false);
const selectedRunKeys = ref<string[]>([]);
const rowSelection = { selectedRowKeys: selectedRunKeys, onChange: (keys: string[]) => { selectedRunKeys.value = keys; } };
const createTaskVisible = ref(false); const createRunVisible = ref(false);
const taskForm = ref({ name: '', target_scope: '["10.0.0.0/24"]', priority: 'medium', authorization_confirmed: false });
const selectedTaskId = ref(''); const detailVisible = ref(false); const detailRun = ref<any>(null);
const events = ref<any[]>([]); const toolCalls = ref<any[]>([]); const traffic = ref<any[]>([]);
const plan = ref<any>({ nodes: [] });

const runStreams = useRunStatusStreams(async (runId, status, event) => {
  const run = runs.value.find((item: any) => item.id === runId);
  if (run) run.status = status;
  if (detailRun.value?.id !== runId) return;
  detailRun.value.status = status;
  try {
    const data = JSON.parse(event?.data || '{}');
    if (data.event_id && !events.value.some((item: any) => item.event_id === data.event_id)) events.value.push(data);
  } catch { /* ignore malformed event */ }
  if (String(event?.event || '').includes('tool')) {
    [toolCalls.value, traffic.value] = await Promise.all([
      getPentRunToolCalls(runId), getPentRunTraffic(runId),
    ]) as [any[], any[]];
  }
});

async function fetchRuns(silent = false) { if (!silent) loading.value=true; try { runs.value=(await getPentRuns()) as any[]; runStreams.sync(runs.value.filter((run:any) => ['queued','running','cancelling'].includes(run.status)).map((run:any) => run.id)); } catch { if (!silent) message.error('加载失败'); } finally { if (!silent) loading.value=false; } }
onMounted(async () => { await fetchRuns(); tasks.value=(await getPentTasks()) as any[]; });

async function handleCreateTask() {
  if (!taskForm.value.authorization_confirmed) { message.warning('请先确认目标测试授权'); return; }
  await createPentTask({...taskForm.value, target_scope: JSON.parse(taskForm.value.target_scope || '[]')});
  message.success('任务创建成功'); createTaskVisible.value=false; tasks.value=(await getPentTasks()) as any[];
}
async function handleCreateRun() { if (selectedTaskId.value) { await createPentRun({task_id:selectedTaskId.value}); message.success('Run 已创建'); createRunVisible.value=false; fetchRuns(); } }
async function handleDeleteRun(run: any) {
  try {
    await deletePentRun(run.id);
    message.success('Run 已删除');
    if (detailRun.value?.id === run.id) detailVisible.value = false;
    await fetchRuns();
  } catch (error: any) {
    message.error(error?.message || '删除失败');
  }
}

async function showDetail(run: any) {
  detailRun.value = run; detailVisible.value = true;
  [events.value, toolCalls.value, traffic.value, plan.value] = await Promise.all([
    getPentRunEvents(run.id), getPentRunToolCalls(run.id), getPentRunTraffic(run.id), getPentRunPlan(run.id),
  ]) as [any[], any[], any[], any];
}

function pretty(value: any) { return JSON.stringify(value ?? {}, null, 2); }
function eventTitle(event: any) {
  const labels: Record<string,string> = {
    'conversation.tool_calls': '模型选择工具',
    'conversation.tool_result': '工具返回结果',
    'conversation.assistant_message': '模型分析',
  };
  return labels[event.event_type] || event.event_type.replaceAll('.', ' / ');
}
function eventTone(type = '') {
  if (type.includes('failed') || type.includes('error')) return 'error';
  if (type.includes('completed') || type.includes('result')) return 'success';
  if (type.includes('tool')) return 'tool';
  return 'info';
}

const statusColor: Record<string,string> = { queued:'default', running:'processing', paused:'gold', completed:'green', failed:'red', cancelled:'orange' };
const columns = [{ title:'Run ID',dataIndex:'id',width:120,ellipsis:true },{ title:'状态',dataIndex:'status',width:90 },{ title:'Worker',dataIndex:'worker_id',width:130 },{ title:'开始',dataIndex:'started_at',width:170 },{ title:'操作',key:'actions',width:240 }];
</script>

<template>
  <div style="padding:24px;background:var(--ant-color-bg-layout);min-height:100%">
    <PageHeader title="运行管理" subtitle="查看和管理 Agent 执行运行">
      <template #extra>
        <a-space><a-button @click="createTaskVisible=true">新建任务</a-button><a-button type="primary" @click="createRunVisible=true">创建 Run</a-button><a-button @click="fetchRuns">刷新</a-button></a-space>
      </template>
    </PageHeader>
    <a-card size="small" style="border-radius:8px">
      <div v-if="selectedRunKeys.length > 0" style="margin-bottom:8px;padding:8px 12px;background:var(--ant-color-primary-bg);border-radius:6px;display:flex;align-items:center;gap:12px">
        <span>已选 <b>{{ selectedRunKeys.length }}</b> 项</span>
        <a-button size="small" @click="selectedRunKeys.forEach(id => cancelPentRun(id)); selectedRunKeys=[]; fetchRuns()">批量取消</a-button>
        <a-button size="small" danger @click="selectedRunKeys.forEach(id => deletePentRun(id)); selectedRunKeys=[]; fetchRuns()">批量删除</a-button>
        <a-button size="small" @click="selectedRunKeys = []">取消选择</a-button>
      </div>
      <a-table :row-selection="rowSelection" :columns="columns" :data-source="runs" :loading="loading" row-key="id" size="middle" :pagination="{pageSize:15}">
        <template #bodyCell="{ column, record }">
          <template v-if="column.key==='status'"><a-tag :color="statusColor[record.status]">{{ record.status }}</a-tag></template>
          <template v-if="column.key==='actions'">
            <a-space>
              <a-button type="link" size="small" @click="showDetail(record)">详情</a-button>
              <a-button v-if="['queued', 'running', 'waiting_approval'].includes(record.status)" type="link" size="small" @click="pausePentRun(record.id).then(fetchRuns)">暂停</a-button>
              <a-button v-if="record.status === 'paused'" type="link" size="small" @click="resumePentRun(record.id).then(fetchRuns)">继续</a-button>
              <a-button v-if="['queued', 'running', 'cancelling'].includes(record.status)" type="link" size="small" danger @click="cancelPentRun(record.id).then(fetchRuns)">取消</a-button>
              <a-popconfirm
                v-if="['completed', 'failed', 'cancelled', 'cancelling'].includes(record.status)"
                title="确定删除该 Run 及其全部运行数据？"
                ok-text="删除"
                cancel-text="取消"
                @confirm="handleDeleteRun(record)"
              >
                <a-button type="link" size="small" danger>删除</a-button>
              </a-popconfirm>
            </a-space>
          </template>
        </template>
      </a-table>
    </a-card>

    <a-modal v-model:open="detailVisible" title="Run 详情" width="960px" :footer="null">
      <template v-if="detailRun">
        <a-descriptions bordered size="small" :column="2" style="margin-bottom:16px">
          <a-descriptions-item label="Run ID">{{ detailRun.id?.slice(0,16) }}...</a-descriptions-item>
          <a-descriptions-item label="状态"><a-tag :color="statusColor[detailRun.status]">{{ detailRun.status }}</a-tag></a-descriptions-item>
          <a-descriptions-item label="Worker">{{ detailRun.worker_id || '-' }}</a-descriptions-item>
          <a-descriptions-item label="开始">{{ detailRun.started_at || '-' }}</a-descriptions-item>
        </a-descriptions>
        <a-tabs>
          <a-tab-pane key="trace" tab="执行轨迹">
        <div class="run-summary">
          <span>事件 {{ events.length }}</span>
          <span>工具调用 {{ toolCalls.length }}</span>
          <span>失败 {{ toolCalls.filter((item:any) => item.exit_code != null && item.exit_code !== 0).length }}</span>
        </div>
        <a-empty v-if="events.length === 0 && toolCalls.length === 0" description="暂无执行记录" />
        <div v-else class="execution-trace">
          <div v-for="evt in events" :key="evt.event_id" class="trace-item" :data-tone="eventTone(evt.event_type)">
            <div class="trace-dot"></div>
            <div class="trace-card">
              <div class="trace-head"><strong>{{ eventTitle(evt) }}</strong><span>#{{ evt.sequence }}</span></div>
              <div class="trace-preview">{{ pretty(evt.payload).slice(0, 180) }}</div>
              <details><summary>展开详情</summary><pre>{{ pretty(evt.payload) }}</pre></details>
            </div>
          </div>
          <a-collapse v-if="toolCalls.length" ghost class="tool-call-list">
            <a-collapse-panel key="tools" :header="`工具调用明细（${toolCalls.length}）`">
              <div v-for="call in toolCalls" :key="call.id" class="tool-call-row">
                <div><strong>🔧 {{ call.tool_name }}</strong><a-tag :color="call.exit_code === 0 ? 'success' : 'error'">{{ call.exit_code === 0 ? '成功' : `退出码 ${call.exit_code ?? '-'}` }}</a-tag></div>
                <details><summary>参数与执行信息</summary><pre>{{ pretty({ params: call.params_summary, audit: call.execution_audit, backend: call.executor_backend, duration_ms: call.duration_ms }) }}</pre></details>
              </div>
            </a-collapse-panel>
          </a-collapse>
        </div>
          </a-tab-pane>
          <a-tab-pane key="traffic" :tab="`HTTP 流量 (${traffic.length})`">
            <a-empty v-if="traffic.length === 0" description="暂无 HTTP 流量" />
            <div v-else class="traffic-list">
              <HttpExchangeCard v-for="item in traffic" :key="item.id" :data="item" />
            </div>
          </a-tab-pane>
          <a-tab-pane key="plan" :tab="`执行计划 (${plan.nodes?.length || 0})`">
            <a-timeline>
              <a-timeline-item v-for="node in plan.nodes || []" :key="node.node_id" :color="node.status === 'completed' ? 'green' : node.status === 'failed' || node.status === 'blocked' ? 'red' : 'blue'">
                <strong>{{ node.objective }}</strong>
                <a-tag class="ml-2">{{ node.status }}</a-tag>
              </a-timeline-item>
            </a-timeline>
          </a-tab-pane>
        </a-tabs>
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

<style scoped>
.run-summary { display:flex;flex-wrap:wrap;gap:10px;margin-bottom:16px; }
.run-summary span { padding:5px 10px;color:var(--ant-color-text-secondary);font-size:12px;background:var(--ant-color-fill-quaternary);border-radius:999px; }
.execution-trace { position:relative;padding-left:18px; }
.execution-trace::before { position:absolute;top:8px;bottom:8px;left:5px;width:1px;background:var(--ant-color-border-secondary);content:''; }
.trace-item { position:relative;margin-bottom:12px; }
.trace-dot { position:absolute;top:14px;left:-17px;width:9px;height:9px;background:var(--ant-color-primary);border:2px solid var(--ant-color-bg-container);border-radius:50%; }
.trace-item[data-tone='success'] .trace-dot { background:var(--ant-color-success); }
.trace-item[data-tone='error'] .trace-dot { background:var(--ant-color-error); }
.trace-item[data-tone='tool'] .trace-dot { background:var(--ant-color-warning); }
.trace-card { padding:10px 12px;background:var(--ant-color-fill-quaternary);border:1px solid var(--ant-color-border-secondary);border-radius:8px; }
.trace-head { display:flex;justify-content:space-between;gap:12px; }
.trace-head span { color:var(--ant-color-text-tertiary);font-size:11px; }
.trace-preview { margin:5px 0;overflow:hidden;color:var(--ant-color-text-secondary);font-size:12px;white-space:nowrap;text-overflow:ellipsis; }
details summary { width:fit-content;color:var(--ant-color-primary);font-size:12px;cursor:pointer; }
details pre { max-height:360px;margin:8px 0 0;padding:10px;overflow:auto;color:var(--ant-color-text);font-size:11px;white-space:pre-wrap;background:var(--ant-color-bg-container);border-radius:6px; }
.tool-call-list { margin-top:12px; }
.tool-call-row { padding:10px 0;border-bottom:1px solid var(--ant-color-border-secondary); }
.tool-call-row strong { margin-right:8px; }
.traffic-list { display:flex;flex-direction:column;gap:12px;max-height:620px;overflow:auto;padding-right:4px; }
</style>
