import { requestClient } from '#/api/request';

export function getRmosMemories(params?: Record<string, any>) {
  return requestClient.get('/pent/rmos/memories', { params });
}

export function getRmosAttackGraph(params?: Record<string, any>) {
  return requestClient.get('/pent/rmos/attack-graph', { params });
}

export function submitRmosFeedback(
  memoryId: string,
  data: { feedback: 'useful' | 'useless'; reason?: string },
) {
  return requestClient.post(`/pent/rmos/memories/${memoryId}/feedback`, data);
}
