import { requestClient } from '#/api/request';

export namespace ScanServiceProviderApi {
  export interface ServiceProvider {
    id?: number; provider_name: string; provider_code: string;
    short_name: string; logo_url?: string; contact_person: string;
    contact_phone: string; contact_email: string; headquarters: string;
    service_area: string; business_license: string; remarks?: string;
    status: string; created_at: string; updated_at?: string;
  }
}
export function getServiceProviderList() { return requestClient.get<ScanServiceProviderApi.ServiceProvider[]>('/asset-ops/service-providers'); }
export function getServiceProvider(id: number) { return requestClient.get<ScanServiceProviderApi.ServiceProvider>(`/asset-ops/service-providers/${id}`); }
export function createServiceProvider(data: ScanServiceProviderApi.ServiceProvider) { return requestClient.post('/asset-ops/service-providers', data); }
export function updateServiceProvider(id: number, data: ScanServiceProviderApi.ServiceProvider) { return requestClient.put(`/asset-ops/service-providers/${id}`, data); }
export function deleteServiceProvider(id: number) { return requestClient.delete(`/asset-ops/service-providers/${id}`); }
