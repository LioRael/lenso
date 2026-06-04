import type { RuntimeRemoteProxyCall } from "../hooks/use-runtime-queries";

export type RemoteProxyCallResultFilter = "all" | "success" | "failed";

export type RemoteProxyCallSummary = {
  total: number;
  success: number;
  failed: number;
  retryable: number;
  avgDurationMs: number;
};

export function remoteProxyCallModules(calls: RuntimeRemoteProxyCall[]) {
  return Array.from(new Set(calls.map((call) => call.module_name))).sort();
}

export function filterRemoteProxyCalls(
  calls: RuntimeRemoteProxyCall[],
  filters: {
    query: string;
    result: RemoteProxyCallResultFilter;
  }
) {
  const terms = filters.query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  return calls
    .filter((call) => {
      if (filters.result === "success") {
        return call.success;
      }
      if (filters.result === "failed") {
        return !call.success;
      }
      return true;
    })
    .filter((call) => {
      if (terms.length === 0) {
        return true;
      }
      const text = [
        call.id,
        call.module_name,
        call.method,
        call.declared_path,
        call.remote_path,
        call.capability ?? "",
        call.request_id,
        call.correlation_id,
        call.trace_id ?? "",
        call.error_code ?? "",
      ]
        .join(" ")
        .toLowerCase();
      return terms.every((term) => text.includes(term));
    })
    .sort((a, b) => b.occurred_at.localeCompare(a.occurred_at));
}

export function summarizeRemoteProxyCalls(
  calls: RuntimeRemoteProxyCall[]
): RemoteProxyCallSummary {
  const totalDuration = calls.reduce((sum, call) => sum + call.duration_ms, 0);
  const failed = calls.filter((call) => !call.success).length;
  return {
    total: calls.length,
    success: calls.length - failed,
    failed,
    retryable: calls.filter((call) => call.retryable).length,
    avgDurationMs:
      calls.length === 0 ? 0 : Math.round(totalDuration / calls.length),
  };
}

export function remoteProxyCallResultLabel(call: RuntimeRemoteProxyCall) {
  if (call.success) {
    return "success";
  }
  return call.retryable ? "retryable" : "failed";
}
