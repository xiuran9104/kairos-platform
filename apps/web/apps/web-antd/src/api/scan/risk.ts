import { requestClient } from '#/api/request';

export namespace ScanRiskApi {
  export interface Risk { id: string; rule_id?: string; asset_ip: string; port: number; severity: string; description: string; solution?: string; status: string; created_at?: string; updated_at?: string; assigned_to?: string; evidence?: Record<string, unknown>; source?: string; first_seen?: string; last_seen?: string; }
}
export function getRiskList() { return requestClient.get<ScanRiskApi.Risk[]>('/asset-ops/risks'); }
export function getRisk(id: string) { return requestClient.get<ScanRiskApi.Risk>(`/asset-ops/risks/${id}`); }
export function updateRisk(id: string, data: { status?: string; assigned_to?: string; solution?: string }) { return requestClient.put(`/asset-ops/risks/${id}`, data); }
