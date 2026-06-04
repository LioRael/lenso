import { describe, expect, test } from "vitest";

import type { RuntimeStory } from "../../data/mock-runtime";
import {
  buildExecutionActivity,
  buildExecutionContext,
  buildExecutionFailures,
  buildExecutionPayload,
  buildRemoteProxyInspectorDetail,
  defaultExecutionInspectorTab,
  executionInspectorTabs,
  getExecutionInspectorTabCounts,
} from "./execution-inspector-model";

const story: RuntimeStory = {
  correlationId: "corr_drawer",
  durationMs: 400,
  edges: [
    { id: "parent:node", source: "parent", target: "node", type: "causation" },
    { id: "node:child", source: "node", target: "child", type: "causation" },
  ],
  id: "story_drawer",
  name: "Drawer Story",
  nodes: [
    {
      attributes: {},
      context: { causation_id: "request_1", actor: "user:1" },
      durationMs: 80,
      events: [],
      id: "parent",
      kind: "http",
      logs: [],
      name: "PublishResource",
      service: "api",
      startMs: 0,
      status: "completed",
    },
    {
      attempts: 2,
      attributes: { aggregate_id: "resource_1" },
      context: { causation_id: "parent", trigger_source: "outbox" },
      durationMs: 100,
      events: [
        {
          attributes: { state: "processing" },
          name: "state.processing",
          timestampMs: 110,
        },
      ],
      id: "node",
      kind: "function",
      logs: ["attempt 1 failed", "attempt 2 succeeded"],
      maxAttempts: 3,
      name: "GenerateSearchIndex",
      parentId: "parent",
      payload: {
        locale: "en-US",
        resource_id: "resource_1",
      },
      retryable: true,
      service: "search",
      startMs: 100,
      status: "completed",
    },
    {
      attributes: {},
      context: {},
      durationMs: 50,
      events: [],
      id: "child",
      kind: "event",
      logs: [],
      name: "SearchIndexGenerated",
      parentId: "node",
      service: "outbox",
      startMs: 220,
      status: "published",
    },
  ],
  service: "runtime",
  source: "runtime-story",
  status: "completed",
  timelineItems: [
    {
      attempts: 1,
      correlationId: "corr_drawer",
      createdAt: "2026-06-01T10:00:00.100Z",
      detailId: "node",
      id: "timeline_node_start",
      maxAttempts: 3,
      name: "GenerateSearchIndex started",
      startedAt: "2026-06-01T10:00:00.100Z",
      status: "running",
      type: "function_run",
    },
    {
      attempts: 2,
      completedAt: "2026-06-01T10:00:00.200Z",
      correlationId: "corr_drawer",
      createdAt: "2026-06-01T10:00:00.200Z",
      detailId: "node",
      id: "timeline_node_done",
      maxAttempts: 3,
      name: "GenerateSearchIndex completed",
      status: "completed",
      type: "function_run",
    },
  ],
  timestamp: "2026-06-01T10:00:00.000Z",
};

