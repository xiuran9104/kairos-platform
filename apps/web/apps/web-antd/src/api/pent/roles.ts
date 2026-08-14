import { requestClient } from '#/api/request';

export interface PentAgentRole {
  created_at: string;
  description: string;
  enabled: boolean;
  hitl_policy: Record<string, any>;
  icon: string;
  id: string;
  inherited?: boolean;
  is_default: boolean;
  name: string;
  tenant_id: number;
  tool_whitelist: string[];
  updated_at: string;
  user_prompt: string;
}

export function getPentRoles() {
  return requestClient.get<PentAgentRole[]>('/pent/roles');
}

export function getPentRole(id: string) {
  return requestClient.get<PentAgentRole>(`/pent/roles/${id}`);
}

export function createPentRole(data: {
  description?: string;
  enabled?: boolean;
  hitl_policy?: Record<string, any>;
  icon?: string;
  name: string;
  tool_whitelist?: string[];
  user_prompt?: string;
}) {
  return requestClient.post<PentAgentRole>('/pent/roles', data);
}

export function updatePentRole(
  id: string,
  data: {
    description?: string;
    enabled?: boolean;
    hitl_policy?: Record<string, any>;
    icon?: string;
    name: string;
    tool_whitelist?: string[];
    user_prompt?: string;
  },
) {
  return requestClient.put<PentAgentRole>(`/pent/roles/${id}`, data);
}

export function updatePentRoleStatus(id: string, enabled: boolean) {
  return requestClient.put<PentAgentRole>(`/pent/roles/${id}/status`, { enabled });
}

export function deletePentRole(id: string) {
  return requestClient.delete(`/pent/roles/${id}`);
}
