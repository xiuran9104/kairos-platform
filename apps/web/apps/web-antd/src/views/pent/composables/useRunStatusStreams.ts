import { onUnmounted } from 'vue';

import { streamPentRunEvents } from '#/api/pent';

const terminalStatuses = new Set(['cancelled', 'completed', 'failed']);

function statusFromEvent(event: any): string | undefined {
  const type = event?.event || event?.event_type || '';
  if (type === 'run.cancelled') return 'cancelled';
  if (type === 'run.completed') return 'completed';
  if (type === 'run.failed') return 'failed';
  if (type === 'run.retry_scheduled' || type === 'run.queued') return 'queued';
  if (type === 'run.started' || type === 'run.claimed') return 'running';
  try {
    const data = JSON.parse(event?.data || '{}');
    return data?.payload?.status;
  } catch {
    return undefined;
  }
}

export function useRunStatusStreams(
  onStatus: (runId: string, status: string, event: any) => void,
) {
  const controllers = new Map<string, AbortController>();

  function stop(runId: string) {
    controllers.get(runId)?.abort();
    controllers.delete(runId);
  }

  function sync(runIds: string[]) {
    const wanted = new Set(runIds.filter(Boolean));
    for (const runId of controllers.keys()) {
      if (!wanted.has(runId)) stop(runId);
    }
    for (const runId of wanted) {
      if (controllers.has(runId)) continue;
      const controller = new AbortController();
      controllers.set(runId, controller);
      streamPentRunEvents(
        runId,
        controller,
        (event: any) => {
          const status = statusFromEvent(event);
          if (!status) return;
          onStatus(runId, status, event);
          if (terminalStatuses.has(status)) stop(runId);
        },
        () => stop(runId),
        () => stop(runId),
      );
    }
  }

  onUnmounted(() => {
    for (const runId of [...controllers.keys()]) stop(runId);
  });

  return { sync, stop };
}