describe("execution inspector model", () => {
  test("uses operator workflow tabs", () => {
    expect(executionInspectorTabs.map((tab) => tab.label)).toEqual([
      "Overview",
      "Payload",
      "Activity",
      "Logs",
      "Failures",
      "Context",
      "Technical",
    ]);
  });

  test("keeps payload as a primary business data tab", () => {
    const payload = buildExecutionPayload(story.nodes[1]!);

    expect(payload).toEqual({
      input: {
        locale: "en-US",
        resource_id: "resource_1",
      },
      metadata: {
        aggregate_id: "resource_1",
      },
      output: undefined,
    });
  });

  test("builds chronological activity from timeline items and node events", () => {
    const node = story.nodes[1]!;
    const activity = buildExecutionActivity(story, node);

    expect(activity.map((item) => item.label)).toEqual([
      "GenerateSearchIndex started",
      "state.processing",
      "GenerateSearchIndex completed",
    ]);
    expect(activity.map((item) => item.kind)).toEqual([
      "function_run",
      "event",
      "function_run",
    ]);
  });

  test("reports no failures for successful execution", () => {
    expect(buildExecutionFailures(story.nodes[1]!)).toEqual([]);
  });

  test("extracts failure details for failed execution", () => {
    const failed = {
      ...story.nodes[1]!,
      logs: ["attempt 1 failed", "last error"],
      status: "failed" as const,
    };

    expect(buildExecutionFailures(failed).map((item) => item.label)).toEqual([
      "current error",
      "last error",
      "retry history",
    ]);
  });

  test("builds remote proxy inspector details from source metadata", () => {
    const node = {
      ...story.nodes[1]!,
      attributes: {
        source_metadata: {
          declared_path: "/contacts/{id}",
          duration_ms: 1420,
          error_code: "remote_http_429",
          error_details: {
            message: "remote module rate limited the request",
          },
          method: "POST",
          module_name: "remote-crm",
          path_params: { id: "contact_1" },
          remote_path: "/v1/contacts/contact_1",
          remote_proxy_call_id: "rproxy_1",
          remote_status: 429,
          request_id: "req_remote_proxy",
          retryable: true,
          span_id: "span_remote_proxy",
          trace_id: "trace_remote_proxy",
        },
      },
      durationMs: 1500,
      kind: "external" as const,
      service: "remote-crm",
      status: "failed" as const,
    };

    const detail = buildRemoteProxyInspectorDetail(node);

    expect(detail?.rows).toEqual([
      ["result", "retryable failure"],
      ["module", "remote-crm"],
      ["declared route", "POST /contacts/{id}"],
      ["remote path", "/v1/contacts/contact_1"],
      ["remote status", 429],
      ["duration", "1.42s"],
      ["request id", "req_remote_proxy"],
      ["trace id", "trace_remote_proxy"],
      ["span id", "span_remote_proxy"],
      ["error code", "remote_http_429"],
      ["retryability", "retryable"],
    ]);
    expect(detail?.pathParams).toEqual({ id: "contact_1" });
    expect(detail?.errorDetails).toEqual({
      message: "remote module rate limited the request",
    });
  });

  test("does not build remote proxy details for ordinary external nodes", () => {
    expect(
      buildRemoteProxyInspectorDetail({
        ...story.nodes[1]!,
        attributes: { provider: "stripe" },
        kind: "external",
      })
    ).toBeUndefined();
  });

  test("builds context lineage with upstream and downstream executions", () => {
    const context = buildExecutionContext(story, story.nodes[1]!);

    expect(context.upstream.map((node) => node.id)).toEqual(["parent"]);
    expect(context.downstream.map((node) => node.id)).toEqual(["child"]);
    expect(context.rows).toEqual(
      expect.arrayContaining([
        ["correlation id", "corr_drawer"],
        ["causation id", "parent"],
        ["actor", "user:1"],
        ["aggregate", "resource_1"],
        ["trigger source", "outbox"],
      ])
    );
  });

  test("selects failure, payload, then overview as default drawer tabs", () => {
    expect(defaultExecutionInspectorTab(story.nodes[1]!)).toBe("payload");
    const { payload: _payload, ...nodeWithoutPayload } = story.nodes[1]!;
    expect(defaultExecutionInspectorTab(nodeWithoutPayload)).toBe("overview");
    expect(
      defaultExecutionInspectorTab({
        ...story.nodes[1]!,
        status: "failed",
      })
    ).toBe("failures");
  });

  test("counts drawer tab content without data-type tab names", () => {
    const counts = getExecutionInspectorTabCounts(story, story.nodes[1]!);

    expect(counts).toMatchObject({
      activity: 3,
      context: 2,
      failures: 0,
      logs: 2,
      overview: 0,
      payload: 2,
      technical: 0,
    });
  });
});
