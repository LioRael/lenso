import type { RuntimeStatus, TraceRun, TraceSpan } from "../data/mock-runtime";

export type StoryNodeType =
  | "request"
  | "event"
  | "function"
  | "worker"
  | "external_provider";

export type StoryNode = {
  id: string;
  span: TraceSpan;
  type: StoryNodeType;
  typeLabel: string;
  title: string;
  service: string;
  status: RuntimeStatus;
  startMs: number;
  durationMs: number;
  error?: string;
};

export type RuntimeStory = {
  id: string;
  correlationId: string;
  title: string;
  status: RuntimeStatus;
  durationMs: number;
  spanCount: number;
  errorCount: number;
  nodes: StoryNode[];
};

export function buildRuntimeStory(trace: TraceRun): RuntimeStory {
  const nodes = trace.spans.flatMap((span) => {
    const type = storyNodeType(span);

    if (!type) {
      return [];
    }

    const error = storyNodeError(span);

    return [
      {
        ...(error ? { error } : {}),
        durationMs: span.durationMs,
        id: span.id,
        service: span.service,
        span,
        startMs: span.startMs,
        status: span.status,
        title: storyNodeTitle(span, type),
        type,
        typeLabel: storyNodeTypeLabel(type),
      },
    ];
  });

  return {
    correlationId: trace.correlationId,
    durationMs: trace.durationMs,
    errorCount: trace.spans.filter(isErrorStatus).length,
    id: trace.correlationId,
    nodes,
    spanCount: trace.spans.length,
    status: trace.status,
    title: storyTitle(trace, nodes),
  };
}

export function storyNodeType(span: TraceSpan): StoryNodeType | null {
  if (span.kind === "http") {
    return "request";
  }

  if (span.kind === "event") {
    return "event";
  }

  if (span.kind === "external") {
    return "external_provider";
  }

  if (span.kind === "function" || span.kind === "command") {
    return "function";
  }

  if (span.kind === "handler" || span.kind === "runtime") {
    return "worker";
  }

  return null;
}

export function storyNodeTypeLabel(type: StoryNodeType) {
  switch (type) {
    case "request": {
      return "Request";
    }
    case "event": {
      return "Event";
    }
    case "function": {
      return "Function";
    }
    case "worker": {
      return "Worker";
    }
    case "external_provider": {
      return "External Provider";
    }
    default: {
      const exhaustive: never = type;
      return exhaustive;
    }
  }
}

function storyTitle(trace: TraceRun, nodes: StoryNode[]) {
  const root = nodes[0]?.span ?? trace.spans[0];

  if (root?.kind === "http" && root.name.includes("/identity/users")) {
    return "User Registration";
  }

  if (trace.name.includes("object_uploaded")) {
    return "File Upload";
  }

  if (trace.name.includes("cleanup_expired_sessions")) {
    return "Session Cleanup";
  }

  return humanizeRuntimeName(trace.name);
}

function storyNodeTitle(span: TraceSpan, type: StoryNodeType) {
  if (type === "external_provider" && isErrorStatus(span)) {
    const message = storyNodeError(span);

    if (message?.toLowerCase().includes("smtp")) {
      return "smtp timeout";
    }
  }

  return humanizeRuntimeName(span.name);
}

function storyNodeError(span: TraceSpan) {
  if (!isErrorStatus(span)) {
    return undefined;
  }

  return span.logs.at(-1) ?? `${span.status} runtime work`;
}

function isErrorStatus(spanOrStatus: TraceSpan | RuntimeStatus) {
  const status =
    typeof spanOrStatus === "string" ? spanOrStatus : spanOrStatus.status;
  return status === "failed" || status === "dead";
}

function humanizeRuntimeName(value: string) {
  return value.replace(/\.v\d+$/u, "");
}
