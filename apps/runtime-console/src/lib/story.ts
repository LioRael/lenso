import type { RuntimeStatus, TraceRun, TraceSpan } from "../data/mock-runtime";

export type RuntimeNodeType =
  | "request"
  | "function"
  | "event"
  | "worker"
  | "external";

export type RuntimeNode = {
  id: string;
  type: RuntimeNodeType;
  typeLabel: string;
  name: string;
  status: RuntimeStatus;
  service: string;
  duration: number;
  timestamp: number;
  error?: string;
  parentId?: string;
  span: TraceSpan;
};

export type RuntimeStory = {
  id: string;
  title: string;
  correlationId: string;
  status: RuntimeStatus;
  duration: number;
  nodeCount: number;
  errorCount: number;
  services: string[];
  pattern: RuntimeNodeType[];
  patternLabel: string;
  rootError?: string;
  nodes: RuntimeNode[];
};

export function buildRuntimeStory(trace: TraceRun): RuntimeStory {
  const nodes = trace.spans.flatMap((span) => {
    const type = runtimeNodeType(span);

    if (!type) {
      return [];
    }

    const error = nodeError(span);

    return [
      {
        ...(error ? { error } : {}),
        ...(span.parentId ? { parentId: span.parentId } : {}),
        duration: span.durationMs,
        id: span.id,
        name: nodeName(span, type),
        service: span.service,
        span,
        status: span.status,
        timestamp: span.startMs,
        type,
        typeLabel: runtimeNodeTypeLabel(type),
      },
    ];
  });
  const services = Array.from(new Set(trace.spans.map((span) => span.service)));
  const pattern = collapsePattern(nodes.map((node) => node.type));
  const rootError = findRootError(nodes);

  return {
    ...(rootError ? { rootError } : {}),
    correlationId: trace.correlationId,
    duration: trace.durationMs,
    errorCount: trace.spans.filter(isErrorStatus).length,
    id: trace.correlationId,
    nodeCount: nodes.length,
    nodes,
    pattern,
    patternLabel: pattern.map(runtimeNodeTypeLabel).join(" -> "),
    services,
    status: trace.status,
    title: storyTitle(trace, nodes),
  };
}

export function runtimeNodeType(span: TraceSpan): RuntimeNodeType | null {
  if (span.kind === "http") {
    return "request";
  }

  if (span.kind === "command" || span.kind === "function") {
    return "function";
  }

  if (span.kind === "event") {
    return "event";
  }

  if (span.kind === "handler" || span.kind === "runtime") {
    return "worker";
  }

  if (span.kind === "external") {
    return "external";
  }

  return null;
}

export function runtimeNodeTypeLabel(type: RuntimeNodeType) {
  switch (type) {
    case "request": {
      return "Request";
    }
    case "function": {
      return "Function";
    }
    case "event": {
      return "Event";
    }
    case "worker": {
      return "Worker";
    }
    case "external": {
      return "External";
    }
    default: {
      const exhaustive: never = type;
      return exhaustive;
    }
  }
}

export function runtimeStatusIntent(status: RuntimeStatus) {
  if (status === "dead") {
    return "dead";
  }

  if (status === "failed") {
    return "failed";
  }

  if (status === "pending" || status === "processing") {
    return "retrying";
  }

  if (status === "running") {
    return "running";
  }

  return "success";
}

function storyTitle(trace: TraceRun, nodes: RuntimeNode[]) {
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

function nodeName(span: TraceSpan, type: RuntimeNodeType) {
  if (type === "external" && isErrorStatus(span)) {
    const error = nodeError(span);

    if (error?.toLowerCase().includes("smtp")) {
      return "smtp.provider";
    }
  }

  return humanizeRuntimeName(span.name);
}

function nodeError(span: TraceSpan) {
  if (!isErrorStatus(span)) {
    return undefined;
  }

  const finalLog = span.logs.at(-1);

  if (finalLog?.toLowerCase().includes("smtp")) {
    return "smtp timeout";
  }

  if (finalLog) {
    return finalLog;
  }

  return `${span.status} runtime work`;
}

function findRootError(nodes: RuntimeNode[]) {
  for (let index = nodes.length - 1; index >= 0; index -= 1) {
    const node = nodes[index];
    if (node?.error) {
      return node.error;
    }
  }

  return undefined;
}

function collapsePattern(types: RuntimeNodeType[]) {
  return types.filter(
    (type, index) => index === 0 || types[index - 1] !== type
  );
}

function isErrorStatus(spanOrStatus: TraceSpan | RuntimeStatus) {
  const status =
    typeof spanOrStatus === "string" ? spanOrStatus : spanOrStatus.status;
  return status === "failed" || status === "dead";
}

function humanizeRuntimeName(value: string) {
  return value.replace(/\.v\d+$/u, "");
}
