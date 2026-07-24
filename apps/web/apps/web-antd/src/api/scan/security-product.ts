import { requestClient } from '#/api/request';

export namespace ScanSecurityProductApi {
  export interface SecurityProduct {
    id?: number; name: string; category: string; vendor: string;
    model: string; version: string; serial_number?: string;
    license_type: string; license_expiry?: string; management_ip?: string;
    deployment_mode: string; cloud_platform_id?: number; machine_room_id?: number;
    provider_id?: number; status: string; features?: string;
    throughput?: string; contact_person: string; contact_phone: string;
    remarks?: string; created_at: string;
  }
}
export function getSecurityProductList() { return requestClient.get<ScanSecurityProductApi.SecurityProduct[]>('/asset-ops/security-products'); }
export function getSecurityProduct(id: number) { return requestClient.get<ScanSecurityProductApi.SecurityProduct>(`/asset-ops/security-products/${id}`); }
export function createSecurityProduct(data: ScanSecurityProductApi.SecurityProduct) { return requestClient.post('/asset-ops/security-products', data); }
export function updateSecurityProduct(id: number, data: ScanSecurityProductApi.SecurityProduct) { return requestClient.put(`/asset-ops/security-products/${id}`, data); }
export function deleteSecurityProduct(id: number) { return requestClient.delete(`/asset-ops/security-products/${id}`); }
