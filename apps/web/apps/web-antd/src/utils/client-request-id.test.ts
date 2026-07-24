import { describe, expect, it } from 'vitest';

import { createClientRequestId } from './client-request-id';

describe('createClientRequestId', () => {
  it('uses native randomUUID when available', () => {
    const id = createClientRequestId({
      randomUUID: () => 'native-id',
    });

    expect(id).toBe('native-id');
  });

  it('creates an RFC 4122 v4 id when randomUUID is unavailable', () => {
    const id = createClientRequestId({
      getRandomValues: (bytes) => {
        bytes.fill(0xab);
        return bytes;
      },
    });

    expect(id).toBe('abababab-abab-4bab-abab-abababababab');
  });
});
