import type { FunctionRun, RuntimeEvent } from "../data/mock-runtime";
import type { QueueRow } from "../pages/queues-model";

export function buildRuntimeQueueRows({
  events,
  functions,
  nowMs = Date.now(),
}: {
  events: RuntimeEvent[];
  functions: FunctionRun[];
  nowMs?: number;
}): QueueRow[] {
  return [
    {
      name: "outbox",
      pending: countStatus(events, "pending"),
      running: countStatus(events, "processing"),
      failed: countStatus(events, "failed"),
      dead: countStatus(events, "dead"),
      ...oldestSeconds(events, nowMs),
    },
    {
      name: "runtime.functions",
      pending: countStatus(functions, "pending"),
      running: countStatus(functions, "running"),
      failed: countStatus(functions, "failed"),
      dead: countStatus(functions, "dead"),
      ...oldestSeconds(functions, nowMs),
    },
  ];
}

function countStatus<T extends { status: string }>(items: T[], status: string) {
  return items.filter((item) => item.status === status).length;
}

function oldestSeconds<T extends { createdAt: string }>(
  items: T[],
  nowMs: number
) {
  const oldestMs = Math.min(
    ...items
      .map((item) => Date.parse(item.createdAt))
      .filter((value) => Number.isFinite(value))
  );
  if (!Number.isFinite(oldestMs)) {
    return {};
  }
  return {
    oldestSeconds: Math.max(0, Math.floor((nowMs - oldestMs) / 1000)),
  };
}
