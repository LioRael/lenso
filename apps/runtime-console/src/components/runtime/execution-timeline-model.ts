import type {
  ExecutionNode,
  RuntimeStatus,
  RuntimeStory,
  TimelineItem,
} from "../../data/mock-runtime";

export type ExecutionTimelineRow = {
  id: string;
  name: string;
  kind: TimelineItem["type"] | ExecutionNode["kind"];
  status: RuntimeStatus;
  attempts?: number;
  maxAttempts?: number;
  service: string;
  startMs: number;
  durationMs: number;
  error?: string;
  metaParts: string[];
  node?: ExecutionNode;
  source: "backend" | "node";
};

export function buildExecutionTimelineRows(
  story: RuntimeStory
): ExecutionTimelineRow[] {
  if (story.timelineItems !== undefined) {
    return story.timelineItems.map((item, index) =>
      rowFromTimelineItem(story, item, index)
    );
  }

  return story.nodes.map(rowFromExecutionNode);
}

export function findExecutionNodeForRow(
  story: RuntimeStory,
  row: ExecutionTimelineRow
) {
  if (row.node) {
    return row.node;
  }

  return (
    story.nodes.find((node) => node.id === row.id) ??
    story.nodes.find((node) => node.id === row.id.replace(/^timeline:/, "")) ??
    null
  );
}

export function executionTimelineEnd(story: RuntimeStory) {
  const rows = buildExecutionTimelineRows(story);
  const latestRowEnd = Math.max(
    0,
    ...rows.map((row) => row.startMs + row.durationMs)
  );
  return Math.max(story.durationMs, latestRowEnd, 1);
}

function rowFromTimelineItem(
  story: RuntimeStory,
  item: TimelineItem,
  index: number
): ExecutionTimelineRow {
  const node =
    story.nodes.find((candidate) => candidate.id === item.detailId) ??
    story.nodes.find((candidate) => candidate.id === item.id);
  const startMs = item.startedAt
    ? offsetMs(story.timestamp, item.startedAt, index)
    : offsetMs(story.timestamp, item.createdAt, index);
  const endMs = item.completedAt
    ? offsetMs(story.timestamp, item.completedAt, index)
    : startMs + (node?.durationMs ?? 1);

  return {
    ...(item.lastError ? { error: item.lastError } : {}),
    ...(node ? { node } : {}),
    attempts: item.attempts,
    durationMs: Math.max(0, endMs - startMs),
    id: item.id,
    kind: item.type,
    maxAttempts: item.maxAttempts,
    metaParts: timelineRowMetaParts({
      kind: item.type,
      ...(node ? { node } : {}),
      service: node?.service ?? serviceFromTimelineType(item.type),
      status: item.status,
    }),
    name: item.name,
    service: node?.service ?? serviceFromTimelineType(item.type),
    source: "backend",
    startMs,
    status: item.status,
  };
}

function rowFromExecutionNode(node: ExecutionNode): ExecutionTimelineRow {
  const error = node.logs.at(-1);

  return {
    ...(error ? { error } : {}),
    ...(node.attempts === undefined ? {} : { attempts: node.attempts }),
    ...(node.maxAttempts === undefined
      ? {}
      : { maxAttempts: node.maxAttempts }),
    durationMs: node.durationMs,
    id: node.id,
    kind: node.kind,
    metaParts: timelineRowMetaParts({
      kind: node.kind,
      node,
      service: node.service,
      status: node.status,
    }),
    name: node.name,
    node,
    service: node.service,
    source: "node",
    startMs: node.startMs,
    status: node.status,
  };
}

function offsetMs(baseTimestamp: string, timestamp: string, fallback: number) {
  const base = Date.parse(baseTimestamp);
  const value = Date.parse(timestamp);
  if (Number.isFinite(base) && Number.isFinite(value)) {
    return Math.max(0, value - base);
  }
  return fallback;
}

function serviceFromTimelineType(type: TimelineItem["type"]) {
  if (type === "outbox_event") {
    return "outbox";
  }
  if (type === "function_run") {
    return "runtime.functions";
  }
  if (type === "http_request") {
    return "http";
  }
  return "runtime";
}

function timelineRowMetaParts(input: {
  kind: TimelineItem["type"] | ExecutionNode["kind"];
  node?: ExecutionNode;
  service: string;
  status: RuntimeStatus;
}) {
  const metadata = objectRecord(input.node?.attributes.source_metadata);
  if (input.kind === "remote_proxy_call" || isRemoteProxyMetadata(metadata)) {
    return remoteProxyMetaParts(input);
  }

  return [input.service];
}

function remoteProxyMetaParts(input: {
  node?: ExecutionNode;
  service: string;
  status: RuntimeStatus;
}) {
  const metadata = objectRecord(input.node?.attributes.source_metadata);
  const moduleName = stringValue(metadata.module_name) ?? input.service;
  const method = stringValue(metadata.method);
  const declaredPath = stringValue(metadata.declared_path);
  const remoteStatus = numberValue(metadata.remote_status);
  const route = [method, declaredPath].filter(Boolean).join(" ");
  const result = remoteProxyResultLabel(input.status, metadata);

  return [
    result,
    moduleName,
    route || undefined,
    typeof remoteStatus === "number" ? `status ${remoteStatus}` : undefined,
  ].filter((part): part is string => part !== undefined);
}

function isRemoteProxyMetadata(metadata: Record<string, unknown>) {
  if (typeof metadata.remote_proxy_call_id === "string") {
    return true;
  }

  return (
    typeof metadata.module_name === "string" &&
    typeof metadata.method === "string" &&
    typeof metadata.declared_path === "string"
  );
}

function remoteProxyResultLabel(
  status: RuntimeStatus,
  metadata: Record<string, unknown>
) {
  if (status === "failed" || status === "dead") {
    return booleanValue(metadata.retryable) ? "retryable" : "failed";
  }
  if (status === "completed" || status === "published") {
    return "ok";
  }
  return status;
}

function objectRecord(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function stringValue(value: unknown) {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function numberValue(value: unknown) {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : undefined;
}

function booleanValue(value: unknown) {
  return typeof value === "boolean" ? value : false;
}
