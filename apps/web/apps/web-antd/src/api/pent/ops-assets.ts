import { requestClient } from '#/api/request';

export interface PentAsset {
  id: string;
  tenant_id: number;
  project_id?: string;
  target: string;
  host?: string;
  ip?: string;
  domain?: string;
  port?: number;
  protocol?: string;
  title?: string;
  server?: string;
  source: string;
  tags: string[];
  risk_level: string;
  status: string;
  responsible_person?: string;
  department?: string;
  business_system?: string;
  environment?: string;
  criticality?: string;
  country?: string;
  province?: string;
  city?: string;
  first_seen?: string;
  last_seen?: string;
  last_scan_at?: string;
  created_at?: string;
  updated_at?: string;
}

export interface PentAssetListResult {
  assets: PentAsset[];
  total: number;
  total_pages: number;
  page: number;
}

export interface PentAssetStats {
  total: number;
  ips: number;
  domains: number;
  ports: number;
  recent: number;
  protocols: Array<{ protocol: string; count: number }>;
  asset_trend: Array<{ date: string; added: number }>;
  coverage: {
    scanned: number;
    scanned_30d: number;
    never_scanned: number;
    overdue_30d: number;
  };
}

export function getPentAssets(params?: Record<string, any>) {
  return requestClient.get<PentAssetListResult>('/pent/assets', { params });
}

export function importPentAssets(data: {
  assets: Array<Record<string, any>>;
  source?: string;
}) {
  return requestClient.post<{ created: number; updated: number; skipped: number }>(
    '/pent/assets/import',
    data,
  );
}

export function getPentAsset(id: string) {
  return requestClient.get<PentAsset>(`/pent/assets/${id}`);
}

export function updatePentAsset(id: string, data: Record<string, any>) {
  return requestClient.put<PentAsset>(`/pent/assets/${id}`, data);
}

export function deletePentAsset(id: string) {
  return requestClient.delete(`/pent/assets/${id}`);
}

export function bindPentAssetsProject(assetIds: string[], projectId: string) {
  return requestClient.post('/pent/assets/project-binding', {
    asset_ids: assetIds,
    project_id: projectId,
  });
}

export function bulkUpdatePentAssets(data: {
  asset_ids: string[];
  patch: Record<string, any>;
  add_tags?: string[];
  remove_tags?: string[];
}) {
  return requestClient.put('/pent/assets/bulk', data);
}

export function batchDeletePentAssets(assetIds: string[]) {
  return requestClient.post('/pent/assets/batch-delete', { asset_ids: assetIds });
}

export function mergePentAssets(data: {
  asset_ids: string[];
  primary_id: string;
}) {
  return requestClient.post('/pent/assets/merge', data);
}

export function getPentAssetStats(days = 7) {
  return requestClient.get<PentAssetStats>('/pent/assets/stats', {
    params: { days },
  });
}
