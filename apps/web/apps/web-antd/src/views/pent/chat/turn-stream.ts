export interface TurnStreamMessage {
  content: string;
  format: string;
  role: string;
  sequence?: number;
  type: string;
}

export type TurnStreamEvent =
  | { kind: 'cancelled' | 'done'; message?: string }
  | { kind: 'error'; message: string }
  | { kind: 'message'; message: TurnStreamMessage };

interface SseMessage {
  data?: string;
  event?: string;
}

function parseData(data?: string): Record<string, any> {
  if (!data || data === '[DONE]' || data === '[CANCELLED]') return {};
  const parsed: unknown = JSON.parse(data);
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    throw new Error('SSE 事件数据格式无效');
  }
  return parsed as Record<string, any>;
}

export function parseTurnStreamEvent(event: SseMessage): TurnStreamEvent {
  const type = event.event || 'message';
  const data = parseData(event.data);

  if (type === 'done') return { kind: 'done' };
  if (type === 'cancelled') return { kind: 'cancelled' };
  if (type === 'error') {
    return { kind: 'error', message: String(data.error || '对话回复失败') };
  }

  return {
    kind: 'message',
    message: {
      content: String(data.content || ''),
      format: String(data.format || 'text'),
      role: String(data.role || 'assistant'),
      sequence:
        typeof data.sequence === 'number' ? data.sequence : undefined,
      type,
    },
  };
}
