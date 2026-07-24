import { requestClient } from '#/api/request';

export namespace ScanCloudServiceAssetApi {
  export interface CloudServiceAsset {
    id: string; asset_type: string; source_type: string; name: string;
    instance_id: string; status: string; region?: string; cloud_platform: string;
    cloud_zone: string; instance_type: string; cpu_cores: number; memory_gb: number;
    system_disk_type: string; system_disk_size_gb: number; os_type: string; os_name: string;
    ip_address: string; public_ip?: string; customer_name: string; department?: string;
    project?: string; application_name?: string; owner_name?: string;
    created_at: string; updated_at?: string;
  }
}
export function getCloudServiceAssetList() { return requestClient.get<ScanCloudServiceAssetApi.CloudServiceAsset[]>('/asset-ops/cloud-service-assets'); }
export function getCloudServiceAsset(id: string) { return requestClient.get<ScanCloudServiceAssetApi.CloudServiceAsset>(`/asset-ops/cloud-service-assets/${id}`); }
