export interface SessionGroup<T> {
  items: T[];
  key: string;
  label: string;
}

interface TimestampedSession {
  created_at?: string;
  updated_at?: string;
}

const DAY_MS = 24 * 60 * 60 * 1000;

export function groupSessionsByTime<T extends TimestampedSession>(
  sessions: T[],
  now = new Date(),
): SessionGroup<T>[] {
  const startOfToday = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate(),
  ).getTime();
  const definitions = [
    { key: 'today', label: '今天' },
    { key: 'yesterday', label: '昨天' },
    { key: 'week', label: '近 7 天' },
    { key: 'month', label: '近 30 天' },
    { key: 'earlier', label: '更早' },
  ];
  const buckets = new Map(definitions.map(({ key }) => [key, [] as T[]]));

  for (const session of sessions) {
    const timestamp = new Date(
      session.updated_at || session.created_at || '',
    ).getTime();
    const key =
      !Number.isFinite(timestamp) || timestamp < startOfToday - 30 * DAY_MS
        ? 'earlier'
        : timestamp >= startOfToday
          ? 'today'
          : timestamp >= startOfToday - DAY_MS
              ? 'yesterday'
              : timestamp >= startOfToday - 7 * DAY_MS
                ? 'week'
                : 'month';
    buckets.get(key)!.push(session);
  }

  return definitions
    .map(({ key, label }) => ({ items: buckets.get(key)!, key, label }))
    .filter((group) => group.items.length > 0);
}
