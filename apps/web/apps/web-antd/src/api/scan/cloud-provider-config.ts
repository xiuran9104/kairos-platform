import { requestClient } from '#/api/request';

export namespace ScanCloudProviderConfigApi {
  export interface CloudProviderConfig {
    id?: number; platform_id: number; provider: string;
    region_id: string; region_name: string; available_zones: string[];
    account_name: string; access_key_id: string; access_key_secret: string;
    auth_type: string; auth_username?: string; auth_password?: string;
    auth_domain?: string; project_id?: string;
    status: string; remarks?: string; last_test_time?: string; last_test_result?: string;
    created_at: string; updated_at?: string;
  }
}
export function getCloudProviderConfigList() { return requestClient.get<ScanCloudProviderConfigApi.CloudProviderConfig[]>('/asset-ops/cloud-provider-configs'); }
export function getCloudProviderConfig(id: number) { return requestClient.get<ScanCloudProviderConfigApi.CloudProviderConfig>(`/asset-ops/cloud-provider-configs/${id}`); }
export function createCloudProviderConfig(data: ScanCloudProviderConfigApi.CloudProviderConfig) { return requestClient.post('/asset-ops/cloud-provider-configs', data); }
export function updateCloudProviderConfig(id: number, data: ScanCloudProviderConfigApi.CloudProviderConfig) { return requestClient.put(`/asset-ops/cloud-provider-configs/${id}`, data); }
export function deleteCloudProviderConfig(id: number) { return requestClient.delete(`/asset-ops/cloud-provider-configs/${id}`); }
export function testCloudConnection(configId: number) { return requestClient.post('/asset-ops/cloud/test', { config_id: configId }); }
export function syncCloudInstances(configId: number) { return requestClient.post(`/asset-ops/cloud/${configId}/sync`, { sync_instances: true, sync_networks: true }); }
export function syncCloudFirewalls(configId: number) { return requestClient.post(`/asset-ops/cloud-firewalls/sync/${configId}`); }
