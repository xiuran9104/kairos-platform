import { onMounted, onUnmounted, ref } from 'vue';
import {
  getPentApprovals,
  getPentProjects,
  getPentRuns,
  getPentSessions,
  getPentTasks,
  getPentToolOverview,
} from '#/api/pent';

export interface DashboardMetrics {
  totalTasks: number;
  totalFindings: number;
  totalAssets: number;
  totalToolCalls: number;
  totalLlmCalls: number;
  totalTokens: number;
}

export interface FindingSeverity {
  critical: number;
  high: number;
  medium: number;
  low: number;
  info: number;
}

export interface ActivityItem {
  type: string;
  text: string;
  time: string;
  color: string;
}

export function useDashboard() {
  const loading = ref(false);
  const error = ref<string | null>(null);

  const projects = ref<any[]>([]);
  const tasks = ref<any[]>([]);
  const runs = ref<any[]>([]);
  const approvals = ref<any[]>([]);
  const sessions = ref<any[]>([]);
  const toolOverview = ref<any>({});

  const activity = ref<ActivityItem[]>([]);
  let refreshTimer: ReturnType<typeof setInterval> | undefined;

  const metrics = ref<DashboardMetrics>({
    totalTasks: 0,
    totalFindings: 0,
    totalAssets: 0,
    totalToolCalls: 0,
    totalLlmCalls: 0,
    totalTokens: 0,
  });

  const runningCount = () => runs.value.filter((r: any) => r.status === 'running').length;
  const queuedCount = () => runs.value.filter((r: any) => r.status === 'queued').length;
  const failedCount = () => runs.value.filter((r: any) => r.status === 'failed').length;
  const completedCount = () => runs.value.filter((r: any) => r.status === 'completed').length;
  const pendingApprovals = () =>
    approvals.value.filter((a: any) => a.status === 'pending' || a.status === 'requested').length;

  // Compute finding severity distribution from run events
  const findingSeverity = ref<FindingSeverity>({
    critical: 0, high: 0, medium: 0, low: 0, info: 0,
  });

  function buildActivity(): ActivityItem[] {
    const items: ActivityItem[] = [];
    sessions.value.slice(0, 3).forEach((s: any) => {
      items.push({
        type: 'session',
        text: `Agent Session ${s.id?.slice(0, 12)} ${s.status}`,
        time: s.created_at,
        color: 'blue',
      });
    });
    runs.value.slice(0, 5).forEach((r: any) => {
      items.push({
        type: 'run',
        text: `Run ${r.id?.slice(0, 12) || 'unknown'} → ${r.status}`,
        time: r.created_at || r.started_at,
        color: r.status === 'failed' ? 'red' : r.status === 'completed' ? 'green' : 'blue',
      });
    });
    approvals.value
      .filter((a: any) => a.status === 'pending')
      .slice(0, 2)
      .forEach((a: any) => {
        items.push({
          type: 'approval',
          text: `审批 ${a.id?.slice(0, 12)} 等待决策`,
          time: a.created_at,
          color: 'orange',
        });
      });
    items.sort((a, b) => new Date(b.time || 0).getTime() - new Date(a.time || 0).getTime());
    return items.slice(0, 10);
  }

  async function fetchAll() {
    loading.value = true;
    error.value = null;
    try {
      const [p, t, r, a, s, to] = await Promise.all([
        getPentProjects().catch(() => []),
        getPentTasks().catch(() => []),
        getPentRuns().catch(() => []),
        getPentApprovals().catch(() => []),
        getPentSessions().catch(() => []),
        getPentToolOverview().catch(() => ({})),
      ]);

      projects.value = (p as any[]) || [];
      tasks.value = (t as any[]) || [];
      runs.value = (r as any[]) || [];
      approvals.value = (a as any[]) || [];
      sessions.value = (s as any[]) || [];
      toolOverview.value = to || {};

      // Compute metrics
      metrics.value = {
        totalTasks: tasks.value.length,
        totalFindings: 0, // Will be populated when aggregation API exists
        totalAssets: projects.value.length,
        totalToolCalls: toolOverview.value?.total_calls ?? 0,
        totalLlmCalls: 0, // Will be populated when LLM usage API exists
        totalTokens: 0, // Will be populated when token tracking API exists
      };

      activity.value = buildActivity();
      findingSeverity.value = { critical: 0, high: 0, medium: 0, low: 0, info: 0 };
    } catch (e: any) {
      error.value = e?.message || 'Pent 概览加载失败';
    } finally {
      loading.value = false;
    }
  }

  function startAutoRefresh(intervalMs = 30000) {
    stopAutoRefresh();
    refreshTimer = setInterval(fetchAll, intervalMs);
  }

  function stopAutoRefresh() {
    if (refreshTimer) {
      clearInterval(refreshTimer);
      refreshTimer = undefined;
    }
  }

  onMounted(() => {
    fetchAll();
    startAutoRefresh();
  });

  onUnmounted(stopAutoRefresh);

  return {
    loading,
    error,
    projects,
    tasks,
    runs,
    approvals,
    sessions,
    toolOverview,
    metrics,
    activity,
    findingSeverity,
    runningCount,
    queuedCount,
    failedCount,
    completedCount,
    pendingApprovals,
    fetchAll,
  };
}
