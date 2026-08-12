import { requestClient } from '#/api/request';

export function getPentRuntimePolicy() {
  return requestClient.get('/pent/settings/runtime-policy');
}

export function updatePentRuntimePolicy(data: Record<string, any>) {
  return requestClient.put('/pent/settings/runtime-policy', data);
}
