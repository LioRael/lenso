import type { RuntimeSummary } from "../hooks/use-runtime-queries";
import { deadLettersPath, functionsPath } from "./operations-url-model";

export type QueueRow = {
  name: string;
  pending: number;
  running: number;
  failed: number;
  dead: number;
  oldestSeconds?: number;
};

export type QueueRouteTarget = {
  label: string;
  path: string;
  reason: string;
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

export function queueRouteTarget(row: QueueRow): QueueRouteTarget {
  if (row.name === "outbox") {
    return {
      label: "Open Outbox Events",
      path: deadLettersPath({ kind: "event", oldestFirst: true }),
      reason: "outbox failures and dead letters",
    };
  }

  const queue = functionQueueName(row.name);
  const status = queueStatusTarget(row);
  return {
    label: "Open Functions",
    path: functionsPath({
      ...(queue ? { queue } : {}),
      status,
    }),
    reason: queue ? `runtime function queue ${queue}` : "runtime functions",
  };
}

export function filterQueueRows(rows: QueueRow[], query: string) {
  const normalized = query.trim().toLowerCase();
  if (!normalized) {
    return rows;
  }
  return rows.filter((row) =>
    [
      row.name,
      String(row.pending),
      String(row.running),
      String(row.failed),
      String(row.dead),
      row.oldestSeconds === undefined ? "" : String(row.oldestSeconds),
    ].some((value) => value.toLowerCase().includes(normalized))
  );
}

export function queueRowId(row: QueueRow) {
  return row.name;
}

function functionQueueName(name: string) {
  const prefix = "runtime.functions:";
  return name.startsWith(prefix) ? name.slice(prefix.length) : "";
}

function queueStatusTarget(row: QueueRow) {
  if (row.dead > 0) {
    return "dead";
  }
  if (row.failed > 0) {
    return "failed";
  }
  if (row.running > 0) {
    return "running";
  }
  if (row.pending > 0) {
    return "pending";
  }
  return "all";
}

function optionalOldestSeconds(value: number | undefined) {
  return value === undefined ? {} : { oldestSeconds: value };
}
