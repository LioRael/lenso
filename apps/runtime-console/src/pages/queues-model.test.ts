import { describe, expect, test } from "vitest";

import type { RuntimeSummary } from "../hooks/use-runtime-queries";
import { buildQueueRowsFromSummary } from "./queues-model";

const summary = {
  functions: {
    completed: 20,
    dead: 1,
    failed: 2,
    oldestFailedAgeSeconds: 99,
    pending: 3,
    running: 4,
  },
  outbox: {
    dead: 0,
    failed: 5,
    oldestPendingAgeSeconds: 42,
    pending: 6,
    processing: 7,
    published: 30,
  },
  recentActivity: [],
  recentFailures: [],
  status: "degraded",
} satisfies RuntimeSummary;

describe("queues model", () => {
  test("builds aggregate queue rows from runtime summary", () => {
    expect(buildQueueRowsFromSummary(summary)).toEqual([
      {
        dead: 0,
        failed: 5,
        name: "outbox",
        oldestSeconds: 42,
        pending: 6,
        running: 7,
      },
      {
        dead: 1,
        failed: 2,
        name: "runtime.functions",
        oldestSeconds: 99,
        pending: 3,
        running: 4,
      },
    ]);
  });

  test("omits oldest age when backend summary has no age fields", () => {
    const rows = buildQueueRowsFromSummary({
      ...summary,
      functions: {
        completed: 0,
        dead: 0,
        failed: 0,
        pending: 0,
        running: 0,
      },
      outbox: {
        dead: 0,
        failed: 0,
        pending: 0,
        processing: 0,
        published: 0,
      },
    });

    expect(rows.map((row) => row.oldestSeconds)).toEqual([
      undefined,
      undefined,
    ]);
  });
});
