import { describe, expect, test } from "vitest";

import {
  type ExecutionNode,
  type RetryableRuntimeRecord,
  retryTargetFor,
  retryTargetForNode,
  type TimelineItem,
} from "./mock-runtime";

function node(overrides: Partial<ExecutionNode>): ExecutionNode {
  return {
    attempts: 2,
    attributes: {},
    context: {},
    durationMs: 10,
    events: [],
    id: "node_id",
    kind: "function",
    logs: [],
    maxAttempts: 3,
    name: "Work",
    retryable: true,
    service: "svc",
    startMs: 0,
    status: "failed",
    ...overrides,
  };
}

describe("retryTargetForNode", () => {
  test("routes failed event nodes to the outbox endpoint", () => {
    expect(retryTargetForNode(node({ kind: "event" }))).toMatchObject({
      kind: "event",
      id: "node_id",
    });
  });

  test("routes failed function nodes to the functions endpoint", () => {
    expect(retryTargetForNode(node({ kind: "function" }))).toMatchObject({
      kind: "function",
      id: "node_id",
    });
  });

  test("returns null for http request nodes (no retry endpoint)", () => {
    expect(
      retryTargetForNode(node({ kind: "http", id: "httpreq_req_123" }))
    ).toBeNull();
  });

  test("returns null for non-retryable status", () => {
    expect(retryTargetForNode(node({ status: "completed" }))).toBeNull();
  });

  test("returns null when the node is not flagged retryable", () => {
    expect(retryTargetForNode(node({ retryable: false }))).toBeNull();
  });
});

function timelineRecord(
  overrides: Partial<TimelineItem>
): RetryableRuntimeRecord {
  return {
    kind: "timeline",
    item: {
      attempts: 2,
      correlationId: "corr",
      createdAt: "2026-06-01T00:00:00.000Z",
      id: "timeline_id",
      maxAttempts: 3,
      name: "Work",
      status: "failed",
      type: "function_run",
      ...overrides,
    },
  };
}

describe("retryTargetFor (timeline records)", () => {
  test("maps function_run timeline items via detailId", () => {
    expect(
      retryTargetFor(
        timelineRecord({ type: "function_run", detailId: "fn_real_id" })
      )
    ).toMatchObject({ kind: "function", id: "fn_real_id" });
  });

  test("maps outbox_event timeline items via detailId", () => {
    expect(
      retryTargetFor(
        timelineRecord({ type: "outbox_event", detailId: "evt_real_id" })
      )
    ).toMatchObject({ kind: "event", id: "evt_real_id" });
  });

  test("falls back to the timeline id when detailId is absent", () => {
    expect(
      retryTargetFor(timelineRecord({ type: "outbox_event" }))
    ).toMatchObject({ kind: "event", id: "timeline_id" });
  });

  test("returns null for timeline kinds without a retry endpoint", () => {
    expect(retryTargetFor(timelineRecord({ type: "http_request" }))).toBeNull();
  });
});
