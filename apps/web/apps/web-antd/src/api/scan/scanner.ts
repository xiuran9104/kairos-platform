import { requestClient } from '#/api/request';

export namespace ScanScannerApi {
  export interface ScannerConfig { id: string; name: string; scanner_type: string; enabled: boolean; config: any; agent_status?: string; agent_version?: string; agent_hostname?: string; agent_ip?: string; capabilities?: string[]; registered_at?: string; last_heartbeat?: string; registration_token?: string; created_at?: string; updated_at?: string; }
}
export function getScannerList() { return requestClient.get<ScanScannerApi.ScannerConfig[]>('/asset-ops/scanners'); }
export function createScanner(data: ScanScannerApi.ScannerConfig) { return requestClient.post('/asset-ops/scanners', data); }
export function updateScanner(id: string, data: ScanScannerApi.ScannerConfig) { return requestClient.put(`/asset-ops/scanners/${id}`, data); }
export function deleteScanner(id: string) { return requestClient.delete(`/asset-ops/scanners/${id}`); }
