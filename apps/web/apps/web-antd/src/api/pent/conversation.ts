import { useAppConfig } from '@vben/hooks';
import { fetchEventSource } from '@vben/request';
import { useAccessStore } from '@vben/stores';

import { requestClient } from '#/api/request';

const { apiURL } = useAppConfig(import.meta.env, import.meta.env.PROD);
const accessStore = useAccessStore();

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

/** Stream a simple chat turn using Server-Sent Events. */
export function createTurnStream(
  sessionId: string,
  content: string,
  clientRequestId: string,
  ctrl: AbortController,
  onMessage: (event: any) => void,
  onError: (err: any) => void,
  onClose: () => void,
) {
  const token = accessStore.accessToken;

  /** Custom onopen: parse JSON errors so the user sees a real message instead of a
   *  misleading content-type error. */
  async function onopen(response: Response) {
    if (response.ok) {
      const contentType = response.headers.get('content-type');
      if (!contentType?.startsWith('text/event-stream')) {
        throw new Error(`预期 SSE 流，但服务器返回了 ${contentType}`);
      }
      return;
    }
    // Non-OK response: extract the server error from JSON body
    let msg = `请求失败 (${response.status})`;
    try {
      const body = await response.json();
      if (body?.message) msg = body.message;
    } catch {
      try {
        msg = await response.text() || msg;
      } catch { /* keep default msg */ }
    }
    throw new Error(msg);
  }

  return fetchEventSource(`${apiURL}/pent/sessions/${sessionId}/turns/stream`, {
    method: 'post',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${token}`,
    },
    openWhenHidden: true,
    body: JSON.stringify({ content, client_request_id: clientRequestId }),
    onopen,
    onmessage: onMessage,
    onerror: onError,
    onclose: onClose,
    signal: ctrl.signal,
  });
}
