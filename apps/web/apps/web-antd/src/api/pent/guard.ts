import { requestClient } from '#/api/request';

export function getPentGuardRules() {
  return requestClient.get('/pent/guard/rules');
}

export function getPentInterceptions(params?: Record<string, any>) {
  return requestClient.get('/pent/guard/interceptions', { params });
}

export function updatePentGuardRule(id: string, data: Record<string, any>) {
  return requestClient.put(`/pent/guard/rules/${id}`, data);
}
