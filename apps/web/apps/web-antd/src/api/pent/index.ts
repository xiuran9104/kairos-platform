import { requestClient } from '#/api/request';
import { fetchEventSource } from '@vben/request';
import { useAccessStore } from '@vben/stores';
import { useAppConfig } from '@vben/hooks';

export * from './assets';
export * from './guard';
export * from './rmos';
export * from './settings';
export * from './traffic';
export * from './tools';
export * from './workspace';

export interface PentAgentSession {
  id: string;
  project_id?: string;
  objective: string;
  constraints: Record<string, any>;
  budget: Record<string, any>;
  scope_proposal: {
    targets?: string[];
    constraints?: Record<string, any>;
    confirmed?: boolean;
  };
  confirmed_scope_snapshot?: {
    targets?: string[];
    constraints?: Record<string, any>;
  };
  status: string;
  active_run_id?: string;
  created_at: string;
}

export interface PentConversationMessage {
  id: string;
  session_id: string;
  role: 'assistant' | 'system' | 'user';
  message_type: string;
  content: string;
  payload: Record<string, any>;
  created_at: string;
}

export interface CreatePentSessionInput {
  project_id?: string;
  objective: string;
  targets: string[];
  constraints: Record<string, any>;
  budget: Record<string, any>;
}

export function getPentProjects() { return requestClient.get('/pent/projects'); }
export function createPentProject(data: any) { return requestClient.post('/pent/projects', data); }
export function getPentTasks() { return requestClient.get('/pent/tasks'); }
export function createPentTask(data: any) { return requestClient.post('/pent/tasks', data); }
export function cancelPentTask(id: string) { return requestClient.post(`/pent/tasks/${id}/cancel`); }
export function deletePentTask(id: string) { return requestClient.delete(`/pent/tasks/${id}`); }
export function getPentRuns() { return requestClient.get('/pent/runs'); }
export function createPentRun(data: any) { return requestClient.post('/pent/runs', data); }
export function cancelPentRun(id: string) { return requestClient.post(`/pent/runs/${id}/cancel`); }
export function getPentRunEvents(runId: string) { return requestClient.get(`/pent/runs/${runId}/events`); }

/** Stream run events via SSE with proper Authorization header (no token in URL). */
export function streamPentRunEvents(
  runId: string,
  ctrl: AbortController,
  onMessage: (event: any) => void,
  onError: (err: any) => void,
  onClose: () => void,
) {
  const { apiURL } = useAppConfig(import.meta.env, import.meta.env.PROD);
  const accessStore = useAccessStore();
  const token = accessStore.accessToken;
  return fetchEventSource(`${apiURL}/pent/runs/${runId}/events/stream`, {
    method: 'get',
    headers: { Authorization: `Bearer ${token}` },
    openWhenHidden: true,
    onmessage: onMessage,
    onerror: onError,
    onclose: onClose,
    signal: ctrl.signal,
  });
}

export function getPentRunEvidence(runId: string) { return requestClient.get(`/pent/runs/${runId}/evidence`); }
export function getPentRunFindings(runId: string) { return requestClient.get(`/pent/runs/${runId}/findings`); }
export function getPentApprovals() { return requestClient.get('/pent/approvals'); }
export function decidePentApproval(id: string, data: any) { return requestClient.post(`/pent/approvals/${id}/decision`, data); }
export function getPentTools() { return requestClient.get('/pent/tools'); }

// Skill management
export interface PentSkill {
  id: string;
  name: string;
  title: string;
  description: string;
  category: string;
  risk_level: string;
  status: string;
  version: string;
  revision: number;
  created_at: string;
  updated_at: string;
  content?: string;
  manifest?: Record<string, any>;
}
export function getPentSkills(params?: { status?: string; category?: string; query?: string }) {
  return requestClient.get<PentSkill[]>('/pent/skills', { params });
}
export function getPentSkill(id: string) {
  return requestClient.get<PentSkill>(`/pent/skills/${id}`);
}
export function createPentSkill(data: { name: string; title: string; content: string; description?: string; category?: string; risk_level?: string }) {
  return requestClient.post<PentSkill>('/pent/skills', data);
}
export function updatePentSkillStatus(id: string, status: string) {
  return requestClient.put<PentSkill>(`/pent/skills/${id}/status`, { status });
}
export function getPentSessions() {
  return requestClient.get<PentAgentSession[]>('/pent/sessions');
}
export function getPentSession(id: string) {
  return requestClient.get<PentAgentSession>(`/pent/sessions/${id}`);
}
export function deletePentSession(id: string) {
  return requestClient.delete(`/pent/sessions/${id}`);
}
export function createPentSession(data: CreatePentSessionInput) {
  return requestClient.post<PentAgentSession>('/pent/sessions', data);
}
export function getPentSessionMessages(id: string) {
  return requestClient.get<PentConversationMessage[]>(`/pent/sessions/${id}/messages`);
}
export function sendPentSessionMessage(
  id: string,
  data: { content: string; message_type?: string; payload?: Record<string, any> },
) {
  return requestClient.post(`/pent/sessions/${id}/messages`, data);
}
export function confirmPentSessionScope(
  id: string,
  data: { targets: string[]; constraints: Record<string, any> },
) {
  return requestClient.post(`/pent/sessions/${id}/scope/confirm`, data);
}
export function startPentSessionRun(
  id: string,
  data: { name?: string; priority?: string } = {},
) {
  return requestClient.post<{ run_id: string; session_id: string; status: string }>(
    `/pent/sessions/${id}/runs`,
    data,
  );
}
export function sendPentRunInstruction(
  runId: string,
  data: {
    instruction_type: string;
    content: string;
    payload?: Record<string, any>;
  },
) {
  return requestClient.post(`/pent/runs/${runId}/instructions`, data);
}
