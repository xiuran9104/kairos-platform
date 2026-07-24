import { requestClient } from '#/api/request';

export interface PentAssetProfile {
  asset_id: string;
  attack_paths: number;
  business_tags: string[];
  endpoint_count: number;
  finding_count: number;
  fingerprints: Array<{ category: string; name: string; version?: string }>;
  languages: string[];
  risk_breakdown: Record<string, number>;
  risk_score: number;
  urls: string[];
}

export function getPentAssetProfile(assetId: string) {
  return requestClient.get<PentAssetProfile>(`/pent/assets/${assetId}/profile`);
}

export function exportPentAssetReport(assetId: string, format: 'docx' | 'html') {
  return requestClient.post(`/pent/assets/${assetId}/reports`, { format });
}
