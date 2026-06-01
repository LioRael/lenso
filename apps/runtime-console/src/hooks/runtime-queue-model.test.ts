import { describe, expect, test } from "vitest";

import type { FunctionRun, RuntimeEvent } from "../data/mock-runtime";
import { buildRuntimeQueueRows } from "./runtime-queue-model";

const events = [
  event("evt_pending", "pending", "2026-06-01T10:00:00.000Z"),
  event("evt_failed", "failed", "2026-06-01T10:00:05.000Z"),
] satisfies RuntimeEvent[];

const functions = [
  run("fn_running", "running", "2026-06-01T10:00:02.000Z"),
  run("fn_dead", "dead", "2026-06-01T10:00:03.000Z"),
] satisfies FunctionRun[];

describe("runtime queue model", () => {
  test("builds queue rows from real outbox and function API records", () => {
    expect(
      buildRuntimeQueueRows({
        events,
        functions,
        nowMs: Date.parse("2026-06-01T10:01:00.000Z"),
      })
    ).toEqual([
      {
        dead: 0,
        failed: 1,
        name: "outbox",
        oldestSeconds: 60,
        pending: 1,
        running: 0,
      },
      {
        dead: 1,
        failed: 0,
        name: "runtime.functions",
        oldestSeconds: 58,
        pending: 0,
        running: 1,
      },
    ]);
  });
});

function event(
  id: string,
  status: RuntimeEvent["status"],
  createdAt: string
): RuntimeEvent {
  return {
    actor: { kind: "system" },
    aggregateId: "resource_1",
    aggregateType: "resource",
    attempts: 1,
    causationId: "-",
    correlationId: "corr_queue",
    createdAt,
    eventName: "resource.published",
    id,
    maxAttempts: 3,
    payload: {},
    status,
  };
}

function run(
  id: string,
  status: FunctionRun["status"],
  createdAt: string
): FunctionRun {
  return {
    actor: { kind: "system" },
    attempts: 1,
    correlationId: "corr_queue",
    createdAt,
    functionName: "worker.process",
    id,
    input: {},
    logs: [],
    maxAttempts: 3,
    status,
  };
}
