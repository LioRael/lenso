import { describe, expect, test } from "vitest";

import { buildRemoteProxyInspectorDetail } from "../components/runtime/execution-inspector-model";
import { buildExecutionTimelineRows } from "../components/runtime/execution-timeline-model";
import {
  normalizeExecutionLogs,
  normalizeExecutionPayload,
  normalizeRuntimeHeatmap,
  normalizeRuntimeStory,
  normalizeRuntimeStoryListItem,
  normalizeTechnicalOperations,
  type ApiRuntimeStoryDetail,
} from "./runtime-api-model";

const normalStory: ApiRuntimeStoryDetail = {
  summary: {
    correlation_id: "corr_normal",
    created_at: "2026-06-01T12:00:00.000Z",
    duration: 500,
    error_count: 0,
    node_count: 2,
    pattern: ["function_run", "outbox_event"],
    services: ["identity", "outbox"],
    status: "completed",
    title: "CreateUser",
    updated_at: "2026-06-01T12:00:00.500Z",
  },
  nodes: [
    {
      duration_ms: 120,
      id: "fn_create_user",
      metadata: { attempts: 1, max_attempts: 3 },
      name: "identity.create_user",
      service: "identity",
      status: "completed",
      timestamp: "2026-06-01T12:00:00.000Z",
      type: "function_run",
    },
    {
      duration_ms: 80,
      id: "evt_user_registered",
      metadata: {
        attempts: 1,
        causation_id: "fn_create_user",
        max_attempts: 3,
      },
      display_name: "User Registered",
      name: "identity.user_registered.v1",
      service: "outbox",
      status: "published",
      timestamp: "2026-06-01T12:00:00.300Z",
      type: "outbox_event",
    },
  ],
  edges: [
    {
      id: "fn_create_user:evt_user_registered:causation",
      source: "fn_create_user",
      target: "evt_user_registered",
      type: "causation",
    },
  ],
  timeline_items: [
    {
      attempts: 1,
      completed_at: "2026-06-01T12:00:00.120Z",
      correlation_id: "corr_normal",
      created_at: "2026-06-01T12:00:00.000Z",
      id: "fn_create_user",
      max_attempts: 3,
      name: "identity.create_user",
      started_at: "2026-06-01T12:00:00.000Z",
      status: "completed",
      type: "function_run",
    },
  ],
};

