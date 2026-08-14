import { requestClient } from '#/api/request';

export interface HitlSettings {
  approval_ttl_seconds: number;
  hitl_audit_model_id?: number;
  hitl_audit_system_prompt: string;
  hitl_audit_user_prompt: string;
  hitl_enabled: boolean;
  hitl_mode: 'approval' | 'review_edit';
  hitl_reviewer: 'audit_agent' | 'human';
  hitl_tool_whitelist: string[];
  retention_days: number;
  inherited?: boolean;
}

export function getHitlSettings() {
  return requestClient.get<HitlSettings>('/pent/hitl/settings');
}
export function updateHitlSettings(data: HitlSettings) {
  return requestClient.put('/pent/hitl/settings', data);
}
export function getHitlApprovals(params?: { status?: string }) {
  return requestClient.get<any[]>('/pent/hitl/approvals', { params });
}
export function decideHitlApproval(
  id: string,
  data: { arguments?: Record<string, any>; comment?: string; decision: 'approve' | 'reject' },
) {
  return requestClient.post(`/pent/hitl/approvals/${id}/decision`, data);
}
