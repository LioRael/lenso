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
    ...functionQueueRows(functions, nowMs),
  ];
}

function functionQueueRows(functions: FunctionRun[], nowMs: number) {
  const queues = new Map<string, FunctionRun[]>();
  for (const run of functions) {
    const queue = run.runtimeDeclaration?.queue?.trim();
    const name = queue ? `runtime.functions:${queue}` : "runtime.functions";
    queues.set(name, [...(queues.get(name) ?? []), run]);
  }

  return [...queues.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([name, runs]) => ({
      name,
      pending: countStatus(runs, "pending"),
      running: countStatus(runs, "running"),
      failed: countStatus(runs, "failed"),
      dead: countStatus(runs, "dead"),
      ...oldestSeconds(runs, nowMs),
    }));
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
