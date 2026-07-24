import { requestClient } from '#/api/request';

export namespace ScanPortDetailApi {
  export interface PortDetail { id: string; asset_id: string; ip: string; port: number; protocol: string; status: string; service_name?: string; service_version?: string; service_banner?: string; system_name?: string; system_type?: string; middleware?: string; framework?: string; language?: string; environment?: string; department?: string; owner?: string; owner_contact?: string; vulnerability_level?: string; has_vulnerability: boolean; vulnerability_count: number; is_bound: boolean; notes?: string; last_scanned?: string; created_at?: string; updated_at?: string; }
}
export function getPortDetailList() { return requestClient.get<ScanPortDetailApi.PortDetail[]>('/asset-ops/port-details'); }
export function getPortDetail(id: string) { return requestClient.get<ScanPortDetailApi.PortDetail>(`/asset-ops/port-details/${id}`); }
export function updatePortDetail(id: string, data: any) { return requestClient.put(`/asset-ops/port-details/${id}`, data); }
