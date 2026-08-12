import { requestClient } from '#/api/request';

export interface PentWorkflowNode {
  id: string;
  type: string;
  label?: string;
  position?: { x: number; y: number };
  config?: Record<string, any>;
}

export interface PentWorkflowEdge {
  id?: string;
  source: string;
  target: string;
  branch?: string;
  config?: { branch?: string };
}

export interface PentWorkflowGraph {
  nodes: PentWorkflowNode[];
  edges: PentWorkflowEdge[];
}

export interface PentWorkflow {
  id: string;
  tenant_id: number;
  name: string;
  description: string;
  enabled: boolean;
  graph: PentWorkflowGraph;
  version: number;
  created_at?: string;
  updated_at?: string;
}

export interface PentWorkflowTraceStep {
  node_id: string;
  label: string;
  type: string;
  status: string;
  detail: string;
}

export function getPentWorkflows(includeDisabled = true) {
  return requestClient.get<PentWorkflow[]>('/pent/workflows', {
    params: { include_disabled: includeDisabled },
  });
}

export function createPentWorkflow(data: Record<string, any>) {
  return requestClient.post<PentWorkflow>('/pent/workflows', data);
}

export function getPentWorkflow(id: string) {
  return requestClient.get<PentWorkflow>(`/pent/workflows/${id}`);
}

export function updatePentWorkflow(id: string, data: Record<string, any>) {
  return requestClient.put<PentWorkflow>(`/pent/workflows/${id}`, data);
}

export function deletePentWorkflow(id: string) {
  return requestClient.delete(`/pent/workflows/${id}`);
}

export function validatePentWorkflow(graph: PentWorkflowGraph) {
  return requestClient.post<{ errors: string[]; valid: boolean }>(
    '/pent/workflows/validate',
    { graph },
  );
}

export function dryRunPentWorkflow(
  graph: PentWorkflowGraph,
  inputs: Record<string, any>,
) {
  return requestClient.post<{
    errors?: string[];
    trace: PentWorkflowTraceStep[];
  }>('/pent/workflows/dry-run', { graph, inputs });
}
