import { describe, expect, it } from 'vitest';

import { parseTurnStreamEvent } from './turn-stream';

describe('parseTurnStreamEvent', () => {
  it('parses assistant and tool events', () => {
    expect(
      parseTurnStreamEvent({
        data: JSON.stringify({
          content: 'hello',
          format: 'text',
          role: 'assistant',
          sequence: 2,
        }),
        event: 'conversation.assistant_message',
      }),
    ).toEqual({
      kind: 'message',
      message: {
        content: 'hello',
        format: 'text',
        role: 'assistant',
        sequence: 2,
        type: 'conversation.assistant_message',
      },
    });
  });

  it('handles terminal events without parsing their sentinel payloads', () => {
    expect(parseTurnStreamEvent({ data: '[DONE]', event: 'done' })).toEqual({
      kind: 'done',
    });
    expect(
      parseTurnStreamEvent({ data: '[CANCELLED]', event: 'cancelled' }),
    ).toEqual({ kind: 'cancelled' });
  });

  it('surfaces server errors', () => {
    expect(
      parseTurnStreamEvent({
        data: JSON.stringify({ error: 'model unavailable' }),
        event: 'error',
      }),
    ).toEqual({ kind: 'error', message: 'model unavailable' });
  });

  it('rejects malformed data', () => {
    expect(() =>
      parseTurnStreamEvent({ data: '{', event: 'conversation.message' }),
    ).toThrow();
  });
});
