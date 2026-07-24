import { requestClient } from '#/api/request';

export function getPentWorkspaceFiles(params?: {
  project_id?: string;
  run_id?: string;
}) {
  return requestClient.get('/pent/workspace/files', { params });
}

export function getPentWorkspaceUsage(projectId?: string) {
  return requestClient.get('/pent/workspace/usage', {
    params: { project_id: projectId },
  });
}
