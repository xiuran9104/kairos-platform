import { requestClient } from '#/api/request';

export interface PentFileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified_at?: string;
}

export interface PentFileListResult {
  path: string;
  entries: PentFileEntry[];
  total: number;
  page: number;
  page_size: number;
}

export function getPentFiles(params?: Record<string, any>) {
  return requestClient.get<PentFileListResult>('/pent/files', { params });
}

export function uploadPentFile(file: File, path: string) {
  const form = new FormData();
  form.append('file', file);
  form.append('path', path);
  return requestClient.post<{ path: string; size: number }>(
    '/pent/files/upload',
    form,
    { headers: { 'Content-Type': 'multipart/form-data' } },
  );
}

export function downloadPentFile(path: string) {
  return requestClient.download('/pent/files/download', {
    params: { path },
    responseType: 'blob',
  });
}

export function getPentFileContent(path: string) {
  return requestClient.get<{ content: string; path: string }>(
    '/pent/files/content',
    { params: { path } },
  );
}

export function savePentFileContent(path: string, content: string) {
  return requestClient.put('/pent/files/content', { path, content });
}

export function renamePentFile(path: string, newName: string) {
  return requestClient.post('/pent/files/rename', { path, new_name: newName });
}

export function mkdirPentFile(path: string, name: string) {
  return requestClient.post('/pent/files/mkdir', { path, name });
}

export function deletePentFile(path: string) {
  return requestClient.delete('/pent/files', { data: { path } });
}
