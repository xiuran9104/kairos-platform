import { requestClient } from '#/api/request';

export interface PentFact {
  id: string;
  tenant_id: number;
  project_id: string;
  fact_key: string;
  category: string;
  summary: string;
  body: string;
  confidence: string;
  pinned: boolean;
  deprecated: boolean;
  vuln_id?: string;
  links_text?: string;
  created_at?: string;
  updated_at?: string;
}

export interface PentFactGraph {
  nodes: Array<{
    fact_key: string;
    category: string;
    summary: string;
    confidence: string;
    deprecated: boolean;
    pinned: boolean;
  }>;
  edges: Array<{
    id: string;
    source_fact_key: string;
    target_fact_key: string;
    edge_type: string;
  }>;
}

export interface PentProjectStats {
  fact_count: number;
  vuln_count: number;
  conversation_count: number;
  sparse_fact_count: number;
}

export interface PentProjectConversation {
  id: string;
  objective: string;
  status: string;
  created_at?: string;
  updated_at?: string;
}

export function getPentProjectStats(projectId: string) {
  return requestClient.get<PentProjectStats>(`/pent/projects/${projectId}/stats`);
}

export function getPentProjectFacts(
  projectId: string,
  params?: Record<string, any>,
) {
  return requestClient.get<PentFact[]>(`/pent/projects/${projectId}/facts`, {
    params,
  });
}

export function createPentFact(projectId: string, data: Record<string, any>) {
  return requestClient.post<PentFact>(`/pent/projects/${projectId}/facts`, data);
}

export function getPentFact(id: string) {
  return requestClient.get<PentFact>(`/pent/facts/${id}`);
}

export function updatePentFact(id: string, data: Record<string, any>) {
  return requestClient.put<PentFact>(`/pent/facts/${id}`, data);
}

export function deletePentFact(id: string) {
  return requestClient.delete(`/pent/facts/${id}`);
}

export function deprecatePentFact(id: string) {
  return requestClient.post(`/pent/facts/${id}/deprecate`);
}

export function restorePentFact(id: string) {
  return requestClient.post(`/pent/facts/${id}/restore`);
}

export function getPentProjectFactGraph(projectId: string) {
  return requestClient.get<PentFactGraph>(
    `/pent/projects/${projectId}/fact-graph`,
  );
}

export function createPentFactEdge(data: {
  project_id: string;
  source_fact_key: string;
  target_fact_key: string;
  edge_type?: string;
}) {
  return requestClient.post('/pent/fact-edges', data);
}

export function deletePentFactEdge(id: string) {
  return requestClient.delete(`/pent/fact-edges/${id}`);
}

export function getPentProjectConversations(projectId: string) {
  return requestClient.get<PentProjectConversation[]>(
    `/pent/projects/${projectId}/conversations`,
  );
}

export function bindPentConversationProject(id: string, projectId: null | string) {
  return requestClient.put(`/pent/conversations/${id}/project`, {
    project_id: projectId,
  });
}
