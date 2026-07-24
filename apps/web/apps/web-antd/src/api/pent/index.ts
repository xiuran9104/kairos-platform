import { requestClient } from '#/api/request';

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
export function getPentRunEvidence(runId: string) { return requestClient.get(`/pent/runs/${runId}/evidence`); }
export function getPentRunFindings(runId: string) { return requestClient.get(`/pent/runs/${runId}/findings`); }
export function getPentApprovals() { return requestClient.get('/pent/approvals'); }
export function decidePentApproval(id: string, data: any) { return requestClient.post(`/pent/approvals/${id}/decision`, data); }
export function getPentTools() { return requestClient.get('/pent/tools'); }
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
