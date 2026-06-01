import type { RuntimeStatus, TraceRun, TraceSpan } from "../data/mock-runtime";

const serviceColors = [
  "#f3f724",
  "#22c55e",
  "#3b82f6",
  "#a855f7",
  "#f97316",
  "#14b8a6",
  "#ec4899",
  "#94a3b8",
] as const;

export function formatTraceDuration(ms: number) {
  if (ms < 1) {
    return `${Math.round(ms * 1000)}us`;
  }
  if (ms < 1000) {
    return `${Math.round(ms)}ms`;
  }
  return `${(ms / 1000).toFixed(2)}s`;
}

export function statusColor(status: RuntimeStatus) {
  if (status === "failed" || status === "dead") {
    return "#ef4444";
  }
  if (status === "pending" || status === "processing" || status === "running") {
    return "#f3f724";
  }
  return "#22c55e";
}

export function serviceColor(service: string) {
  const hash = [...service].reduce(
    (total, char) => total + (char.codePointAt(0) ?? 0),
    0
  );
  return serviceColors[hash % serviceColors.length];
}

export function traceStats(trace: TraceRun) {
  const errors = trace.spans.filter(
    (span) => span.status === "failed" || span.status === "dead"
  );
  const services = Array.from(new Set(trace.spans.map((span) => span.service)));
  return {
    errors: errors.length,
    services,
    spanCount: trace.spans.length,
  };
}

export function traceTimelineEnd(trace: TraceRun) {
  const latestSpanEnd = Math.max(
    0,
    ...trace.spans.map((span) => span.startMs + span.durationMs)
  );
  return Math.max(trace.durationMs, latestSpanEnd, 1);
}

export function spanDepth(span: TraceSpan, spans: TraceSpan[]) {
  let depth = 0;
  let { parentId } = span;
  while (parentId) {
    const currentParentId = parentId;
    const parent = spans.find((item) => item.id === currentParentId);
    if (!parent) {
      break;
    }
    depth += 1;
    ({ parentId } = parent);
  }
  return depth;
}

export function criticalPath(trace: TraceRun) {
  const byParent = new Map<string | undefined, TraceSpan[]>();
  trace.spans.forEach((span) => {
    byParent.set(span.parentId, [...(byParent.get(span.parentId) ?? []), span]);
  });

  const path: TraceSpan[] = [];
  const roots = [...(byParent.get(undefined) ?? [])].sort(
    (left, right) => right.durationMs - left.durationMs
  );
  let [current] = roots;

  while (current) {
    path.push(current);
    const children = [...(byParent.get(current.id) ?? [])].sort(
      (left, right) => right.durationMs - left.durationMs
    );
    [current] = children;
  }

  return path;
}
