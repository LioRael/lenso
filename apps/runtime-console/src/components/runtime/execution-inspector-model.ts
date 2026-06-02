import type { ExecutionNode, RuntimeStory } from "../../data/mock-runtime";

export type ExecutionInspectorTab =
  | "overview"
  | "payload"
  | "activity"
  | "failures"
  | "logs"
  | "context"
  | "technical";

export type ExecutionActivityItem = {
  id: string;
  kind: string;
  label: string;
  status: string;
  timestampMs: number;
  detail?: string;
};

export type ExecutionFailureItem = {
  id: string;
  label: string;
  value: string;
};

export type ExecutionContextModel = {
  rows: Array<[string, unknown]>;
  upstream: ExecutionNode[];
  downstream: ExecutionNode[];
};

export type ExecutionPayloadModel = {
  input?: unknown;
  output?: unknown;
  metadata?: unknown;
};

export const executionInspectorTabs = [
  { id: "overview", label: "Overview" },
  { id: "payload", label: "Payload" },
  { id: "activity", label: "Activity" },
  { id: "logs", label: "Logs" },
  { id: "failures", label: "Failures" },
  { id: "context", label: "Context" },
  { id: "technical", label: "Technical" },
] as const satisfies ReadonlyArray<{
  id: ExecutionInspectorTab;
  label: string;
}>;

export function buildExecutionPayload(
  node: ExecutionNode
): ExecutionPayloadModel {
  const rawPayload = node.payload;
  const payload = isRecord(rawPayload) ? rawPayload : undefined;
  const input = firstValue(payload, [
    "input",
    "input_json",
    "inputJson",
    "request",
    "request_json",
    "requestJson",
    "request_payload",
    "requestPayload",
    "payload",
  ]);
  const output = firstValue(payload, [
    "output",
    "output_json",
    "outputJson",
    "response",
    "response_json",
    "responseJson",
    "response_payload",
    "responsePayload",
  ]);
  const metadata = firstValue(payload, ["metadata", "meta", "headers"]);
  const hasStructuredPayload = input !== undefined || output !== undefined;

  return {
    input: input ?? (payload && !hasStructuredPayload ? payload : undefined),
    metadata:
      metadata ??
      (payload && Object.keys(node.attributes).length > 0
        ? node.attributes
        : undefined),
    output,
  };
}

export function defaultExecutionInspectorTab(
  node: ExecutionNode
): ExecutionInspectorTab {
  if (node.status === "failed" || node.status === "dead") {
    return "failures";
  }
  return hasExecutionPayloadData(node) ? "payload" : "overview";
}

export function buildExecutionActivity(
  story: RuntimeStory,
  node: ExecutionNode
): ExecutionActivityItem[] {
  const timelineActivity =
    story.timelineItems
      ?.filter((item) => item.detailId === node.id || item.id === node.id)
      .map((item) => ({
        id: item.id,
        kind: item.type,
        label: item.name,
        status: item.status,
        timestampMs: offsetMs(
          story.timestamp,
          item.startedAt ?? item.createdAt
        ),
        ...(item.attempts > 1
          ? { detail: `attempt ${item.attempts}/${item.maxAttempts}` }
          : {}),
      })) ?? [];
  const nodeActivity = node.events.map((event) => ({
    id: `${node.id}:${event.name}:${event.timestampMs}`,
    kind: "event",
    label: event.name,
    status: node.status,
    timestampMs: event.timestampMs,
    ...(event.attributes ? { detail: JSON.stringify(event.attributes) } : {}),
  }));

  return [...timelineActivity, ...nodeActivity].sort(
    (left, right) =>
      left.timestampMs - right.timestampMs ||
      left.label.localeCompare(right.label)
  );
}

export function buildExecutionFailures(
  node: ExecutionNode
): ExecutionFailureItem[] {
  const isFailure = node.status === "failed" || node.status === "dead";
  if (!isFailure) {
    return [];
  }

  const lastLog = node.logs.at(-1);
  const failures: ExecutionFailureItem[] = [];

  if (isFailure) {
    failures.push({
      id: "current-error",
      label: "current error",
      value: lastLog ?? `${node.status} execution`,
    });
  }

  if (lastLog) {
    failures.push({
      id: "last-error",
      label: "last error",
      value: lastLog,
    });
  }

  if ((node.attempts ?? 1) > 1 || node.retryable) {
    failures.push({
      id: "retry-history",
      label: "retry history",
      value: `${node.attempts ?? 1}/${node.maxAttempts ?? 1} attempts`,
    });
  }

  return failures;
}

