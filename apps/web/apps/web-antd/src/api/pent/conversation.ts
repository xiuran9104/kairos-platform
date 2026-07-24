import { requestClient } from '#/api/request';
export function createTurn(sessionId: string, data: { content: string; client_request_id?: string }) {
  // Tool-calling loop can take several minutes; timeout must allow for it.
  return requestClient.post(`/pent/sessions/${sessionId}/turns`, data, { timeout: 300_000 });
}
export function listTurns(sessionId: string) {
  return requestClient.get(`/pent/sessions/${sessionId}/turns`);
}
export function cancelTurn(turnId: string) {
  return requestClient.post(`/pent/turns/${turnId}/cancel`);
}
export function retryTurn(turnId: string) {
  return requestClient.post(`/pent/turns/${turnId}/retry`);
}
