import { describe, expect, test } from "vitest";

import { remoteProxyCalls, runtimeStories } from "../data/mock-runtime";
import { queryDataWithMockFallback } from "./runtime-query-data";
import {
  filterRemoteProxyCallsForQuery,
  normalizeFunctionRunForConsole,
  type RuntimeRemoteProxyCall,
} from "./use-runtime-queries";

describe("runtime query data helpers", () => {
  test("does not show mock fallback while API data is still loading", () => {
    expect(
      queryDataWithMockFallback({
        apiMode: true,
        data: undefined,
        fallback: runtimeStories,
        isError: false,
      })
    ).toEqual([]);
  });

  test("uses mock fallback outside API mode", () => {
    expect(
      queryDataWithMockFallback({
        apiMode: false,
        data: undefined,
        fallback: runtimeStories,
        isError: false,
      })
    ).toBe(runtimeStories);
  });

  test("keeps mock fallback when API data is unavailable after an error", () => {
    expect(
      queryDataWithMockFallback({
        apiMode: true,
        data: undefined,
        fallback: runtimeStories,
        isError: true,
      })
    ).toBe(runtimeStories);
  });
});

describe("remote proxy call query helpers", () => {
  test("filters mock remote calls by story correlation", () => {
    const page = filterRemoteProxyCallsForQuery(remoteProxyCalls, {
      correlationId: "corr_resource_published_fanout",
      limit: 10,
    });

    expect(page.data.map((call) => call.id)).toEqual([
      "rpc_01J2REMOTE_OK_ACCOUNTS",
      "rpc_01J2REMOTE_FAIL_BILLING",
    ]);
    expect(
      page.data.every(
        (call) => call.correlation_id === "corr_resource_published_fanout"
      )
    ).toBe(true);
  });

  test("combines correlation filter with cursor pagination", () => {
    const page = filterRemoteProxyCallsForQuery(
      [
        remoteProxyCall({
          correlation_id: "corr_story",
          id: "newer",
          occurred_at: "2026-06-03T10:00:00.000Z",
        }),
        remoteProxyCall({
          correlation_id: "corr_story",
          id: "older",
          occurred_at: "2026-06-03T09:00:00.000Z",
        }),
        remoteProxyCall({
          correlation_id: "corr_other",
          id: "other",
          occurred_at: "2026-06-03T08:00:00.000Z",
        }),
      ],
      {
        correlationId: "corr_story",
        createdBefore: "2026-06-03T09:30:00.000Z",
        limit: 1,
      }
    );

    expect(page.data.map((call) => call.id)).toEqual(["older"]);
    expect(page.page.next_created_before).toBeNull();
  });
});

describe("runtime function query helpers", () => {
  test("normalizes runtime declaration metadata from API function runs", () => {
    expect(
      normalizeFunctionRunForConsole({
        attempts: 1,
        available_at: "2026-06-03T00:00:00.000Z",
        correlation_id: "corr_remote",
        created_at: "2026-06-03T00:00:00.000Z",
        function_name: "remote_crm.sync_contact.v1",
        id: "fn_remote",
        max_attempts: 3,
        runtime_declaration: {
          input_schema: "remote_crm.sync_contact.v1",
          module_name: "remote-crm",
          module_source: "remote",
          name: "remote_crm.sync_contact.v1",
          queue: "remote-crm",
          retry_policy: {
            initial_delay_ms: 1000,
            max_attempts: 3,
          },
          version: 1,
        },
        status: "completed",
      }).runtimeDeclaration
    ).toEqual({
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
    });
  });
});

function remoteProxyCall(
  overrides: Partial<RuntimeRemoteProxyCall>
): RuntimeRemoteProxyCall {
  return {
    capability: null,
    correlation_id: "corr_test",
    declared_path: "/resources/:id",
    duration_ms: 1,
    error_code: null,
    error_details: null,
    id: "rpc_test",
    method: "GET",
    module_name: "remote-test",
    occurred_at: "2026-06-03T00:00:00.000Z",
    path_params: {},
    remote_path: "/v1/resources/res_1",
    remote_status: 200,
    request_id: "req_test",
    retryable: false,
    span_id: null,
    success: true,
    trace_id: null,
    ...overrides,
  };
}
