<script lang="ts" setup>
import { onMounted, ref } from 'vue';
import { message } from 'ant-design-vue';
import {
  cancelPentRun,
  createPentBatchRuns,
  createPentRun,
  createPentTask,
  deletePentRun,
  getPentProjects,
  getPentRunEvents,
  getPentRunFindings,
  getPentRunPlan,
  getPentRunToolCalls,
  getPentRunTraffic,
  getPentRuns,
  getPentTasks,
  pausePentRun,
  resumePentRun,
} from '#/api/pent';
import PageHeader from '../components/PageHeader.vue';
import HttpExchangeCard from '../task-monitor/components/HttpExchangeCard.vue';
import { useRunStatusStreams } from '../composables/useRunStatusStreams';

const runs = ref<any[]>([]);
const tasks = ref<any[]>([]);
const projects = ref<any[]>([]);
const loading = ref(false);
const filters = ref<{ status?: string; project_id?: string }>({});
const pagination = ref({ current: 1, pageSize: 15, total: 0 });

const selectedRunKeys = ref<string[]>([]);
const rowSelection = {
  selectedRowKeys: selectedRunKeys,
  onChange: (keys: string[]) => {
    selectedRunKeys.value = keys;
  },
};

const createTaskVisible = ref(false);
const createRunVisible = ref(false);
const batchVisible = ref(false);
const batchForm = ref({
  project_id: undefined as string | undefined,
  name: '',
  objective: '',
  targetsText: '',
  priority: 'medium',
});
const taskForm = ref({
  name: '',
  target_scope: '["10.0.0.0/24"]',
  priority: 'medium',
  project_id: undefined as string | undefined,
  authorization_confirmed: false,
});
const selectedTaskId = ref('');

const detailVisible = ref(false);
const detailRun = ref<any>(null);
const detailTab = ref('trace');
const events = ref<any[]>([]);
const toolCalls = ref<any[]>([]);
const traffic = ref<any[]>([]);
const findings = ref<any[]>([]);
const plan = ref<any>({ nodes: [] });

const statusColor: Record<string, string> = {
  queued: 'default',
  running: 'processing',
  paused: 'gold',
  completed: 'green',
  failed: 'red',
  cancelled: 'orange',
  blocked: 'red',
  cancelling: 'orange',
  waiting_approval: 'gold',
};

const runStreams = useRunStatusStreams(async (runId, status, event) => {
  const run = runs.value.find((item: any) => item.id === runId);
  if (run) run.status = status;
  if (detailRun.value?.id !== runId) return;
  detailRun.value.status = status;
  try {
    const data = JSON.parse(event?.data || '{}');
    if (data.event_id && !events.value.some((item: any) => item.event_id === data.event_id)) {
      events.value.push(data);
    }
  } catch {
    /* ignore malformed event */
  }
  if (String(event?.event || '').includes('tool')) {
    [toolCalls.value, traffic.value] = (await Promise.all([
      getPentRunToolCalls(runId),
      getPentRunTraffic(runId),
    ])) as [any[], any[]];
  }
});

async function fetchRuns(silent = false) {
  if (!silent) loading.value = true;
  try {
    const params: Record<string, any> = { limit: 500 };
    if (filters.value.status) params.status = filters.value.status;
    if (filters.value.project_id) params.project_id = filters.value.project_id;
    runs.value = (await getPentRuns(params)) as any[];
    pagination.value.total = runs.value.length;
    runStreams.sync(
      runs.value
        .filter((run: any) => ['queued', 'running', 'cancelling'].includes(run.status))
        .map((run: any) => run.id),
    );
  } catch {
    if (!silent) message.error('加载失败');
  } finally {
    if (!silent) loading.value = false;
  }
}

onMounted(async () => {
  await Promise.all([fetchRuns(), loadTasks(), loadProjects()]);
});

async function loadTasks() {
  try {
    tasks.value = (await getPentTasks()) as any[];
  } catch {
    tasks.value = [];
  }
}
async function loadProjects() {
  try {
    projects.value = (await getPentProjects()) as any[];
  } catch {
    projects.value = [];
  }
}

