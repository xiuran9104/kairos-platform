import { requestClient } from '#/api/request';

export interface PentAgent {
  id: string;
  name: string;
  description: string;
  instruction: string;
}

export function getPentAgents() {
  return requestClient.get<{ agents: PentAgent[]; dir: string }>('/pent/agents');
}

export function getPentAgent(id: string) {
  return requestClient.get<PentAgent>(`/pent/agents/${id}`);
}

export function createPentAgent(data: {
  description: string;
  id: string;
  instruction: string;
  name: string;
}) {
  return requestClient.post('/pent/agents', data);
}

export function updatePentAgent(
  id: string,
  data: { description: string; instruction: string; name: string },
) {
  return requestClient.put(`/pent/agents/${id}`, data);
}

export function deletePentAgent(id: string) {
  return requestClient.delete(`/pent/agents/${id}`);
}
