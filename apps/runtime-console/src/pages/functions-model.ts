import type { FunctionRun, RuntimeStatus } from "../data/mock-runtime";

export type FunctionStatusFilter = RuntimeStatus | "all";
export type FunctionAggregateGroup = "module" | "queue" | "status";

export type FunctionRunSummary = {
  total: number;
  pending: number;
  running: number;
  completed: number;
  failed: number;
  dead: number;
};

export type FunctionRunAggregate = {
  key: string;
  total: number;
  failed: number;
  dead: number;
  avgDurationMs: number;
};

export const functionStatusFilters: FunctionStatusFilter[] = [
  "all",
  "pending",
  "running",
  "completed",
  "failed",
  "dead",
];

export function filterFunctionRuns(
  runs: FunctionRun[],
  filters: {
    moduleName: string;
    query: string;
    queue: string;
    status: FunctionStatusFilter;
  }
) {
  const terms = filters.query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const moduleName = filters.moduleName.trim().toLowerCase();
  const queue = filters.queue.trim().toLowerCase();
  return runs
    .filter((run) =>
      filters.status === "all" ? true : run.status === filters.status
    )
    .filter((run) =>
      moduleName
        ? (run.runtimeDeclaration?.moduleName ?? "").toLowerCase() ===
          moduleName
        : true
    )
    .filter((run) =>
      queue
        ? (run.runtimeDeclaration?.queue ?? "").toLowerCase() === queue
        : true
    )
    .filter((run) => {
      if (terms.length === 0) {
        return true;
      }
      const text = [
        run.id,
        run.functionName,
        run.runtimeDeclaration?.moduleName ?? "",
        run.runtimeDeclaration?.moduleSource ?? "",
        run.runtimeDeclaration?.queue ?? "",
        run.runtimeDeclaration?.inputSchema ?? "",
        run.correlationId,
        run.lastError ?? "",
      ]
        .join(" ")
        .toLowerCase();
      return terms.every((term) => text.includes(term));
    })
    .sort((a, b) => b.createdAt.localeCompare(a.createdAt));
}

export function summarizeFunctionRuns(runs: FunctionRun[]): FunctionRunSummary {
  return {
    total: runs.length,
    pending: runs.filter((run) => run.status === "pending").length,
    running: runs.filter((run) => run.status === "running").length,
    completed: runs.filter((run) => run.status === "completed").length,
    failed: runs.filter((run) => run.status === "failed").length,
    dead: runs.filter((run) => run.status === "dead").length,
  };
}

export function aggregateFunctionRuns(
  runs: FunctionRun[],
  groupBy: FunctionAggregateGroup,
  limit: number
): FunctionRunAggregate[] {
  const groups = new Map<string, FunctionRun[]>();
  for (const run of runs) {
    const key = aggregateKey(run, groupBy);
    groups.set(key, [...(groups.get(key) ?? []), run]);
  }
  return Array.from(groups.entries())
    .map(([key, items]) => ({
      key,
      total: items.length,
      failed: items.filter((run) => run.status === "failed").length,
      dead: items.filter((run) => run.status === "dead").length,
      avgDurationMs: avgDuration(items),
    }))
    .sort(
      (a, b) =>
        b.dead - a.dead ||
        b.failed - a.failed ||
        b.total - a.total ||
        a.key.localeCompare(b.key)
    )
    .slice(0, limit);
}

export function distinctFunctionMetadata(
  runs: FunctionRun[],
  key: "module" | "queue"
) {
  return Array.from(
    new Set(
      runs
        .map((run) =>
          key === "module"
            ? run.runtimeDeclaration?.moduleName
            : run.runtimeDeclaration?.queue
        )
        .filter(Boolean) as string[]
    )
  ).sort();
}

export function runDurationMs(run: FunctionRun) {
  if (!run.startedAt) {
    return 0;
  }
  const end = run.completedAt ? Date.parse(run.completedAt) : Date.now();
  return Math.max(0, end - Date.parse(run.startedAt));
}

export function formatFunctionDuration(ms: number) {
  if (ms < 1000) {
    return `${ms}ms`;
  }
  return `${(ms / 1000).toFixed(1)}s`;
}

function aggregateKey(run: FunctionRun, groupBy: FunctionAggregateGroup) {
  if (groupBy === "module") {
    return run.runtimeDeclaration?.moduleName ?? "undeclared";
  }
  if (groupBy === "queue") {
    return run.runtimeDeclaration?.queue ?? "undeclared";
  }
  return run.status;
}

function avgDuration(runs: FunctionRun[]) {
  if (runs.length === 0) {
    return 0;
  }
  return Math.round(
    runs.reduce((sum, run) => sum + runDurationMs(run), 0) / runs.length
  );
}
