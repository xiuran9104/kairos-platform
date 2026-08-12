import { requestClient } from '#/api/request';

export namespace ScanTaskApi {
  export interface ScanTask {
    id: string; scanner_id?: string; name: string; target: string; status: string; start_time?: string;
    end_time?: string; found_assets: number; found_risks: number;
    port_policy: string; domain_brute: boolean; service_detection: boolean;
    os_detection: boolean; site_identify: boolean;
  }
}
export function getTaskList() { return requestClient.get<ScanTaskApi.ScanTask[]>('/asset-ops/tasks'); }
export function getTask(id: string) { return requestClient.get<ScanTaskApi.ScanTask>(`/asset-ops/tasks/${id}`); }
export function createTask(data: any) { return requestClient.post('/asset-ops/tasks', data); }
export function deleteTask(id: string) { return requestClient.delete(`/asset-ops/tasks/${id}`); }
export function executeScan(data: { target_ip: string; ports: number[] }) { return requestClient.post('/asset-ops/scan', data); }
export function getScanResults() { return requestClient.get<any[]>('/asset-ops/results'); }
