import type {
  RuntimeAdminActionInvocation,
  RuntimeAdminActionInvocationPage,
} from "../hooks/use-runtime-queries";
import { operationsPath } from "./operations-url-model";

export type AdminActionResultFilter = "all" | "success" | "failed";

export type AdminActionSummary = {
  total: number;
  success: number;
  failed: number;
  avgDurationMs: number;
};

export type AdminActionAggregate = {
  key: string;
  total: number;
  failed: number;
  failureRate: number;
};

export type AdminActionInspectorDetails = {
  actionRows: Array<[string, string]>;
  lineageRows: Array<[string, string]>;
  summaries: {
    input_summary: string | null;
    result_summary: string | null;
  };
  failure: {
    error_code: string | null;
    error_message: string | null;
  } | null;
};

export function adminActionsPath(
  filters: {
    actionName?: string;
    capability?: string;
    correlationId?: string;
    moduleName?: string;
    query?: string;
    result?: AdminActionResultFilter;
    selectedId?: string;
  } = {}
) {
  return operationsPath("/operations/admin-actions", {
    action: filters.actionName,
    capability: filters.capability,
    correlation_id: filters.correlationId,
    module: filters.moduleName,
    q: filters.query,
    result: filters.result === "all" ? undefined : filters.result,
    selected: filters.selectedId,
  });
}

export function flattenAdminActionInvocationPages(
  pages: RuntimeAdminActionInvocationPage[] | undefined
) {
  return pages?.flatMap((page) => page.data) ?? [];
}

export function nextAdminActionInvocationCursor(
  pages: RuntimeAdminActionInvocationPage[] | undefined
) {
  return pages?.at(-1)?.page.next_created_before ?? null;
}

export function filterAdminActionInvocations(
  actions: RuntimeAdminActionInvocation[],
  filters: {
    query: string;
    result: AdminActionResultFilter;
  }
) {
  const terms = filters.query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  return actions
    .filter((action) => {
      if (filters.result === "success") {
        return action.success;
      }
      if (filters.result === "failed") {
        return !action.success;
      }
      return true;
    })
    .filter((action) => {
      if (terms.length === 0) {
        return true;
      }
      const text = [
        action.id,
        action.module_name,
        action.action_name,
        action.label,
        action.capability ?? "",
        action.request_id ?? "",
        action.correlation_id,
        action.trace_id ?? "",
        action.error_code ?? "",
        action.input_summary ?? "",
        action.result_summary ?? "",
      ]
        .join(" ")
        .toLowerCase();
      return terms.every((term) => text.includes(term));
    })
    .sort((a, b) => b.occurred_at.localeCompare(a.occurred_at));
}

export function summarizeAdminActionInvocations(
  actions: RuntimeAdminActionInvocation[]
): AdminActionSummary {
  const totalDuration = actions.reduce(
    (sum, action) => sum + action.duration_ms,
    0
  );
  const failed = actions.filter((action) => !action.success).length;
  return {
    avgDurationMs:
      actions.length === 0 ? 0 : Math.round(totalDuration / actions.length),
    failed,
    success: actions.length - failed,
    total: actions.length,
  };
}

export function aggregateAdminActionInvocations(
  actions: RuntimeAdminActionInvocation[],
  groupBy: "module" | "action" | "capability" | "error",
  limit = 5
): AdminActionAggregate[] {
  const groups = new Map<string, RuntimeAdminActionInvocation[]>();
  for (const action of actions) {
    const key = aggregateKey(action, groupBy);
    groups.set(key, [...(groups.get(key) ?? []), action]);
  }

  return Array.from(groups.entries())
    .map(([key, items]) => {
      const failed = items.filter((action) => !action.success).length;
      return {
        failed,
        failureRate: items.length === 0 ? 0 : failed / items.length,
        key,
        total: items.length,
      };
    })
    .sort(
      (a, b) =>
        b.failed - a.failed || b.total - a.total || a.key.localeCompare(b.key)
    )
    .slice(0, limit);
}

export function adminActionResultLabel(action: RuntimeAdminActionInvocation) {
  return action.success ? "success" : "failed";
}

export function adminActionPrimarySummary(
  action: RuntimeAdminActionInvocation
) {
  if (!action.success) {
    return action.error_message ?? action.error_code ?? "failed";
  }
  return action.result_summary ?? action.input_summary ?? action.label;
}

export function adminActionInspectorDetails(
  action: RuntimeAdminActionInvocation
): AdminActionInspectorDetails {
  return {
    actionRows: [
      ["result", adminActionResultLabel(action)],
      ["module", action.module_name],
      ["action", action.action_name],
      ["label", action.label],
      ["capability", action.capability ?? "-"],
      ["duration_ms", String(action.duration_ms)],
      ["occurred", action.occurred_at],
    ],
    failure: action.success
      ? null
      : {
          error_code: action.error_code ?? null,
          error_message: action.error_message ?? null,
        },
    lineageRows: [
      ["story_node", action.id],
      ["request", action.request_id ?? "-"],
      ["correlation", action.correlation_id],
      ["trace", action.trace_id ?? "-"],
      ["span", action.span_id ?? "-"],
    ],
    summaries: {
      input_summary: action.input_summary ?? null,
      result_summary: action.result_summary ?? null,
    },
  };
}

function aggregateKey(
  action: RuntimeAdminActionInvocation,
  groupBy: "module" | "action" | "capability" | "error"
) {
  if (groupBy === "module") {
    return action.module_name;
  }
  if (groupBy === "action") {
    return action.action_name;
  }
  if (groupBy === "capability") {
    return action.capability ?? "none";
  }
  return action.error_code ?? (action.success ? "success" : "unknown_error");
}
