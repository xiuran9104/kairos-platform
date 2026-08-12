import { requestClient } from '#/api/request';

export namespace ScanCloudZoneApi {
  export interface CloudZone { id?: number; provider_id: number; zone_name: string; zone_code: string; zone_type: 'private_cloud' | 'public_cloud'; description?: string; created_at: string; }
}
export namespace ScanCloudPlatformApi {
  export interface CloudPlatform { id?: number; zone_id: number; platform_name: string; platform_code: string; description?: string; created_at: string; }
}
export function getCloudZoneList() { return requestClient.get<ScanCloudZoneApi.CloudZone[]>('/asset-ops/cloud-zones'); }
export function getCloudZone(id: number) { return requestClient.get<ScanCloudZoneApi.CloudZone>(`/asset-ops/cloud-zones/${id}`); }
export function createCloudZone(data: ScanCloudZoneApi.CloudZone) { return requestClient.post('/asset-ops/cloud-zones', data); }
export function updateCloudZone(id: number, data: ScanCloudZoneApi.CloudZone) { return requestClient.put(`/asset-ops/cloud-zones/${id}`, data); }
export function deleteCloudZone(id: number) { return requestClient.delete(`/asset-ops/cloud-zones/${id}`); }
export function getCloudPlatformList() { return requestClient.get<ScanCloudPlatformApi.CloudPlatform[]>('/asset-ops/cloud-platforms'); }
export function getCloudPlatform(id: number) { return requestClient.get<ScanCloudPlatformApi.CloudPlatform>(`/asset-ops/cloud-platforms/${id}`); }
export function createCloudPlatform(data: ScanCloudPlatformApi.CloudPlatform) { return requestClient.post('/asset-ops/cloud-platforms', data); }
export function updateCloudPlatform(id: number, data: ScanCloudPlatformApi.CloudPlatform) { return requestClient.put(`/asset-ops/cloud-platforms/${id}`, data); }
export function deleteCloudPlatform(id: number) { return requestClient.delete(`/asset-ops/cloud-platforms/${id}`); }