async function handleCreateTask() {
  if (!taskForm.value.name.trim()) {
    message.warning('请输入任务名称');
    return;
  }
  if (!taskForm.value.authorization_confirmed) {
    message.warning('请先确认目标测试授权');
    return;
  }
  try {
    await createPentTask({
      name: taskForm.value.name,
      project_id: taskForm.value.project_id,
      target_scope: JSON.parse(taskForm.value.target_scope || '[]'),
      priority: taskForm.value.priority,
    });
    message.success('任务创建成功');
    createTaskVisible.value = false;
    await loadTasks();
  } catch (error: any) {
    message.error(error?.message || '创建失败');
  }
}

async function handleCreateRun() {
  if (!selectedTaskId.value) return;
  try {
    await createPentRun({ task_id: selectedTaskId.value });
    message.success('Run 已创建');
    createRunVisible.value = false;
    await fetchRuns();
  } catch (error: any) {
    message.error(error?.message || '创建失败');
  }
}

async function handleBatchCreate() {
  if (!batchForm.value.project_id) {
    message.warning('请选择项目');
    return;
  }
  const targets = batchForm.value.targetsText
    .split(/[\n,]/)
    .map((s) => s.trim())
    .filter(Boolean);
  if (!targets.length) {
    message.warning('请填写至少一个目标（换行或逗号分隔）');
    return;
  }
  try {
    const res: any = await createPentBatchRuns({
      project_id: batchForm.value.project_id!,
      targets,
      name: batchForm.value.name || undefined,
      objective: batchForm.value.objective || undefined,
      priority: batchForm.value.priority,
    });
    message.success(`已批量创建 ${res?.total ?? targets.length} 个 Run`);
    batchVisible.value = false;
    await fetchRuns();
  } catch (error: any) {
    message.error(error?.message || '批量创建失败');
  }
}

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
  detailRun.value = run;
  detailVisible.value = true;
  detailTab.value = 'trace';
  [events.value, toolCalls.value, traffic.value, plan.value, findings.value] = (await Promise.all([
    getPentRunEvents(run.id),
    getPentRunToolCalls(run.id),
    getPentRunTraffic(run.id),
    getPentRunPlan(run.id),
    getPentRunFindings(run.id),
  ])) as [any[], any[], any[], any, any[]];
}

