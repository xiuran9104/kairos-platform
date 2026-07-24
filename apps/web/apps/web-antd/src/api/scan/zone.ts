import { requestClient } from '#/api/request';

export namespace ScanZoneApi {
  export interface ZoneConfig { id: string; name: string; cidr: string; priority: number; cloud_platform_id?: number; cloud_platform_name?: string; machine_room_id?: number; machine_room_name?: string; }
}
export function getZoneList() { return requestClient.get<ScanZoneApi.ZoneConfig[]>('/asset-ops/zones'); }
export function getZone(id: string) { return requestClient.get<ScanZoneApi.ZoneConfig>(`/asset-ops/zones/${id}`); }
export function createZone(data: ScanZoneApi.ZoneConfig) { return requestClient.post('/asset-ops/zones', data); }
export function updateZone(id: string, data: ScanZoneApi.ZoneConfig) { return requestClient.put(`/asset-ops/zones/${id}`, data); }
export function deleteZone(id: string) { return requestClient.delete(`/asset-ops/zones/${id}`); }
