import { requestClient } from '#/api/request';

// ── Task-based API (replaces chat dialog) ──────────────────────────

export function createPentTaskApi(data: {
  name: string;
  description?: string;
  project_id?: string;
  targets: string[];
  priority?: string;
  orchestration?: 'deep' | 'plan_execute' | 'supervisor';
  hitl?: {
    enabled?: boolean;
    mode?: 'approval' | 'review_edit';
    reviewer?: 'audit_agent' | 'human';
    tool_whitelist?: string[];
  };
}) {
  return requestClient.post('/pent/tasks', data, { timeout: 30_000 });
}

export function getPentTaskApi(id: string) {
  return requestClient.get(`/pent/tasks/${id}`);
}

export function cancelPentTaskApi(id: string) {
  return requestClient.post(`/pent/tasks/${id}/cancel`);
}

// ── Turn management (used by task monitoring) ──────────────────────

export function cancelTurn(turnId: string) {
  return requestClient.post(`/pent/turns/${turnId}/cancel`);
}
