import { requestClient } from '#/api/request';

export function getPentToolOverview() {
  return requestClient.get('/pent/tools/overview');
}

export function getPentToolHealth(params?: Record<string, any>) {
  return requestClient.get('/pent/tools/health', { params });
}

export function runPentToolHealthCheck(toolId: string, version: string) {
  return requestClient.post(
    `/pent/tools/${toolId}/versions/${version}/health-check`,
  );
}

export function enablePentTool(toolId: string, version: string) {
  return requestClient.post(
    `/pent/tools/${toolId}/versions/${version}/enable`,
  );
}

export function disablePentTool(toolId: string, version: string) {
  return requestClient.post(
    `/pent/tools/${toolId}/versions/${version}/disable`,
  );
}
