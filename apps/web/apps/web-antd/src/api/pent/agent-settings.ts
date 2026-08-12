import { requestClient } from '#/api/request';
export function getPentAgentSettings() { return requestClient.get<Record<string, any>>('/pent/agent/settings'); }
export function updatePentAgentSettings(data: Record<string, any>) { return requestClient.put('/pent/agent/settings', data); }
