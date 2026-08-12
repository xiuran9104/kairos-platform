import { requestClient } from '#/api/request';

export namespace ScanBusinessResourceApi {
  export interface BusinessResource {
    id?: number; resource_type: string; ecs_name: string; ecs_status: string;
    resource_id: string; cloud_region: string; cloud_category: string;
    cloud_provider_config_id?: number; machine_room_id?: number; zone_name?: string; platform_name?: string;
    cabinet_id?: number; cabinet_name?: string; chassis_name?: string; chassis_slot?: string; rack_start_u?: number; rack_end_u?: number;
    deployment_type?: 'cloud_managed' | 'datacenter_managed' | 'standalone';
    management_ip?: string; business_ip?: string; network_cidr?: string; gateway?: string;
    vlan_id?: string; dns_servers?: string; mac_address?: string; switch_name?: string; switch_port?: string;
    county_city?: string; vdc_name?: string; customer_name: string;
    application_name?: string; contract_name?: string; instance_id: string;
    ecs_type: string; ecs_os: string; cpu_cores: number; memory_gb: number;
    system_disk: string; system_disk_size_gb: number; data_disk?: string;
    has_security_product: boolean; ip_address: string;
    ecs_login_method?: string; ecs_login_username?: string; ecs_initial_password?: string;
    bastion_address?: string; bastion_admin_account?: string; bastion_initial_password?: string;
    remarks?: string; created_at?: string; updated_at?: string; created_by?: string;
    applicant?: string; department?: string; approval_time?: string;
    application_status?: string; delivery_status?: string;
  }
}
export function getBusinessResourceList() { return requestClient.get<ScanBusinessResourceApi.BusinessResource[]>('/asset-ops/business-resources'); }
export function getBusinessResource(id: number) { return requestClient.get<ScanBusinessResourceApi.BusinessResource>(`/asset-ops/business-resources/${id}`); }
export function createBusinessResource(data: ScanBusinessResourceApi.BusinessResource) { return requestClient.post('/asset-ops/business-resources', data); }
export function updateBusinessResource(id: number, data: ScanBusinessResourceApi.BusinessResource) { return requestClient.put(`/asset-ops/business-resources/${id}`, data); }
export function deleteBusinessResource(id: number) { return requestClient.delete(`/asset-ops/business-resources/${id}`); }