function pretty(value: any) {
  return JSON.stringify(value ?? {}, null, 2);
}
function executionAudit(call: any) {
  const value = call?.execution_audit;
  if (typeof value !== 'string') return value || {};
  try {
    return JSON.parse(value);
  } catch {
    return {};
  }
}
function eventTitle(event: any) {
  const labels: Record<string, string> = {
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
function fmt(value?: string) {
  return value ? new Date(value).toLocaleString() : '-';
}
function targetsText(run: any) {
  const targets = run?.targets;
  if (Array.isArray(targets) && targets.length) return targets.join(', ');
  if (run?.scope_snapshot?.targets) return run.scope_snapshot.targets.join(', ');
  return run?.objective || '-';
}
function severityColor(sev: string) {
  return { critical: 'red', high: 'volcano', medium: 'orange', low: 'gold', info: 'blue' }[sev] || 'default';
}

const columns = [
  { title: '目标 / 任务', key: 'objective', width: 260, ellipsis: true },
  { title: '项目', dataIndex: 'project_name', key: 'project', width: 140, ellipsis: true },
  { title: '状态', dataIndex: 'status', key: 'status', width: 110 },
  { title: 'Worker', dataIndex: 'worker_id', key: 'worker', width: 130, ellipsis: true },
  { title: '漏洞', dataIndex: 'finding_count', key: 'findings', width: 70, align: 'right' },
  { title: '工具调用', dataIndex: 'tool_call_count', key: 'tools', width: 90, align: 'right' },
  { title: '开始', dataIndex: 'started_at', key: 'started', width: 170 },
  { title: '操作', key: 'actions', width: 250 },
];
</script>

<template>
  <div style="padding: 24px; background: var(--ant-color-bg-layout); min-height: 100%">
    <PageHeader title="运行管理" subtitle="查看和管理 Agent 执行运行">
      <template #extra>
        <a-space>
          <a-button @click="createTaskVisible = true">新建任务</a-button>
          <a-button @click="batchVisible = true">批量创建 Run</a-button>
          <a-button type="primary" @click="createRunVisible = true">创建 Run</a-button>
          <a-button @click="fetchRuns">刷新</a-button>
        </a-space>
      </template>
    </PageHeader>

    <a-card size="small" style="border-radius: 8px">
      <a-space style="margin-bottom: 12px" wrap>
        <a-select
          v-model:value="filters.status"
          allow-clear
          placeholder="全部状态"
          style="width: 150px"
          @change="fetchRuns"
        >
          <a-select-option value="queued">排队中</a-select-option>
          <a-select-option value="running">运行中</a-select-option>
          <a-select-option value="paused">已暂停</a-select-option>
          <a-select-option value="completed">已完成</a-select-option>
          <a-select-option value="failed">失败</a-select-option>
          <a-select-option value="cancelled">已取消</a-select-option>
          <a-select-option value="blocked">已阻塞</a-select-option>
          <a-select-option value="waiting_approval">待审批</a-select-option>
        </a-select>
        <a-select
          v-model:value="filters.project_id"
          allow-clear
          show-search
          option-filter-prop="label"
          placeholder="全部项目"
          style="width: 200px"
          :options="projects.map((p: any) => ({ label: p.name, value: p.id }))"
          @change="fetchRuns"
        />
        <span style="color: var(--ant-color-text-tertiary); font-size: 12px">
          共 {{ pagination.total }} 条
        </span>
      </a-space>

      <div
        v-if="selectedRunKeys.length > 0"
        style="
          margin-bottom: 8px;
          padding: 8px 12px;
          background: var(--ant-color-primary-bg);
          border-radius: 6px;
          display: flex;
          align-items: center;
          gap: 12px;
        "
      >
        <span>已选 <b>{{ selectedRunKeys.length }}</b> 项</span>
        <a-button
          size="small"
          @click="selectedRunKeys.forEach((id) => cancelPentRun(id)); selectedRunKeys = []; fetchRuns()"
        >
          批量取消
        </a-button>
        <a-button
          size="small"
          danger
          @click="selectedRunKeys.forEach((id) => deletePentRun(id)); selectedRunKeys = []; fetchRuns()"
        >
          批量删除
        </a-button>
        <a-button size="small" @click="selectedRunKeys = []">取消选择</a-button>
      </div>

      <a-table
        :row-selection="rowSelection"
        :columns="columns"
        :data-source="runs"
        :loading="loading"
        row-key="id"
        size="middle"
        :pagination="pagination"
        :scroll="{ x: 1200 }"
      >
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'objective'">
            <a-tooltip :title="record.objective || '-'">
              <div style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap">
                {{ targetsText(record) }}
              </div>
            </a-tooltip>
          </template>
          <template v-if="column.key === 'project'">
            <span>{{ record.project_name || '-' }}</span>
          </template>
          <template v-if="column.key === 'status'">
            <a-tag :color="statusColor[record.status]">{{ record.status }}</a-tag>
          </template>
          <template v-if="column.key === 'worker'">
            <span style="font-family: monospace; font-size: 12px">{{ record.worker_id || '-' }}</span>
          </template>
          <template v-if="column.key === 'findings'">
            <a-badge
              v-if="record.finding_count"
              :count="record.finding_count"
              :style="{ backgroundColor: 'var(--ant-color-error)' }"
            />
            <span v-else>0</span>
          </template>
          <template v-if="column.key === 'tools'">
            <span>{{ record.tool_call_count || 0 }}</span>
          </template>
          <template v-if="column.key === 'started'">{{ fmt(record.started_at) }}</template>
          <template v-if="column.key === 'actions'">
            <a-space>
              <a-button type="link" size="small" @click="showDetail(record)">详情</a-button>
              <a-button
                v-if="['queued', 'running', 'waiting_approval'].includes(record.status)"
                type="link"
                size="small"
                @click="pausePentRun(record.id).then(fetchRuns)"
              >
                暂停
              </a-button>
              <a-button
                v-if="record.status === 'paused'"
                type="link"
                size="small"
                @click="resumePentRun(record.id).then(fetchRuns)"
              >
                继续
              </a-button>
              <a-button
                v-if="['queued', 'running', 'cancelling'].includes(record.status)"
                type="link"
                size="small"
                danger
                @click="cancelPentRun(record.id).then(fetchRuns)"
              >
                取消
              </a-button>
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
        <a-descriptions bordered size="small" :column="2" style="margin-bottom: 16px">
          <a-descriptions-item label="Run ID">{{ detailRun.id }}</a-descriptions-item>
          <a-descriptions-item label="状态">
            <a-tag :color="statusColor[detailRun.status]">{{ detailRun.status }}</a-tag>
          </a-descriptions-item>
          <a-descriptions-item label="Worker">{{ detailRun.worker_id || '-' }}</a-descriptions-item>
          <a-descriptions-item label="项目">{{ detailRun.project_name || '-' }}</a-descriptions-item>
          <a-descriptions-item label="目标" :span="2">
            {{ targetsText(detailRun) }}
          </a-descriptions-item>
          <a-descriptions-item label="开始">{{ fmt(detailRun.started_at) }}</a-descriptions-item>
          <a-descriptions-item label="完成">{{ fmt(detailRun.completed_at) }}</a-descriptions-item>
          <a-descriptions-item v-if="detailRun.failure_code" label="失败原因" :span="2">
            <span style="color: var(--ant-color-error)">
              {{ detailRun.failure_code }} — {{ detailRun.failure_message }}
            </span>
          </a-descriptions-item>
        </a-descriptions>
        <a-tabs v-model:activeKey="detailTab">
          <a-tab-pane key="trace" tab="执行轨迹">
            <div class="run-summary">
              <span>事件 {{ events.length }}</span>
              <span>工具调用 {{ toolCalls.length }}</span>
              <span>
                失败
                {{ toolCalls.filter((item: any) => item.exit_code != null && item.exit_code !== 0).length }}
              </span>
            </div>
            <a-empty v-if="events.length === 0 && toolCalls.length === 0" description="暂无执行记录" />
            <div v-else class="execution-trace">
              <div v-for="evt in events" :key="evt.event_id" class="trace-item" :data-tone="eventTone(evt.event_type)">
                <div class="trace-dot"></div>
                <div class="trace-card">
                  <div class="trace-head">
                    <strong>{{ eventTitle(evt) }}</strong>
                    <span>#{{ evt.sequence }}</span>
                  </div>
                  <div class="trace-preview">{{ pretty(evt.payload).slice(0, 180) }}</div>
                  <details>
                    <summary>展开详情</summary>
                    <pre>{{ pretty(evt.payload) }}</pre>
                  </details>
                </div>
              </div>
              <a-collapse v-if="toolCalls.length" ghost class="tool-call-list">
                <a-collapse-panel key="tools" :header="`工具调用明细（${toolCalls.length}）`">
                  <div v-for="call in toolCalls" :key="call.id" class="tool-call-row">
                    <div>
                      <strong>🔧 {{ call.tool_name }}</strong>
                      <a-tag :color="call.exit_code === 0 ? 'success' : 'error'">
                        {{ call.exit_code === 0 ? '成功' : `退出码 ${call.exit_code ?? '-'}` }}
                      </a-tag>
                    </div>
                    <a-space v-if="executionAudit(call).backend" wrap style="margin: 6px 0">
                      <a-tag color="blue">{{ executionAudit(call).backend }}</a-tag>
                      <a-tag :color="executionAudit(call).mount_namespace ? 'green' : 'default'">Mount NS</a-tag>
                      <a-tag :color="executionAudit(call).pid_namespace ? 'green' : 'default'">PID NS</a-tag>
                      <a-tag :color="executionAudit(call).user_namespace ? 'green' : 'default'">User NS</a-tag>
                      <a-tag :color="executionAudit(call).capabilities_dropped ? 'green' : 'red'">Capabilities Drop</a-tag>
                      <a-tag :color="executionAudit(call).seccomp ? 'green' : 'red'">Seccomp-BPF</a-tag>
                      <a-tag color="orange">网络：{{ executionAudit(call).network_policy || '共享宿主网络' }}</a-tag>
                    </a-space>
                    <div v-if="executionAudit(call).workspace_dir" class="trace-preview">
                      workspace: {{ executionAudit(call).workspace_dir }} · artifacts: {{ executionAudit(call).artifact_dir }}
                    </div>
                    <details>
                      <summary>参数与执行信息</summary>
                      <pre>{{ pretty({ params: call.params_summary, audit: call.execution_audit, backend: call.executor_backend, duration_ms: call.duration_ms }) }}</pre>
                    </details>
                  </div>
                </a-collapse-panel>
              </a-collapse>
            </div>
          </a-tab-pane>
          <a-tab-pane key="findings" :tab="`漏洞 (${findings.length})`">
            <a-empty v-if="findings.length === 0" description="暂无漏洞" />
            <a-table
              v-else
              :columns="[
                { title: '标题', dataIndex: 'title', ellipsis: true },
                { title: '等级', dataIndex: 'severity', width: 90 },
                { title: '状态', dataIndex: 'status', width: 90 },
                { title: '目标', dataIndex: 'target', ellipsis: true },
              ]"
              :data-source="findings"
              row-key="id"
              size="small"
              :pagination="{ pageSize: 10 }"
            >
              <template #bodyCell="{ column, record }">
                <template v-if="column.dataIndex === 'severity'">
                  <a-tag :color="severityColor(record.severity)">{{ record.severity }}</a-tag>
                </template>
                <template v-if="column.dataIndex === 'status'">
                  <a-tag>{{ record.status }}</a-tag>
                </template>
              </template>
            </a-table>
          </a-tab-pane>
          <a-tab-pane key="traffic" :tab="`HTTP 流量 (${traffic.length})`">
            <a-empty v-if="traffic.length === 0" description="暂无 HTTP 流量" />
            <div v-else class="traffic-list">
              <HttpExchangeCard v-for="item in traffic" :key="item.id" :data="item" />
            </div>
          </a-tab-pane>
          <a-tab-pane key="plan" :tab="`执行计划 (${plan.nodes?.length || 0})`">
            <a-timeline>
              <a-timeline-item
                v-for="node in plan.nodes || []"
                :key="node.node_id"
                :color="
                  node.status === 'completed'
                    ? 'green'
                    : node.status === 'failed' || node.status === 'blocked'
                      ? 'red'
                      : 'blue'
                "
              >
                <strong>{{ node.objective }}</strong>
                <a-tag class="ml-2">{{ node.status }}</a-tag>
              </a-timeline-item>
            </a-timeline>
          </a-tab-pane>
        </a-tabs>
      </template>
    </a-modal>

    <a-modal v-model:open="createTaskVisible" title="新建任务" @ok="handleCreateTask" width="520px">
      <a-form layout="vertical">
        <a-form-item label="任务名称" required>
          <a-input v-model:value="taskForm.name" placeholder="例如：官网渗透测试" />
        </a-form-item>
        <a-form-item label="所属项目">
          <a-select
            v-model:value="taskForm.project_id"
            allow-clear
            show-search
            option-filter-prop="label"
            placeholder="选择项目（可选）"
            :options="projects.map((p: any) => ({ label: p.name, value: p.id }))"
            style="width: 100%"
          />
        </a-form-item>
        <a-form-item label="目标范围" required>
          <a-input v-model:value="taskForm.target_scope" placeholder='["10.0.0.0/24"]' />
        </a-form-item>
        <a-form-item label="优先级">
          <a-select v-model:value="taskForm.priority">
            <a-select-option value="low">低</a-select-option>
            <a-select-option value="medium">中</a-select-option>
            <a-select-option value="high">高</a-select-option>
            <a-select-option value="critical">紧急</a-select-option>
          </a-select>
        </a-form-item>
        <a-form-item required>
          <a-checkbox v-model:checked="taskForm.authorization_confirmed">
            我确认已获得上述目标的安全测试授权
          </a-checkbox>
        </a-form-item>
      </a-form>
    </a-modal>
    <a-modal v-model:open="createRunVisible" title="创建 Run" @ok="handleCreateRun" width="500px">
      <a-form layout="vertical">
        <a-form-item label="选择任务" required>
          <a-select
            v-model:value="selectedTaskId"
            placeholder="选择一个任务"
            :options="tasks.map((t: any) => ({ value: t.id, label: t.name }))"
            style="width: 100%"
          />
        </a-form-item>
      </a-form>
    </a-modal>
    <a-modal v-model:open="batchVisible" title="批量创建 Run" @ok="handleBatchCreate" width="560px">
      <a-form layout="vertical">
        <a-form-item label="所属项目" required>
          <a-select
            v-model:value="batchForm.project_id"
            show-search
            option-filter-prop="label"
            placeholder="选择项目"
            :options="projects.map((p: any) => ({ label: p.name, value: p.id }))"
            style="width: 100%"
          />
        </a-form-item>
        <a-form-item label="批次名称">
          <a-input v-model:value="batchForm.name" placeholder="如：官网资产批量测试" />
        </a-form-item>
        <a-form-item label="测试目标" required>
          <a-textarea
            v-model:value="batchForm.targetsText"
            :rows="5"
            placeholder="每行一个目标，或逗号分隔，如：&#10;https://example.com&#10;192.168.1.10"
          />
          <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-top: 4px">
            每个目标会创建一个独立 Run，共用同一套执行与审批链路，并发受运行策略配额限制。
          </div>
        </a-form-item>
        <a-form-item label="测试目标说明">
          <a-input v-model:value="batchForm.objective" placeholder="如：授权安全测试" />
        </a-form-item>
        <a-form-item label="优先级">
          <a-select v-model:value="batchForm.priority">
            <a-select-option value="low">低</a-select-option>
            <a-select-option value="medium">中</a-select-option>
            <a-select-option value="high">高</a-select-option>
            <a-select-option value="critical">紧急</a-select-option>
          </a-select>
        </a-form-item>
      </a-form>
    </a-modal>
  </div>
</template>

<style scoped>
.run-summary {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  margin-bottom: 16px;
}
.run-summary span {
  padding: 5px 10px;
  color: var(--ant-color-text-secondary);
  font-size: 12px;
  background: var(--ant-color-fill-quaternary);
  border-radius: 999px;
}
.execution-trace {
  position: relative;
  padding-left: 18px;
}
.execution-trace::before {
  position: absolute;
  top: 8px;
  bottom: 8px;
  left: 5px;
  width: 1px;
  background: var(--ant-color-border-secondary);
  content: '';
}
.trace-item {
  position: relative;
  margin-bottom: 12px;
}
.trace-dot {
  position: absolute;
  top: 14px;
  left: -17px;
  width: 9px;
  height: 9px;
  background: var(--ant-color-primary);
  border: 2px solid var(--ant-color-bg-container);
  border-radius: 50%;
}
.trace-item[data-tone='success'] .trace-dot {
  background: var(--ant-color-success);
}
.trace-item[data-tone='error'] .trace-dot {
  background: var(--ant-color-error);
}
.trace-item[data-tone='tool'] .trace-dot {
  background: var(--ant-color-warning);
}
.trace-card {
  padding: 10px 12px;
  background: var(--ant-color-fill-quaternary);
  border: 1px solid var(--ant-color-border-secondary);
  border-radius: 8px;
}
.trace-head {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}
.trace-head span {
  color: var(--ant-color-text-tertiary);
  font-size: 11px;
}
.trace-preview {
  margin: 5px 0;
  overflow: hidden;
  color: var(--ant-color-text-secondary);
  font-size: 12px;
  white-space: nowrap;
  text-overflow: ellipsis;
}
details summary {
  width: fit-content;
  color: var(--ant-color-primary);
  font-size: 12px;
  cursor: pointer;
}
details pre {
  max-height: 360px;
  margin: 8px 0 0;
  padding: 10px;
  overflow: auto;
  color: var(--ant-color-text);
  font-size: 11px;
  white-space: pre-wrap;
  background: var(--ant-color-bg-container);
  border-radius: 6px;
}
.tool-call-list {
  margin-top: 12px;
}
.tool-call-row {
  padding: 10px 0;
  border-bottom: 1px solid var(--ant-color-border-secondary);
}
.tool-call-row strong {
  margin-right: 8px;
}
.traffic-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-height: 620px;
  overflow: auto;
  padding-right: 4px;
}
</style>
