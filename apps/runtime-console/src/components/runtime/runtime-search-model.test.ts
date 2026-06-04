import { describe, expect, test } from "vitest";

import type {
  FunctionRun,
  RuntimeEvent,
  RuntimeStory,
} from "../../data/mock-runtime";
import { buildRuntimeSearchResults } from "./runtime-search-model";

const stories = [
  {
    correlationId: "corr_backend",
    durationMs: 10,
    id: "corr_backend",
    name: "Backend Story",
    nodes: [
      {
        attributes: {},
        context: {},
        durationMs: 10,
        events: [],
        id: "fn_backend",
        kind: "function",
        logs: [],
        name: "SendBackendEmail",
        service: "notifications",
        startMs: 0,
        status: "completed",
      },
    ],
    service: "notifications",
    source: "runtime-story",
    status: "completed",
    timestamp: "2026-06-01T00:00:00.000Z",
  },
] satisfies RuntimeStory[];

const events = [
  {
    actor: { kind: "system" },
    aggregateId: "user_1",
    aggregateType: "user",
    attempts: 1,
    causationId: "fn_backend",
    correlationId: "corr_event",
    createdAt: "2026-06-01T00:00:00.000Z",
    eventName: "identity.user_registered",
    id: "evt_backend",
    maxAttempts: 3,
    payload: {},
    status: "failed",
  },
] satisfies RuntimeEvent[];

const functions = [
  {
    actor: { kind: "system" },
    attempts: 2,
    correlationId: "corr_function",
    createdAt: "2026-06-01T00:00:00.000Z",
    functionName: "notifications.retry_email",
    id: "fn_retry",
    input: {},
    logs: [],
    maxAttempts: 3,
    status: "running",
  },
] satisfies FunctionRun[];

const declaredFunctions = [
  {
    actor: { kind: "system" },
    attempts: 2,
    correlationId: "corr_function",
    createdAt: "2026-06-01T00:00:00.000Z",
    functionName: "notifications.retry_email",
    id: "fn_retry",
    input: {},
    logs: [],
    maxAttempts: 3,
    runtimeDeclaration: {
      inputSchema: "remote_crm.sync_contact.v1",
      moduleName: "remote-crm",
      moduleSource: "remote",
      name: "remote_crm.sync_contact.v1",
      queue: "remote-crm",
      retryPolicy: {
        initialDelayMs: 1000,
        maxAttempts: 3,
      },
      version: 1,
    },
    status: "running",
  },
] satisfies FunctionRun[];

describe("runtime search model", () => {
  test("searches query-backed stories by summary fields", () => {
    const results = buildRuntimeSearchResults({
      events: [],
      functions: [],
      query: "corr_backend",
      stories,
    });

    expect(results[0]).toMatchObject({
      kind: "story",
      correlationId: "corr_backend",
      storyId: "corr_backend",
      title: "Backend Story",
    });
  });

  test("searches query-backed stories by execution node names", () => {
    const results = buildRuntimeSearchResults({
      events: [],
      functions: [],
      query: "SendBackendEmail",
      stories,
    });

    expect(results[0]).toMatchObject({
      kind: "story",
      id: "fn_backend",
      nodeId: "fn_backend",
      title: "SendBackendEmail",
    });
  });

  test("searches real event and function names without static story data", () => {
    const results = buildRuntimeSearchResults({
      events,
      functions,
      query: "retry_email",
      stories: [],
    });

    expect(results.map((result) => result.kind)).toEqual(["function"]);
    expect(results[0]).toMatchObject({
      correlationId: "corr_function",
      title: "notifications.retry_email",
    });
  });

  test("includes story, event, and function correlations", () => {
    const results = buildRuntimeSearchResults({
      events,
      functions,
      query: "corr_",
      stories,
    });

    expect(results.filter((result) => result.kind === "correlation")).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ correlationId: "corr_backend" }),
        expect.objectContaining({ correlationId: "corr_event" }),
        expect.objectContaining({ correlationId: "corr_function" }),
      ])
    );
  });

  test("searches function declaration metadata", () => {
    const results = buildRuntimeSearchResults({
      events: [],
      functions: declaredFunctions,
      query: "remote-crm",
      stories: [],
    });

    expect(results).toEqual([
      expect.objectContaining({
        kind: "function",
        subtitle: "running · remote-crm",
        title: "notifications.retry_email",
      }),
    ]);
  });
});
