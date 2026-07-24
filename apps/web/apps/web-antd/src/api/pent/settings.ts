import { requestClient } from '#/api/request';

export function getPentModelRoleBindings() {
  return requestClient.get('/pent/settings/model-role-bindings');
}

export function updatePentModelRoleBindings(data: Record<string, any>) {
  return requestClient.put('/pent/settings/model-role-bindings', data);
}

export function getPentRuntimePolicy() {
  return requestClient.get('/pent/settings/runtime-policy');
}

export function updatePentRuntimePolicy(data: Record<string, any>) {
  return requestClient.put('/pent/settings/runtime-policy', data);
}