describe("runtime API model normalization", () => {
  test("preserves backend story summary, nodes, edges, and timeline items", () => {
    const story = normalizeRuntimeStory(normalStory);

    expect(story).toMatchObject({
      correlationId: "corr_normal",
      durationMs: 500,
      id: "corr_normal",
      name: "CreateUser",
      status: "completed",
    });
    expect(story.nodes.map((node) => node.id)).toEqual([
      "fn_create_user",
      "evt_user_registered",
    ]);
    expect(story.nodes[1]).toMatchObject({
      canonicalName: "identity.user_registered.v1",
      name: "User Registered",
    });
    expect(story.edges).toEqual(normalStory.edges);
    expect(story.timelineItems?.map((item) => item.id)).toEqual([
      "fn_create_user",
    ]);
  });

  test("normalizes remote proxy calls as story graph and timeline nodes", () => {
    const story = normalizeRuntimeStory({
      ...normalStory,
      nodes: [
        ...(normalStory.nodes ?? []),
        {
          duration_ms: 42,
          id: "remoteproxy_rproxy_1",
          metadata: {
            source_metadata: {
              declared_path: "/contacts/{id}",
              duration_ms: 42,
              error_code: null,
              error_details: [],
              method: "GET",
              module_name: "remote-crm",
              path_params: { id: "contact_1" },
              remote_path: "/contacts/contact_1",
              remote_proxy_call_id: "rproxy_1",
              remote_status: 200,
              request_id: "req_remote_proxy",
              retryable: false,
              span_id: "span_remote_proxy",
              trace_id: "trace_remote_proxy",
            },
          },
          name: "Fetch Contact",
          service: "remote-crm",
          status: "completed",
          timestamp: "2026-06-01T12:00:00.180Z",
          type: "remote_proxy_call",
        },
      ],
      edges: [
        ...(normalStory.edges ?? []),
        {
          id: "fn_create_user:remoteproxy_rproxy_1:causation",
          source: "fn_create_user",
          target: "remoteproxy_rproxy_1",
          type: "causation",
        },
      ],
      timeline_items: [
        ...(normalStory.timeline_items ?? []),
        {
          attempts: 1,
          completed_at: "2026-06-01T12:00:00.222Z",
          correlation_id: "corr_normal",
          created_at: "2026-06-01T12:00:00.180Z",
          id: "remoteproxy_rproxy_1",
          max_attempts: 1,
          name: "Fetch Contact",
          started_at: "2026-06-01T12:00:00.180Z",
          status: "completed",
          type: "remote_proxy_call",
        },
      ],
    });

    const remoteNode = story.nodes.find(
      (node) => node.id === "remoteproxy_rproxy_1"
    );
    expect(remoteNode).toMatchObject({
      durationMs: 42,
      kind: "external",
      name: "Fetch Contact",
      parentId: "fn_create_user",
      service: "remote-crm",
      status: "completed",
    });
    expect(remoteNode?.attributes.source_metadata).toMatchObject({
      declared_path: "/contacts/{id}",
      duration_ms: 42,
      error_details: [],
      method: "GET",
      module_name: "remote-crm",
      path_params: { id: "contact_1" },
      remote_path: "/contacts/contact_1",
      remote_proxy_call_id: "rproxy_1",
      remote_status: 200,
      request_id: "req_remote_proxy",
      retryable: false,
      span_id: "span_remote_proxy",
      trace_id: "trace_remote_proxy",
    });
    expect(
      buildExecutionTimelineRows(story).find(
        (row) => row.id === "remoteproxy_rproxy_1"
      )
    ).toMatchObject({
      metaParts: ["ok", "remote-crm", "GET /contacts/{id}", "status 200"],
    });
    expect(buildRemoteProxyInspectorDetail(remoteNode!)).toMatchObject({
      errorDetails: [],
      pathParams: { id: "contact_1" },
      rows: expect.arrayContaining([
        ["request id", "req_remote_proxy"],
        ["trace id", "trace_remote_proxy"],
        ["span id", "span_remote_proxy"],
      ]),
    });
    expect(
      story.timelineItems?.find((item) => item.id === "remoteproxy_rproxy_1")
    ).toMatchObject({
      detailId: "remoteproxy_rproxy_1",
      name: "Fetch Contact",
      type: "remote_proxy_call",
    });
  });

  test("normalizes fan-out story edges without collapsing siblings", () => {
    const story = normalizeRuntimeStory({
      ...normalStory,
      summary: {
        ...normalStory.summary,
        correlation_id: "corr_fanout",
        title: "Resource Published Fan-out",
      },
      nodes: [
        {
          duration_ms: 100,
          id: "event",
          metadata: {},
          name: "ResourceVersionPublished",
          service: "outbox",
          status: "published",
          timestamp: "2026-06-01T10:00:01.400Z",
          type: "outbox_event",
        },
        ...["search", "cdn", "notifications"].map((id, index) => ({
          duration_ms: 1000 + index,
          id,
          metadata: {},
          name: id,
          service: id,
          status: "completed",
          timestamp: `2026-06-01T10:00:02.${index}00Z`,
          type: "function_run",
        })),
      ],
      edges: [
        {
          id: "event:search",
          source: "event",
          target: "search",
          type: "causation",
        },
        { id: "event:cdn", source: "event", target: "cdn", type: "causation" },
        {
          id: "event:notifications",
          source: "event",
          target: "notifications",
          type: "causation",
        },
      ],
      timeline_items: [],
    });

    expect(
      story.edges
        ?.filter((edge) => edge.source === "event")
        .map((edge) => edge.target)
        .sort()
    ).toEqual(["cdn", "notifications", "search"]);
  });

  test("keeps failed and dead retry metadata usable", () => {
    const story = normalizeRuntimeStory({
      ...normalStory,
      summary: {
        ...normalStory.summary,
        correlation_id: "corr_failed",
        root_error: "connect ETIMEDOUT",
        status: "dead",
      },
      nodes: [
        {
          duration_ms: -20,
          error: "connect ETIMEDOUT",
          id: "dead_fn",
          metadata: { attempts: 3, max_attempts: 3 },
          name: "SendWelcomeEmail",
          service: "notifications",
          status: "dead",
          timestamp: "2026-06-01T12:00:00.000Z",
          type: "function_run",
        },
      ],
      edges: [],
      timeline_items: [],
    });

    expect(story.status).toBe("dead");
    expect(story.nodes[0]).toMatchObject({
      attempts: 3,
      durationMs: 0,
      maxAttempts: 3,
      retryable: true,
      status: "dead",
    });
    expect(story.nodes[0]?.logs).toEqual(["connect ETIMEDOUT"]);
  });

  test("handles an empty backend story detail", () => {
    const story = normalizeRuntimeStory({
      summary: {
        correlation_id: "corr_empty",
        created_at: "2026-06-01T12:00:00.000Z",
        duration: 0,
        error_count: 0,
        node_count: 0,
        pattern: [],
        services: [],
        status: "completed",
        title: "Empty Story",
        updated_at: "2026-06-01T12:00:00.000Z",
      },
      nodes: [],
      edges: [],
      timeline_items: [],
    });

    expect(story.nodes).toEqual([]);
    expect(story.edges).toEqual([]);
    expect(story.timelineItems).toEqual([]);
    expect(story.service).toBe("runtime");
  });

  test("repairs malformed but valid story data", () => {
    const story = normalizeRuntimeStory({
      summary: {
        correlation_id: "corr_malformed",
        created_at: "not-a-date",
        duration: -1,
        status: "mysterious",
        title: "",
      },
      nodes: [
        {
          id: "duplicate",
          metadata: null,
          name: "",
          service: "",
          status: "strange",
          timestamp: "not-a-date",
          type: "database_write",
        },
        {
          duration_ms: 10,
          id: "duplicate",
          metadata: { causation_id: "missing" },
          name: "Second",
          service: "worker",
          status: "running",
          timestamp: "2026-06-01T12:00:00.010Z",
          type: "worker",
        },
      ],
      edges: [
        {
          id: "orphan",
          source: "missing",
          target: "duplicate",
          type: "causation",
        },
      ],
    });

    expect(story.durationMs).toBe(10);
    expect(story.timestamp).toBe("1970-01-01T00:00:00.000Z");
    expect(story.status).toBe("pending");
    expect(story.nodes.map((node) => node.id)).toEqual([
      "duplicate",
      "duplicate__2",
    ]);
    expect(story.nodes[0]).toMatchObject({
      durationMs: 0,
      kind: "runtime",
      name: "Runtime Work",
      service: "runtime",
      status: "pending",
    });
    expect(story.edges).toEqual([]);
  });

  test("preserves disconnected components and drops only orphan edges", () => {
    const story = normalizeRuntimeStory({
      ...normalStory,
      summary: {
        ...normalStory.summary,
        correlation_id: "corr_disconnected",
      },
      nodes: [
        { ...normalStory.nodes![0]!, id: "component_a" },
        { ...normalStory.nodes![1]!, id: "component_b" },
        {
          duration_ms: 40,
          id: "component_c",
          metadata: {},
          name: "cleanup",
          service: "runtime",
          status: "completed",
          timestamp: "2026-06-01T12:00:01.000Z",
          type: "function_run",
        },
      ],
      edges: [
        {
          id: "valid",
          source: "component_a",
          target: "component_b",
          type: "sequence",
        },
        {
          id: "invalid",
          source: "component_b",
          target: "missing",
          type: "sequence",
        },
      ],
      timeline_items: [],
    });

    expect(story.nodes.map((node) => node.id)).toEqual([
      "component_a",
      "component_b",
      "component_c",
    ]);
    expect(story.edges).toEqual([
      {
        id: "valid",
        source: "component_a",
        target: "component_b",
        type: "sequence",
      },
    ]);
  });

  test("normalizes story list items without detail payloads", () => {
    const story = normalizeRuntimeStoryListItem({
      correlation_id: "corr_list",
      created_at: "2026-06-01T12:00:00.000Z",
      duration: 125,
      error_count: 1,
      node_count: 3,
      pattern: ["function_run"],
      root_error: "boom",
      services: ["runtime"],
      status: "failed",
      title: "Listed Story",
      updated_at: "2026-06-01T12:00:00.125Z",
    });

    expect(story).toMatchObject({
      correlationId: "corr_list",
      durationMs: 125,
      name: "Listed Story",
      status: "failed",
    });
    expect(story.nodes).toHaveLength(3);
    expect(story.nodes[0]?.kind).toBe("function");
  });

  test("normalizes backend heatmap cells defensively", () => {
    const heatmap = normalizeRuntimeHeatmap({
      bucket_seconds: -60,
      data: [
        {
          avg_duration_ms: -10,
          bucket_end: "bad",
          bucket_start: "2026-06-01T12:00:00.000Z",
          dead_count: -1,
          error_count: 2,
          max_duration_ms: 100,
          node_type: "database",
          service: "",
          total_count: -5,
        },
      ],
      page: { limit: 20, next_created_before: "2026-06-01T11:00:00.000Z" },
    });

    expect(heatmap.bucketSeconds).toBe(300);
    expect(heatmap.page).toEqual({
      limit: 20,
      nextCreatedBefore: "2026-06-01T11:00:00.000Z",
    });
    expect(heatmap.cells).toEqual([
      {
        bucketEnd: "2026-06-01T12:00:00.000Z",
        bucketStart: "2026-06-01T12:00:00.000Z",
        deadCount: 0,
        errorCount: 2,
        maxDurationMs: 100,
        nodeType: "function",
        service: "runtime",
        totalCount: 0,
      },
    ]);
  });

  test("normalizes execution payload responses", () => {
    const payload = normalizeExecutionPayload({
      data: {
        input: { user_id: "usr_1" },
        metadata: { function_name: "notifications.send_welcome_email.v1" },
        output: null,
        redacted_fields: ["input.email"],
      },
    });

    expect(payload).toEqual({
      input: { user_id: "usr_1" },
      metadata: { function_name: "notifications.send_welcome_email.v1" },
      output: null,
      redactedFields: ["input.email"],
    });
  });

  test("preserves remote proxy technical operation source", () => {
    const operations = normalizeTechnicalOperations({
      data: [
        {
          attributes: {
            error_code: "external_dependency_failure",
            module_name: "remote-crm",
          },
          category: "external",
          correlation_id: "corr_1",
          duration_ms: 125,
          ended_at: "2026-06-01T12:00:01.125Z",
          id: "remote_proxy:rproxy_1",
          name: "Fetch Contact",
          related_node_id: null,
          source: "remote_proxy",
          started_at: "2026-06-01T12:00:01.000Z",
          status: "error",
          story_id: "corr_1",
        },
      ],
    });

    expect(operations[0]).toMatchObject({
      category: "external",
      id: "remote_proxy:rproxy_1",
      source: "remote_proxy",
      status: "error",
    });
  });

  test("normalizes execution log responses", () => {
    const logs = normalizeExecutionLogs({
      data: [
        {
          attributes: { attempt: 1 },
          body: "Function run started",
          correlation_id: "corr_1",
          execution_name: "notifications.send_welcome_email.v1",
          id: "elog_1",
          node_id: "fnrun_1",
          node_type: "function_run",
          occurred_at: "2026-06-01T12:00:01.000Z",
          redacted_fields: ["attributes.email"],
          service_name: "notifications",
          severity: "info",
          span_id: "span_1",
          story_id: "corr_1",
          trace_id: "trace_1",
        },
      ],
    });

    expect(logs).toEqual([
      {
        attributes: { attempt: 1 },
        body: "Function run started",
        correlationId: "corr_1",
        executionName: "notifications.send_welcome_email.v1",
        id: "elog_1",
        nodeId: "fnrun_1",
        nodeType: "function_run",
        occurredAt: "2026-06-01T12:00:01.000Z",
        redactedFields: ["attributes.email"],
        serviceName: "notifications",
        severity: "info",
        spanId: "span_1",
        storyId: "corr_1",
        traceId: "trace_1",
      },
    ]);
  });
});
