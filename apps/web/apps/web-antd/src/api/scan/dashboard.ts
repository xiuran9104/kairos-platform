import { requestClient } from '#/api/request';

export namespace ScanDashboardApi {
  export interface DashboardSummary { asset_count: number; scanned_asset_count: number; task_count: number; running_task_count: number; risk_count: number; open_risk_count: number; database_connected: boolean; version: string; }
}
export function getDashboardSummary() { return requestClient.get<ScanDashboardApi.DashboardSummary>('/asset-ops/dashboard'); }
