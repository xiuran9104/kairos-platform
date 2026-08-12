import { requestClient } from '#/api/request';

export interface PentTrafficQuery {
  duration_max_ms?: number;
  method?: string;
  page?: number;
  page_size?: number;
  source?: string;
  status_code?: number;
  url?: string;
}

export function getPentTrafficPage(params: PentTrafficQuery) {
  return requestClient.get('/pent/traffic', { params });
}

export function getPentTrafficDetail(id: string) {
  return requestClient.get(`/pent/traffic/${id}`);
}

export function getPentRunTraffic(runId: string) {
  return requestClient.get(`/pent/runs/${runId}/traffic`);
}