export function buildExecutionContext(
  story: RuntimeStory,
  node: ExecutionNode
): ExecutionContextModel {
  const upstream = relatedNodes(story, node, "upstream");
  const downstream = relatedNodes(story, node, "downstream");
  const causationId =
    typeof node.context.causation_id === "string"
      ? node.context.causation_id
      : node.parentId;
  const actor =
    node.context.actor ??
    upstream.find((candidate) => candidate.context.actor)?.context.actor ??
    story.service;
  const aggregate =
    node.attributes.aggregate_id ??
    node.attributes.aggregateId ??
    node.payload?.aggregate_id ??
    node.payload?.aggregateId;
  const triggerSource =
    node.context.trigger_source ??
    node.attributes.trigger_source ??
    node.attributes.triggerSource ??
    node.kind;

  return {
    downstream,
    rows: [
      ["correlation id", story.correlationId],
      ["causation id", causationId ?? "-"],
      ["actor", actor ?? "-"],
      ["aggregate", aggregate ?? "-"],
      ["trigger source", triggerSource ?? "-"],
      ["related executions", upstream.length + downstream.length],
    ],
    upstream,
  };
}

export function getExecutionInspectorTabCounts(
  story: RuntimeStory,
  node: ExecutionNode
): Record<ExecutionInspectorTab, number> {
  return {
    activity: buildExecutionActivity(story, node).length,
    context:
      relatedNodes(story, node, "upstream").length +
      relatedNodes(story, node, "downstream").length,
    failures: buildExecutionFailures(node).length,
    logs: node.logs.length,
    overview: 0,
    payload: payloadCount(node),
    technical: 0,
  };
}

function payloadCount(node: ExecutionNode) {
  const payload = buildExecutionPayload(node);
  return [payload.input, payload.metadata, payload.output].filter((value) =>
    hasMeaningfulValue(value)
  ).length;
}

function hasExecutionPayloadData(node: ExecutionNode) {
  const payload = buildExecutionPayload(node);
  return (
    hasMeaningfulValue(payload.input) || hasMeaningfulValue(payload.output)
  );
}

function firstValue(
  payload: Record<string, unknown> | undefined,
  keys: string[]
) {
  if (!payload) {
    return undefined;
  }
  for (const key of keys) {
    if (payload[key] !== undefined) {
      return payload[key];
    }
  }
  return undefined;
}

function hasMeaningfulValue(value: unknown) {
  if (value === undefined || value === null) {
    return false;
  }
  if (Array.isArray(value)) {
    return value.length > 0;
  }
  if (isRecord(value)) {
    return Object.keys(value).length > 0;
  }
  return true;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === "object" && !Array.isArray(value));
}

function relatedNodes(
  story: RuntimeStory,
  node: ExecutionNode,
  direction: "upstream" | "downstream"
) {
  const edges = story.edges ?? [];
  const relatedIds = edges
    .filter((edge) =>
      direction === "upstream"
        ? edge.target === node.id
        : edge.source === node.id
    )
    .map((edge) => (direction === "upstream" ? edge.source : edge.target));
  const parentId = direction === "upstream" ? node.parentId : undefined;
  const childIds =
    direction === "downstream"
      ? story.nodes
          .filter((candidate) => candidate.parentId === node.id)
          .map((candidate) => candidate.id)
      : [];
  const ids = new Set([
    ...relatedIds,
    ...(parentId ? [parentId] : []),
    ...childIds,
  ]);

  return story.nodes.filter((candidate) => ids.has(candidate.id));
}

function offsetMs(baseTimestamp: string, timestamp: string) {
  const base = Date.parse(baseTimestamp);
  const value = Date.parse(timestamp);
  if (Number.isFinite(base) && Number.isFinite(value)) {
    return Math.max(0, value - base);
  }
  return 0;
}
