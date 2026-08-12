<script lang="ts" setup>
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { message } from 'ant-design-vue';
import { requestClient } from '#/api/request';
import { streamPentRunEvents } from '#/api/pent';

defineOptions({ name: 'PentAgentSession' });

const route = useRoute();
const router = useRouter();
const taskId = computed(() => route.params.id as string | undefined);

interface TaskInfo {
  id: string;
  name: string;
  status: string;
  target_scope?: { targets?: string[] };
  priority?: string;
  latest_run?: {
    id: string;
    status: string;
    objective_snapshot?: string;
    started_at?: string;
    completed_at?: string;
  };
}

// ── Task info ──────────────────────────────────────────────────────
const task = ref<TaskInfo | null>(null);
const loading = ref(true);
const events = ref<any[]>([]);
const activeRunStream = ref<AbortController>();
const pollTimer = ref<ReturnType<typeof setInterval>>();

async function loadTask() {
  if (!taskId.value) return;
  loading.value = true;
  try {
    const resp: any = await requestClient.get(`/api/tasks/${taskId.value}`);
    task.value = resp?.data ?? resp;
    if (task.value?.latest_run?.status === 'running' || task.value?.status === 'running') {
      startRunStream();
    }
  } catch {
    message.error('加载任务信息失败');
  } finally {
    loading.value = false;
  }
}

function startRunStream() {
  stopRunStream();
  const runId = task.value?.latest_run?.id;
  if (!runId) return;
  activeRunStream.value = new AbortController();
  streamPentRunEvents(
    runId,
    activeRunStream.value,
    (event: any) => {
      try {
        const data = JSON.parse(event.data || '{}');
        events.value.push({
          type: event.event || data.event_type || 'event',
          payload: data.payload || data,
          timestamp: new Date().toISOString(),
        });
        // Update run status
        if (event.event === 'run.completed') {
          if (task.value?.latest_run) task.value.latest_run.status = 'completed';
          stopRunStream();
        } else if (event.event === 'run.failed') {
          if (task.value?.latest_run) task.value.latest_run.status = 'failed';
          stopRunStream();
        }
      } catch { /* ignore parse errors */ }
    },
    () => { /* reconnect */ },
    () => { /* closed */ },
  );
}

function stopRunStream() {
  activeRunStream.value?.abort();
  activeRunStream.value = undefined;
}

async function cancelTask() {
  if (!taskId.value) return;
  try {
    await requestClient.post(`/api/tasks/${taskId.value}/cancel`);
    message.success('任务已取消');
    if (task.value) task.value.status = 'cancelled';
    stopRunStream();
  } catch (e: any) {
    message.error(e?.message || '取消失败');
  }
}

// Auto-refresh task info every 10s while running
function startPolling() {
  stopPolling();
  pollTimer.value = setInterval(async () => {
    if (!taskId.value) return;
    try {
      const resp: any = await requestClient.get(`/api/tasks/${taskId.value}`);
      const data = resp?.data ?? resp;
      if (data) {
        task.value = data;
        if (data.latest_run?.status !== 'running' && data.status !== 'running') {
          stopRunStream();
          stopPolling();
        }
      }
    } catch { /* silent */ }
  }, 10_000);
}

function stopPolling() {
  if (pollTimer.value) { clearInterval(pollTimer.value); pollTimer.value = undefined; }
}

onMounted(async () => {
  await loadTask();
  if (task.value?.status === 'running') startPolling();
});

onUnmounted(() => {
  stopRunStream();
  stopPolling();
});

const statusColor: Record<string, string> = {
  running: 'green',
  completed: 'blue',
  failed: 'red',
  cancelled: 'orange',
  queued: 'gold',
};
const statusLabel: Record<string, string> = {
  running: '运行中',
  completed: '已完成',
  failed: '失败',
  cancelled: '已取消',
  queued: '排队中',
};

// Poll task events for non-SSE view
const turns = ref<any[]>([]);
async function refreshTurns() {
  if (!taskId.value) return;
  try {
    const resp: any = await requestClient.get(`/api/tasks/${taskId.value}`);
    const data = resp?.data ?? resp;
    if (data?.latest_run?.id) {
      const tid = data.latest_run.id;
      const msgsResp: any = await requestClient.get(`/pent/runs/${tid}/events`);
      turns.value = Array.isArray(msgsResp?.data) ? msgsResp.data : (msgsResp?.data ? [msgsResp.data] : []);
    }
  } catch { /* silent */ }
}
watch(taskId, () => { if (taskId.value) loadTask(); });
</script>

