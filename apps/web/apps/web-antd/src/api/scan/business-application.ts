import { requestClient } from '#/api/request';

export namespace ScanBusinessApplicationApi {
  export interface BusinessApplication { id?: number; name: string; description?: string; created_at: string; updated_at?: string; }
  export interface ApplicationEndpoint { id?: number; business_application_id: number; protocol: string; dest_ip: string; nat_ip?: string; dest_port: string; domain?: string; created_at: string; updated_at?: string; }
}
export function getBusinessApplicationList() { return requestClient.get<ScanBusinessApplicationApi.BusinessApplication[]>('/asset-ops/business-applications'); }
export function getBusinessApplication(id: number) { return requestClient.get<ScanBusinessApplicationApi.BusinessApplication>(`/asset-ops/business-applications/${id}`); }
export function createBusinessApplication(data: ScanBusinessApplicationApi.BusinessApplication) { return requestClient.post('/asset-ops/business-applications', data); }
export function updateBusinessApplication(id: number, data: ScanBusinessApplicationApi.BusinessApplication) { return requestClient.put(`/asset-ops/business-applications/${id}`, data); }
export function deleteBusinessApplication(id: number) { return requestClient.delete(`/asset-ops/business-applications/${id}`); }
export function getApplicationEndpoints(appId: number) { return requestClient.get<ScanBusinessApplicationApi.ApplicationEndpoint[]>(`/asset-ops/business-applications/${appId}/endpoints`); }
export function addApplicationEndpoint(appId: number, data: ScanBusinessApplicationApi.ApplicationEndpoint) { return requestClient.post(`/asset-ops/business-applications/${appId}/endpoints`, data); }
