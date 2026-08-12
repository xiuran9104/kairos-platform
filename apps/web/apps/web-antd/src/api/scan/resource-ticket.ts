import { requestClient } from '#/api/request';

export namespace ScanResourceTicketApi {
  export interface ResourceTicket {
    id?: number; resource_type: string; ecs_name: string; ticket_status: string;
    provider_id?: number; provider_name?: string; cloud_platform_id?: number;
    cloud_platform_name?: string; machine_room_id?: number; machine_room_name?: string;
    cloud_region?: string; cloud_category?: string; zone_name?: string;
    zone_cabinet?: string; rack_units: number; customer_name?: string;
    application_name?: string; contract_name?: string; ecs_type?: string;
    ecs_os?: string; resource_count: number; cpu_cores: number; memory_gb: number;
    system_disk?: string; system_disk_size_gb: number; data_disk?: string;
    expire_at?: string; has_security_product: boolean; security_products?: string;
    ip_address?: string; delivery_status?: string; remarks?: string;
    created_at: string; updated_at?: string; created_by: string;
    approver?: string; approve_time?: string; approve_comment?: string;
    provisioner?: string; provision_time?: string; provision_details?: string;
    deliverer?: string; deliver_time?: string; deliver_comment?: string;
  }
}
export function getResourceTicketList() { return requestClient.get<ScanResourceTicketApi.ResourceTicket[]>('/asset-ops/resource-tickets'); }
export function getResourceTicket(id: number) { return requestClient.get<ScanResourceTicketApi.ResourceTicket>(`/asset-ops/resource-tickets/${id}`); }
export function createResourceTicket(data: any) { return requestClient.post('/asset-ops/resource-tickets', data); }
export function updateResourceTicket(id: number, data: any) { return requestClient.put(`/asset-ops/resource-tickets/${id}`, data); }
export function deleteResourceTicket(id: number) { return requestClient.delete(`/asset-ops/resource-tickets/${id}`); }
export function approveTicket(id: number, data: { approved: boolean; comment?: string }) { return requestClient.post(`/asset-ops/resource-tickets/${id}/approve`, data); }
export function getTicketApprovalHistory(id: number) { return requestClient.get<any[]>(`/asset-ops/resource-tickets/${id}/approvals`); }
export function provisionTicket(id: number, data: { details?: string; config_id?: number; image_id?: string; flavor?: string }) { return requestClient.post(`/asset-ops/resource-tickets/${id}/provision`, data); }
export function deliverTicket(id: number, data: { comment?: string }) { return requestClient.post(`/asset-ops/resource-tickets/${id}/deliver`, data); }
