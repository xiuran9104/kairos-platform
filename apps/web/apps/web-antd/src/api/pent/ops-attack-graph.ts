import { requestClient } from '#/api/request';

export interface PentAttackGraphNode {
  category?: string;
  confidence?: number | string;
  evidence_ids?: string[];
  id: string;
  kind: 'asset' | 'context' | 'credential' | 'exploit' | 'vulnerability';
  name: string;
  risk_level?: string;
  severity?: string;
  status?: string;
  target?: string;
}

export interface PentAttackGraphEdge {
  id: string;
  kind: string;
  label: string;
  source: string;
  target: string;
}

export interface PentProjectAttackGraph {
  edges: PentAttackGraphEdge[];
  nodes: PentAttackGraphNode[];
  summary: {
    assets: number;
    credentials: number;
    exploits: number;
    vulnerabilities: number;
  };
}

export function getPentProjectAttackGraph(projectId: string) {
  return requestClient.get<PentProjectAttackGraph>(
    `/pent/projects/${projectId}/attack-graph`,
  );
}
