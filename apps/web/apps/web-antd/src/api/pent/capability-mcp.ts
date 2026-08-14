import { requestClient } from '#/api/request';

// ── MCP monitor (tool call executions) ─────────────────────────────────────

export interface PentMonitorExecution {
  id: string;
  run_id?: string;
  tool_name: string;
  status: 'completed' | 'failed' | 'running' | string;
  started_at?: string;
  completed_at?: string;
  duration_ms?: number;
  exit_code?: number;
  executor_backend?: string;
  params_summary?: string;
}

export interface PentMonitorSummary {
  total_calls: number;
  success_calls: number;
  failed_calls: number;
  last_call_time?: string;
  tool_count: number;
}

export interface PentMonitorTopTool {
  tool_name: string;
  calls: number;
  successes: number;
  failures: number;
}

export interface PentMonitorResponse {
  executions: PentMonitorExecution[];
  summary: PentMonitorSummary;
  top_tools: PentMonitorTopTool[];
  page: number;
  page_size: number;
  total: number;
  total_pages: number;
}

export interface PentMonitorEvidence {
  id: string;
  content_type: string;
  source: string;
}

export function getPentMonitor(params?: {
  page?: number;
  page_size?: number;
  status?: string;
  tool?: string;
}) {
  return requestClient.get<PentMonitorResponse>('/pent/monitor', { params });
}

export interface PentMonitorTimelineBucket {
  ts: string;
  total: number;
  success: number;
  failed: number;
}

export function getPentMonitorTimeline(range: '24h' | '30d' | '7d') {
  return requestClient.get<{ buckets: PentMonitorTimelineBucket[] }>(
    '/pent/monitor/calls-timeline',
    { params: { range } },
  );
}

export function getPentMonitorExecution(id: string) {
  return requestClient.get<{
    evidence: PentMonitorEvidence[];
    execution: PentMonitorExecution;
  }>(`/pent/monitor/execution/${id}`);
}

export function deletePentMonitorExecution(id: string) {
  return requestClient.delete(`/pent/monitor/execution/${id}`);
}

export function deletePentMonitorExecutions(ids: string[]) {
  return requestClient.delete('/pent/monitor/executions', { data: { ids } });
}

// ── External MCP servers ───────────────────────────────────────────────────

export interface PentMcpServer {
  id: string;
  name: string;
  transport: 'http' | 'sse' | 'stdio';
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  url?: string;
  headers?: Record<string, string>;
  enabled: boolean;
  timeout_secs?: number;
  description?: string;
  status?: 'connected' | 'connecting' | 'disabled' | 'disconnected' | 'error' | string;
  tool_count?: number;
  error?: string;
  created_at?: string;
  updated_at?: string;
}

export interface PentMcpServerStats {
  total: number;
  enabled: number;
  disabled: number;
  connected: number;
}

export function getPentMcpServers() {
  return requestClient.get<{
    servers: PentMcpServer[];
    stats: PentMcpServerStats;
  }>('/pent/mcp/servers');
}

export function createPentMcpServer(data: Partial<PentMcpServer>) {
  return requestClient.post('/pent/mcp/servers', data);
}

export function updatePentMcpServer(id: string, data: Partial<PentMcpServer>) {
  return requestClient.put(`/pent/mcp/servers/${id}`, data);
}

export function deletePentMcpServer(id: string) {
  return requestClient.delete(`/pent/mcp/servers/${id}`);
}

export function startPentMcpServer(id: string) {
  return requestClient.post(`/pent/mcp/servers/${id}/start`);
}

export function stopPentMcpServer(id: string) {
  return requestClient.post(`/pent/mcp/servers/${id}/stop`);
}
