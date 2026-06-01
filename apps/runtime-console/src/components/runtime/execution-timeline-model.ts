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
