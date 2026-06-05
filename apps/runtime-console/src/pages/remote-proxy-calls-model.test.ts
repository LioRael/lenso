import { describe, expect, test } from "vitest";

import type { RuntimeRemoteProxyCall } from "../hooks/use-runtime-queries";
import {
  aggregateRemoteProxyCalls,
  filterRemoteProxyCalls,
  flattenRemoteProxyCallPages,
  nextRemoteProxyCallCursor,
  remoteProxyCallsPath,
  remoteProxyCallModules,
  remoteProxyCallResultLabel,
  summarizeRemoteProxyCalls,
} from "./remote-proxy-calls-model";

const calls = [
  remoteProxyCall({
    duration_ms: 100,
    id: "rpc_a",
    module_name: "remote-crm",
    occurred_at: "2026-06-03T10:00:00.000Z",
    success: true,
  }),
  remoteProxyCall({
    capability: "billing.invoices.create",
    duration_ms: 500,
    error_code: "remote_http_429",
    id: "rpc_b",
    module_name: "remote-billing",
    occurred_at: "2026-06-03T10:05:00.000Z",
    remote_status: 429,
    retryable: true,
    success: false,
  }),
  remoteProxyCall({
    duration_ms: 300,
    id: "rpc_c",
    module_name: "remote-crm",
    occurred_at: "2026-06-03T09:55:00.000Z",
    success: false,
  }),
  remoteProxyCall({
    duration_ms: 900,
    error_code: "remote_timeout",
    id: "rpc_d",
    module_name: "remote-crm",
    occurred_at: "2026-06-03T09:50:00.000Z",
    remote_status: null,
    retryable: true,
    success: false,
  }),
];

describe("remote proxy calls model", () => {
  test("filters by result and sorts newest first", () => {
    expect(
      filterRemoteProxyCalls(calls, { query: "", result: "failed" }).map(
        (call) => call.id
      )
    ).toEqual(["rpc_b", "rpc_c", "rpc_d"]);
  });

  test("searches operational identifiers and error fields", () => {
    expect(
      filterRemoteProxyCalls(calls, {
        query: "invoices 429",
        result: "all",
      }).map((call) => call.id)
    ).toEqual(["rpc_b"]);
  });

  test("summarizes result counts and average duration", () => {
    expect(summarizeRemoteProxyCalls(calls)).toEqual({
      avgDurationMs: 450,
      failed: 3,
      p95DurationMs: 900,
      retryable: 2,
      success: 1,
      total: 4,
    });
  });

  test("aggregates failure pressure by module", () => {
    expect(aggregateRemoteProxyCalls(calls, "module", 2)).toEqual([
      {
        failed: 2,
        failureRate: 2 / 3,
        key: "remote-crm",
        p95DurationMs: 900,
        total: 3,
      },
      {
        failed: 1,
        failureRate: 1,
        key: "remote-billing",
        p95DurationMs: 500,
        total: 1,
      },
    ]);
  });

  test("aggregates by error code and remote status", () => {
    expect(
      aggregateRemoteProxyCalls(calls, "error", 3).map((row) => row.key)
    ).toEqual(["remote_timeout", "remote_http_429", "unknown_error"]);
    expect(
      aggregateRemoteProxyCalls(calls, "status", 4).map((row) => row.key)
    ).toEqual(["no_status", "429", "200"]);
  });

  test("deduplicates module names for filter controls", () => {
    expect(remoteProxyCallModules(calls)).toEqual([
      "remote-billing",
      "remote-crm",
    ]);
  });

  test("flattens loaded cursor pages", () => {
    expect(
      flattenRemoteProxyCallPages([
        {
          data: [calls[0]!],
          page: {
            limit: 1,
            next_created_before: calls[0]!.occurred_at,
          },
        },
        {
          data: [calls[1]!],
          page: {
            limit: 1,
            next_created_before: null,
          },
        },
      ]).map((call) => call.id)
    ).toEqual(["rpc_a", "rpc_b"]);
  });

  test("reads the next cursor from the latest loaded page", () => {
    expect(
      nextRemoteProxyCallCursor([
        {
          data: [calls[0]!],
          page: {
            limit: 1,
            next_created_before: calls[0]!.occurred_at,
          },
        },
      ])
    ).toBe("2026-06-03T10:00:00.000Z");

    expect(nextRemoteProxyCallCursor(undefined)).toBeNull();
  });

  test("labels retryable failures distinctly", () => {
    expect(remoteProxyCallResultLabel(calls[1]!)).toBe("retryable");
    expect(remoteProxyCallResultLabel(calls[2]!)).toBe("failed");
  });

  test("builds correlation-scoped remote calls paths", () => {
    expect(remoteProxyCallsPath()).toBe("/operations/remote-calls");
    expect(
      remoteProxyCallsPath({
        correlationId: "corr_1",
        moduleName: "remote-crm",
        query: "contact",
        result: "failed",
        selectedId: "rpc_1",
      })
    ).toBe(
      "/operations/remote-calls?correlation_id=corr_1&module=remote-crm&q=contact&result=failed&selected=rpc_1"
    );
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
