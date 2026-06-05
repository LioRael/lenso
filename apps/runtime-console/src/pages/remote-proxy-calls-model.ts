import type {
  RuntimeRemoteProxyCall,
  RuntimeRemoteProxyCallPage,
} from "../hooks/use-runtime-queries";

export type RemoteProxyCallResultFilter = "all" | "success" | "failed";

export type RemoteProxyCallSummary = {
  total: number;
  success: number;
  failed: number;
  retryable: number;
  avgDurationMs: number;
  p95DurationMs: number;
};

export type RemoteProxyCallAggregate = {
  key: string;
  total: number;
  failed: number;
  failureRate: number;
  p95DurationMs: number;
};

export function remoteProxyCallsPath(filters: { correlationId?: string } = {}) {
  const params = new URLSearchParams();
  if (filters.correlationId) {
    params.set("correlation_id", filters.correlationId);
  }
  const search = params.toString();
  return search
    ? `/operations/remote-calls?${search}`
    : "/operations/remote-calls";
}

export function remoteProxyCallModules(calls: RuntimeRemoteProxyCall[]) {
  return Array.from(new Set(calls.map((call) => call.module_name))).sort();
}

export function flattenRemoteProxyCallPages(
  pages: RuntimeRemoteProxyCallPage[] | undefined
) {
  return pages?.flatMap((page) => page.data) ?? [];
}

export function nextRemoteProxyCallCursor(
  pages: RuntimeRemoteProxyCallPage[] | undefined
) {
  return pages?.at(-1)?.page.next_created_before ?? null;
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
    p95DurationMs: percentileDuration(calls, 0.95),
  };
}

export function aggregateRemoteProxyCalls(
  calls: RuntimeRemoteProxyCall[],
  groupBy: "module" | "error" | "status",
  limit = 5
): RemoteProxyCallAggregate[] {
  const groups = new Map<string, RuntimeRemoteProxyCall[]>();
  for (const call of calls) {
    const key = aggregateKey(call, groupBy);
    groups.set(key, [...(groups.get(key) ?? []), call]);
  }

  return Array.from(groups.entries())
    .map(([key, items]) => {
      const failed = items.filter((call) => !call.success).length;
      return {
        key,
        total: items.length,
        failed,
        failureRate: items.length === 0 ? 0 : failed / items.length,
        p95DurationMs: percentileDuration(items, 0.95),
      };
    })
    .sort(
      (a, b) =>
        b.failed - a.failed ||
        b.p95DurationMs - a.p95DurationMs ||
        b.total - a.total ||
        a.key.localeCompare(b.key)
    )
    .slice(0, limit);
}

export function remoteProxyCallResultLabel(call: RuntimeRemoteProxyCall) {
  if (call.success) {
    return "success";
  }
  return call.retryable ? "retryable" : "failed";
}

function aggregateKey(
  call: RuntimeRemoteProxyCall,
  groupBy: "module" | "error" | "status"
) {
  if (groupBy === "module") {
    return call.module_name;
  }
  if (groupBy === "error") {
    return call.error_code ?? (call.success ? "success" : "unknown_error");
  }
  return call.remote_status === null || call.remote_status === undefined
    ? "no_status"
    : String(call.remote_status);
}

function percentileDuration(
  calls: RuntimeRemoteProxyCall[],
  percentile: number
) {
  if (calls.length === 0) {
    return 0;
  }
  const durations = calls.map((call) => call.duration_ms).sort((a, b) => a - b);
  const index = Math.max(0, Math.ceil(percentile * durations.length) - 1);
  return durations[index] ?? 0;
}
