import { requestClient } from '#/api/request';

export interface InfocollectProvider {
  id: string;
  name: string;
  configured: boolean;
}

export interface InfocollectSearchResult {
  provider: string;
  fields: string[];
  results: Array<Record<string, any>>;
  total: number;
  page: number;
  size: number;
}

export function getInfocollectProviders() {
  return requestClient.get<InfocollectProvider[]>('/pent/infocollect/providers');
}

export interface AssetImportItem {
  target: string;
  [key: string]: any;
}

/** 信息收集结果入库（复用资产导入接口）。 */
export function importAssetsForInfocollect(data: {
  assets: AssetImportItem[];
  source?: string;
}) {
  return requestClient.post<{ created: number; updated: number; skipped: number }>(
    '/pent/assets/import',
    data,
  );
}

export function searchInfocollect(data: {
  provider: string;
  query: string;
  size?: number;
  page?: number;
  fields?: string[];
  full?: boolean;
}) {
  return requestClient.post<InfocollectSearchResult>(
    '/pent/infocollect/search',
    data,
  );
}
