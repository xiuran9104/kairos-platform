import { requestClient } from '#/api/request';

export interface PentTrafficQuery {
  duration_max_ms?: number;
  method?: string;
  page?: number;
  page_size?: number;
  project_id?: string;
  run_id?: string;
  source?: string;
  status_code?: number;
  url?: string;
}

export interface PentTrafficPage {
  items: any[];
  page: number;
  page_size: number;
  total: number;
}

export function getPentTrafficPage(params: PentTrafficQuery) {
  return requestClient.get<PentTrafficPage>('/pent/traffic', { params });
}

export function getPentTrafficDetail(id: string) {
  return requestClient.get<any>(`/pent/traffic/${id}`);
}

export function getPentRunTraffic(runId: string) {
  return requestClient.get(`/pent/runs/${runId}/traffic`);
}
