import { describe, expect, it } from 'vitest';

import { groupSessionsByTime } from './session-groups';

describe('groupSessionsByTime', () => {
  it('groups sessions by calendar age and omits empty groups', () => {
    const groups = groupSessionsByTime(
      [
        { id: 'today', updated_at: '2026-08-14T08:00:00+08:00' },
        { created_at: '2026-08-13T20:00:00+08:00', id: 'yesterday' },
        { id: 'week', updated_at: '2026-08-10T08:00:00+08:00' },
        { id: 'month', updated_at: '2026-07-20T08:00:00+08:00' },
        { id: 'earlier', updated_at: '2026-06-01T08:00:00+08:00' },
      ],
      new Date('2026-08-14T12:00:00+08:00'),
    );

    expect(groups.map((group) => group.label)).toEqual([
      '今天',
      '昨天',
      '近 7 天',
      '近 30 天',
      '更早',
    ]);
    expect(groups.map((group) => group.items[0]?.id)).toEqual([
      'today',
      'yesterday',
      'week',
      'month',
      'earlier',
    ]);
  });

  it('falls back to created_at when updated_at is absent', () => {
    const groups = groupSessionsByTime(
      [{ created_at: '2026-08-14T01:00:00Z', id: 'created' }],
      new Date('2026-08-14T12:00:00Z'),
    );
    expect(groups[0]?.items[0]?.id).toBe('created');
  });
});
