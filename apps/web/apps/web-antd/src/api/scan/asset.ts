import { requestClient } from '#/api/request';

export namespace ScanAssetApi {
  export interface Asset {
    id?: number; name: string; ip: string; zone: string; ports: any[];
    last_scanned?: string; contact_person?: string; contact_phone?: string;
    owner?: string; weight: number; labels: string[]; os?: string; device_type?: string;
    url?: string; language?: string; finding_count?: number; endpoint_count?: number;
  }
}
export function getAssetList() { return requestClient.get<ScanAssetApi.Asset[]>('/asset-ops/assets'); }
export function getAsset(id: number) { return requestClient.get<ScanAssetApi.Asset>(`/asset-ops/assets/${id}`); }
export function createAsset(data: ScanAssetApi.Asset) { return requestClient.post('/asset-ops/assets', data); }
export function updateAsset(id: number, data: ScanAssetApi.Asset) { return requestClient.put(`/asset-ops/assets/${id}`, data); }
export function deleteAsset(id: number) { return requestClient.delete(`/asset-ops/assets/${id}`); }
