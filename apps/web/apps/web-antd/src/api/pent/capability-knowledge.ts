import { requestClient } from '#/api/request';

export interface PentKnowledgeRetrievalItem {
  id?: string;
  title?: string;
}

export interface PentKnowledgeRetrievalLog {
  id: string;
  query: string;
  risk_type?: string;
  conversation_id?: string;
  message_id?: string;
  retrieved_items: PentKnowledgeRetrievalItem[];
  created_at: string;
}

export function getPentKnowledgeRetrievalLogs(params?: {
  conversation_id?: string;
  limit?: number;
  message_id?: string;
}) {
  return requestClient.get<PentKnowledgeRetrievalLog[]>(
    '/pent/knowledge/retrieval-logs',
    { params },
  );
}

export function deletePentKnowledgeRetrievalLog(id: string) {
  return requestClient.delete(`/pent/knowledge/retrieval-logs/${id}`);
}
