import type { RuntimeSummary } from "../hooks/use-runtime-queries";

export type QueueRow = {
  name: string;
  pending: number;
  running: number;
  failed: number;
  dead: number;
  oldestSeconds?: number;
};

export function buildQueueRowsFromSummary(
  summary: RuntimeSummary | undefined
): QueueRow[] {
  if (!summary) {
    return [];
  }

  return [
    {
      name: "outbox",
      pending: summary.outbox.pending,
      running: summary.outbox.processing,
      failed: summary.outbox.failed,
      dead: summary.outbox.dead,
      ...optionalOldestSeconds(
        summary.outbox.oldestPendingAgeSeconds ??
          summary.outbox.oldestFailedAgeSeconds
      ),
    },
    {
      name: "runtime.functions",
      pending: summary.functions.pending,
      running: summary.functions.running,
      failed: summary.functions.failed,
      dead: summary.functions.dead,
      ...optionalOldestSeconds(
        summary.functions.oldestPendingAgeSeconds ??
          summary.functions.oldestFailedAgeSeconds
      ),
    },
  ];
}

function optionalOldestSeconds(value: number | undefined) {
  return value === undefined ? {} : { oldestSeconds: value };
}
