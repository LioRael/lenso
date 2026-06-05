import { describe, expect, test } from "vitest";

import type {
  FunctionRun,
  RuntimeEvent,
  TimelineItem,
} from "../../data/mock-runtime";
import {
  remoteProxyCallSelectedId,
  resolveTimelineSourceRecord,
} from "./runtime-console-context";

describe("runtime console context helpers", () => {
  test("resolves outbox event timeline items from detail ids", () => {
    const record = resolveTimelineSourceRecord(
      timelineItem({ detailId: "evt_1", type: "outbox_event" }),
      {
        events: [runtimeEvent({ id: "evt_1" })],
        functions: [],
      }
    );

    expect(record).toMatchObject({
      kind: "event",
      item: { id: "evt_1" },
    });
  });

  test("resolves function run timeline items from detail ids", () => {
    const record = resolveTimelineSourceRecord(
      timelineItem({ detailId: "fn_1", type: "function_run" }),
      {
        events: [],
        functions: [functionRun({ id: "fn_1" })],
      }
    );

    expect(record).toMatchObject({
      kind: "function",
      item: { id: "fn_1" },
    });
  });

  test("does not treat remote proxy calls as event or function records", () => {
    expect(
      resolveTimelineSourceRecord(
        timelineItem({ detailId: "rpc_1", type: "remote_proxy_call" }),
        {
          events: [runtimeEvent({ id: "rpc_1" })],
          functions: [functionRun({ id: "rpc_1" })],
        }
      )
    ).toBeNull();
  });

  test("normalizes remote proxy timeline ids for remote call selection", () => {
    expect(
      remoteProxyCallSelectedId(
        timelineItem({
          detailId: "remoteproxy_rproxy_1",
          type: "remote_proxy_call",
        })
      )
    ).toBe("rproxy_1");

    expect(
      remoteProxyCallSelectedId(
        timelineItem({ detailId: "rproxy_2", type: "remote_proxy_call" })
      )
    ).toBe("rproxy_2");
  });
});

function timelineItem(overrides: Partial<TimelineItem>): TimelineItem {
  return {
    attempts: 1,
    correlationId: "corr_test",
    createdAt: "2026-06-03T00:00:00.000Z",
    id: "timeline_1",
    maxAttempts: 1,
    name: "Runtime Work",
    status: "completed",
    type: "runtime",
    ...overrides,
  };
}

function runtimeEvent(overrides: Partial<RuntimeEvent>): RuntimeEvent {
  return {
    actor: { kind: "system" },
    aggregateId: "-",
    aggregateType: "-",
    attempts: 1,
    causationId: "-",
    correlationId: "corr_test",
    createdAt: "2026-06-03T00:00:00.000Z",
    eventName: "test.event.v1",
    id: "evt_test",
    maxAttempts: 3,
    payload: {},
    status: "published",
    ...overrides,
  };
}

function functionRun(overrides: Partial<FunctionRun>): FunctionRun {
  return {
    actor: { kind: "system" },
    attempts: 1,
    correlationId: "corr_test",
    createdAt: "2026-06-03T00:00:00.000Z",
    functionName: "test.fn.v1",
    id: "fn_test",
    input: {},
    logs: [],
    maxAttempts: 3,
    status: "completed",
    ...overrides,
  };
}
