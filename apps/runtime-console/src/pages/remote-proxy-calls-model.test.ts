import { describe, expect, test } from "vitest";

import type { RuntimeRemoteProxyCall } from "../hooks/use-runtime-queries";
import {
  filterRemoteProxyCalls,
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
];

describe("remote proxy calls model", () => {
  test("filters by result and sorts newest first", () => {
    expect(
      filterRemoteProxyCalls(calls, { query: "", result: "failed" }).map(
        (call) => call.id
      )
    ).toEqual(["rpc_b", "rpc_c"]);
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
      avgDurationMs: 300,
      failed: 2,
      retryable: 1,
      success: 1,
      total: 3,
    });
  });

  test("deduplicates module names for filter controls", () => {
    expect(remoteProxyCallModules(calls)).toEqual([
      "remote-billing",
      "remote-crm",
    ]);
  });

  test("labels retryable failures distinctly", () => {
    expect(remoteProxyCallResultLabel(calls[1]!)).toBe("retryable");
    expect(remoteProxyCallResultLabel(calls[2]!)).toBe("failed");
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