<template>
  <div style="max-width: 1200px; margin: 0 auto; padding: 16px; height: calc(100vh - 60px); display: flex; flex-direction: column;">
    <!-- Header -->
    <a-card :bordered="false" size="small" style="margin-bottom: 8px;">
      <template #title>
        <a-space>
          <span style="font-size: 16px; font-weight: 600">
            {{ task?.name || '加载中…' }}
          </span>
          <a-tag v-if="task?.status" :color="statusColor[task.status] || 'default'">
            {{ statusLabel[task.status] || task.status }}
          </a-tag>
          <a-tag v-if="task?.priority && task.priority !== 'medium'" :color="task.priority === 'critical' ? 'red' : task.priority === 'high' ? 'orange' : 'blue'">
            {{ task.priority === 'critical' ? '紧急' : task.priority === 'high' ? '高' : '低' }}
          </a-tag>
        </a-space>
      </template>
      <template #extra>
        <a-space>
          <a-button size="small" @click="refreshTurns">刷新</a-button>
          <a-button
            v-if="task?.status === 'running'"
            size="small"
            danger
            @click="cancelTask"
          >
            取消任务
          </a-button>
          <a-button size="small" @click="router.push('/pent/agent')">
            新建任务
          </a-button>
        </a-space>
      </template>
      <a-descriptions size="small" :column="4">
        <a-descriptions-item label="任务 ID">{{ task?.id?.slice(0, 12) || '-' }}</a-descriptions-item>
        <a-descriptions-item label="执行状态">
          <a-tag :color="statusColor[task?.latest_run?.status || ''] || 'default'" size="small">
            {{ statusLabel[task?.latest_run?.status || ''] || task?.latest_run?.status || '等待中' }}
          </a-tag>
        </a-descriptions-item>
        <a-descriptions-item label="目标数">{{ task?.target_scope?.targets?.length || '-' }}</a-descriptions-item>
        <a-descriptions-item label="启动时间">{{ task?.latest_run?.started_at ? new Date(task.latest_run.started_at).toLocaleString() : '-' }}</a-descriptions-item>
      </a-descriptions>
      <a-space v-if="task?.target_scope?.targets" wrap style="margin-top: 8px;">
        <a-tag v-for="t in task.target_scope.targets" :key="t" color="blue" size="small">{{ t }}</a-tag>
      </a-space>
    </a-card>

    <!-- Targets and progress -->
    <div style="display: flex; gap: 8px; flex: 1; min-height: 0;">
      <!-- Run events -->
      <div style="flex: 1; overflow-y: auto; background: var(--ant-color-bg-container); border-radius: 8px; padding: 16px;">
        <a-spin v-if="loading" style="display: block; margin: 40px auto;" />
        <a-empty
          v-else-if="turns.length === 0 && task?.status !== 'running'"
          description="暂无执行事件"
          style="margin-top: 40px;"
        />
        <div v-else>
          <div v-for="(ev, idx) in turns" :key="idx" style="margin-bottom: 12px; padding: 8px 12px; border-radius: 6px; background: var(--ant-color-fill-quaternary);">
            <div style="font-size: 12px; color: #999; margin-bottom: 4px;">
              {{ ev.event_type || ev.type }} · {{ ev.occurred_at ? new Date(ev.occurred_at).toLocaleTimeString() : (ev.created_at ? new Date(ev.created_at).toLocaleTimeString() : '') }}
            </div>
            <div style="font-size: 13px; white-space: pre-wrap; word-break: break-all;">
              {{ typeof ev.payload === 'string' ? ev.payload : JSON.stringify(ev.payload, null, 2).slice(0, 1000) }}
            </div>
          </div>
          <div v-if="task?.status === 'running'" style="text-align: center; color: #999; padding: 20px;">
            <a-spin size="small" /> Agent 正在自主执行…
          </div>
        </div>
      </div>
    </div>

    <!-- Quick action: download report when completed -->
    <div v-if="task?.status === 'completed' && task?.latest_run?.id" style="margin-top: 8px; text-align: center;">
      <a-space>
        <a-button type="primary" @click="router.push(`/pent/runs?id=${task.latest_run?.id}`)">
          查看运行详情
        </a-button>
        <a-button @click="router.push('/pent/findings')">
          查看漏洞发现
        </a-button>
      </a-space>
    </div>
  </div>
</template>
