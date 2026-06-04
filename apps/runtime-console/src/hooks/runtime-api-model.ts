import type {
  AdminRuntimeHeatmapCell,
  AdminRuntimeHeatmapResponse,
  AdminRuntimeExecutionLog,
  AdminRuntimeExecutionLogListResponse,
  AdminRuntimeExecutionPayloadResponse,
  AdminRuntimeStoryDetail,
  AdminRuntimeStoryDetailResponse,
  AdminRuntimeStoryEdge,
  AdminRuntimeStoryListItem,
  AdminRuntimeStoryListResponse,
  AdminRuntimeTechnicalOperation,
  AdminRuntimeTechnicalOperationListResponse,
  AdminRuntimeTimelineItem,
  AdminRuntimeTimelineResponse,
  PageInfo as ApiPageInfo,
} from "../../../../packages/ts-sdk/src/generated/types";
import type {
  ExecutionEdge,
  ExecutionLogEntry,
  ExecutionPayload,
  ExecutionNode,
  RuntimeStatus,
  RuntimeStory,
  TechnicalOperation,
  TimelineItem,
} from "../data/mock-runtime";
import { isRetryable } from "../data/mock-runtime";

export type RuntimeHeatmapCell = {
  bucketStart: string;
  bucketEnd: string;
  service: string;
  nodeType: "event" | "function" | "http";
  totalCount: number;
  errorCount: number;
  deadCount: number;
  avgDurationMs?: number;
  maxDurationMs?: number;
};

export type RuntimeHeatmap = {
  bucketSeconds: number;
  cells: RuntimeHeatmapCell[];
  page?: PageInfo;
};

export type PageInfo = {
  limit: number;
  nextCreatedBefore?: string;
};

type DeepPartial<T> =
  T extends Array<infer Item>
    ? Array<DeepPartial<Item>>
    : T extends object
      ? { [Key in keyof T]?: DeepPartial<T[Key]> }
      : T;

export type ApiRuntimeStoryListResponse =
  DeepPartial<AdminRuntimeStoryListResponse>;
export type ApiRuntimeStoryListItem = DeepPartial<AdminRuntimeStoryListItem>;
export type ApiRuntimeStoryDetailResponse =
  DeepPartial<AdminRuntimeStoryDetailResponse>;
export type ApiRuntimeStoryDetail = DeepPartial<AdminRuntimeStoryDetail>;
export type ApiRuntimeStoryEdge = DeepPartial<AdminRuntimeStoryEdge>;
export type ApiTimelineResponse = DeepPartial<AdminRuntimeTimelineResponse>;
export type ApiTimelineItem = DeepPartial<AdminRuntimeTimelineItem>;
export type ApiRuntimeHeatmapResponse =
  DeepPartial<AdminRuntimeHeatmapResponse>;
export type ApiRuntimeHeatmapCell = DeepPartial<AdminRuntimeHeatmapCell>;
export type ApiExecutionPayloadResponse =
  DeepPartial<AdminRuntimeExecutionPayloadResponse>;
export type ApiExecutionLogResponse =
  DeepPartial<AdminRuntimeExecutionLogListResponse>;
export type ApiExecutionLog = DeepPartial<AdminRuntimeExecutionLog>;
export type ApiTechnicalOperationResponse =
  DeepPartial<AdminRuntimeTechnicalOperationListResponse>;
export type ApiTechnicalOperation = DeepPartial<AdminRuntimeTechnicalOperation>;

const fallbackTimestamp = "1970-01-01T00:00:00.000Z";

export function normalizeRuntimeStoryListResponse(
  response: ApiRuntimeStoryListResponse
): { stories: RuntimeStory[]; page?: PageInfo } {
  return {
    ...(response.page ? { page: normalizePageInfoPartial(response.page) } : {}),
    stories: (response.data ?? []).map(normalizeRuntimeStoryListItem),
  };
}

export function normalizeRuntimeStoryListItem(
  item: ApiRuntimeStoryListItem
): RuntimeStory {
  const correlationId = safeString(item.correlation_id, "unknown");
  const timestamp = normalizeTimestamp(item.created_at);
  const durationMs = normalizeDuration(item.duration);
  const services = normalizeStringArray(item.services);
  const pattern = normalizeStringArray(item.pattern);
  const nodeCount = normalizeInteger(item.node_count, 0);
  const errorCount = normalizeInteger(item.error_count, 0);
  const nodes = Array.from({ length: nodeCount }, (_, index) =>
    placeholderNode({
      correlationId,
      durationMs: index === 0 ? durationMs : 0,
      ...(index === 0 && item.root_error ? { error: item.root_error } : {}),
      id: `${correlationId}:summary:${index + 1}`,
      index,
      kind: toExecutionNodeKind(pattern[index] ?? pattern.at(-1)),
      service: services[index] ?? services.at(-1) ?? "runtime",
      status: normalizeRuntimeStatus(
        index < errorCount ? "failed" : item.status
      ),
      timestamp,
    })
  );

  return {
    correlationId,
    durationMs,
    id: correlationId,
    name: safeString(item.title, "Runtime Story"),
    nodes,
    service: services[0] ?? "runtime",
    source: "runtime-story",
    status: normalizeRuntimeStatus(item.status),
    timestamp,
  };
}

export function normalizeRuntimeStory(
  detail: ApiRuntimeStoryDetail
): RuntimeStory {
  const summary = detail.summary ?? {};
  const correlationId = safeString(summary.correlation_id, "unknown");
  const timestamp = normalizeTimestamp(summary.created_at);
  const hasValidBaseTimestamp =
    typeof summary.created_at === "string" &&
    Number.isFinite(Date.parse(summary.created_at));
  const baseTimestamp = Date.parse(timestamp);
  const rawNodes = detail.nodes ?? [];
  const nodeIdAliases = new Map<string, string>();
  const seenIds = new Map<string, number>();
  const nodes = rawNodes.map((node, index): ExecutionNode => {
    const rawId = safeString(node.id, `node_${index + 1}`);
    const seenCount = seenIds.get(rawId) ?? 0;
    seenIds.set(rawId, seenCount + 1);
    const id = seenCount === 0 ? rawId : `${rawId}__${seenCount + 1}`;
    if (!nodeIdAliases.has(rawId)) {
      nodeIdAliases.set(rawId, id);
    }

    const metadata = objectRecord(node.metadata);
    const nodeTimestamp = normalizeTimestamp(node.timestamp, timestamp);
    const parsedNodeTimestamp = Date.parse(nodeTimestamp);
    const startMs = hasValidBaseTimestamp
      ? Number.isFinite(baseTimestamp) && Number.isFinite(parsedNodeTimestamp)
        ? Math.max(0, parsedNodeTimestamp - baseTimestamp)
        : index
      : 0;
    const error = node.error ?? undefined;
    const status = normalizeRuntimeStatus(node.status);
    const attempts = normalizeOptionalInteger(metadata.attempts);
    const maxAttempts = normalizeOptionalInteger(metadata.max_attempts);
    const canonicalName = safeString(node.name, "Runtime Work");
    const displayName = safeString(node.display_name, canonicalName);

    return {
      ...(attempts === undefined ? {} : { attempts }),
      ...(displayName === canonicalName ? {} : { canonicalName }),
      ...(maxAttempts === undefined ? {} : { maxAttempts }),
      attributes: metadata,
      context: {
        correlation_id: correlationId,
        ...(typeof metadata.causation_id === "string"
          ? { causation_id: metadata.causation_id }
          : {}),
      },
      durationMs: normalizeDuration(node.duration_ms),
      events: [],
      id,
      kind: toExecutionNodeKind(node.type),
      logs: error ? [error] : [],
      name: displayName,
      retryable: isRetryable(status),
      service: safeString(node.service, "runtime"),
      startMs,
      status,
    };
  });
  const nodeIds = new Set(nodes.map((node) => node.id));
  const edges = normalizeRuntimeEdges(
    detail.edges ?? [],
    nodeIdAliases,
    nodeIds
  );
  const parentByTarget = new Map(
    edges.map((edge) => [edge.target, edge.source])
  );
  const nodesWithParents = nodes.map((node) => {
    const parentId = parentByTarget.get(node.id);
    return parentId ? { ...node, parentId } : node;
  });
  const timelineItems =
    detail.timeline_items?.map((item, index) =>
      normalizeTimelineItem(item, correlationId, index)
    ) ?? [];
  const lastNodeEnd = Math.max(
    0,
    ...nodesWithParents.map((node) => node.startMs + node.durationMs),
    ...timelineItems.map((item) =>
      timelineItemOffset(timestamp, item.completedAt ?? item.createdAt)
    )
  );

  return {
    correlationId,
    durationMs: Math.max(normalizeDuration(summary.duration), lastNodeEnd),
    edges,
    id: correlationId,
    name: safeString(summary.title, "Runtime Story"),
    nodes: nodesWithParents,
    service: nodesWithParents[0]?.service ?? "runtime",
    source: "runtime-story",
    status: normalizeRuntimeStatus(summary.status),
    timelineItems,
    timestamp,
  };
}

export function normalizeTimelineItems(
  response: ApiTimelineResponse,
  fallbackCorrelationId: string
): TimelineItem[] {
  return (response.data ?? []).map((item, index) =>
    normalizeTimelineItem(item, fallbackCorrelationId, index)
  );
}

export function normalizeRuntimeHeatmap(
  response: ApiRuntimeHeatmapResponse
): RuntimeHeatmap {
  return {
    bucketSeconds:
      normalizeOptionalInteger(response.bucket_seconds) &&
      Number(response.bucket_seconds) > 0
        ? Number(response.bucket_seconds)
        : 300,
    cells: (response.data ?? []).map(normalizeRuntimeHeatmapCell),
    ...(response.page ? { page: normalizePageInfoPartial(response.page) } : {}),
  };
}

export function normalizeTechnicalOperations(
  response: ApiTechnicalOperationResponse
): TechnicalOperation[] {
  return (response.data ?? []).map(normalizeTechnicalOperation);
}

export function normalizeExecutionPayload(
  response: ApiExecutionPayloadResponse
): ExecutionPayload {
  const data = response.data ?? {};
  return {
    input: data.input,
    metadata: data.metadata,
    output: data.output,
    redactedFields: normalizeStringArray(data.redacted_fields),
  };
}

export function normalizeExecutionLogs(
  response: ApiExecutionLogResponse
): ExecutionLogEntry[] {
  return (response.data ?? []).map(normalizeExecutionLog);
}

function normalizeRuntimeEdges(
  edges: ApiRuntimeStoryEdge[],
  nodeIdAliases: Map<string, string>,
  nodeIds: Set<string>
): ExecutionEdge[] {
  const seenEdges = new Set<string>();
  const normalizedEdges: ExecutionEdge[] = [];

  for (const edge of edges) {
    const source = nodeIdAliases.get(edge.source ?? "") ?? edge.source;
    const target = nodeIdAliases.get(edge.target ?? "") ?? edge.target;
    if (!source || !target || !nodeIds.has(source) || !nodeIds.has(target)) {
      continue;
    }
    const id = safeString(
      edge.id,
      `${source}:${target}:${edge.type ?? "edge"}`
    );
    const dedupeKey = `${source}:${target}:${edge.type ?? "edge"}:${id}`;
    if (seenEdges.has(dedupeKey)) {
      continue;
    }
    seenEdges.add(dedupeKey);
    normalizedEdges.push({
      id,
      ...(edge.label ? { label: edge.label } : {}),
      source,
      target,
      type: safeString(edge.type, "sequence"),
    });
  }

  return normalizedEdges;
}

function normalizeTimelineItem(
  item: ApiTimelineItem,
  fallbackCorrelationId: string,
  index: number
): TimelineItem {
  const id = safeString(item.id, `timeline_${index + 1}`);
  const createdAt = normalizeTimestamp(item.created_at);
  const completedAt = maybeTimestamp(item.completed_at);
  const lastError = item.last_error;
  const startedAt = maybeTimestamp(item.started_at);
  return {
    attempts: normalizeInteger(item.attempts, 1),
    correlationId: safeString(item.correlation_id, fallbackCorrelationId),
    createdAt,
    detailId: id,
    id,
    maxAttempts: normalizeInteger(item.max_attempts, 1),
    name: safeString(item.name, "Runtime Work"),
    ...(completedAt ? { completedAt } : {}),
    ...(lastError ? { lastError } : {}),
    ...(startedAt ? { startedAt } : {}),
    status: normalizeRuntimeStatus(item.status),
    type: safeString(item.type, "runtime"),
  };
}

function normalizeExecutionLog(log: ApiExecutionLog): ExecutionLogEntry {
  return {
    attributes: objectRecord(log.attributes),
    body: safeString(log.body, ""),
    correlationId: safeString(log.correlation_id, "unknown"),
    executionName: safeString(log.execution_name, "Runtime Work"),
    id: safeString(log.id, "execution_log"),
    nodeId: safeString(log.node_id, "unknown"),
    nodeType: safeString(log.node_type, "runtime"),
    occurredAt: normalizeTimestamp(log.occurred_at),
    redactedFields: normalizeStringArray(log.redacted_fields),
    serviceName: safeString(log.service_name, "runtime"),
    severity: normalizeLogSeverity(log.severity),
    ...(typeof log.span_id === "string" ? { spanId: log.span_id } : {}),
    storyId: safeString(
      log.story_id,
      safeString(log.correlation_id, "unknown")
    ),
    ...(typeof log.trace_id === "string" ? { traceId: log.trace_id } : {}),
  };
}

function normalizeTechnicalOperation(
  operation: ApiTechnicalOperation
): TechnicalOperation {
  return {
    attributes: objectRecord(operation.attributes),
    category: normalizeTechnicalOperationCategory(operation.category),
    correlationId: safeString(operation.correlation_id, "unknown"),
    durationMs: normalizeDuration(operation.duration_ms),
    endedAt: normalizeTimestamp(operation.ended_at),
    id: safeString(operation.id, "technical_operation"),
    name: safeString(operation.name, "Technical Operation"),
    ...(operation.related_node_id
      ? { relatedNodeId: operation.related_node_id }
      : {}),
    source: normalizeTechnicalOperationSource(operation.source),
    startedAt: normalizeTimestamp(operation.started_at),
    status: safeString(operation.status, "unknown"),
    storyId: safeString(operation.story_id, "unknown"),
  };
}

function normalizeLogSeverity(severity: string | undefined) {
  switch (severity) {
    case "trace":
    case "debug":
    case "info":
    case "warn":
    case "error": {
      return severity;
    }
    default: {
      return "info";
    }
  }
}

function normalizeTechnicalOperationCategory(
  category: string | undefined
): TechnicalOperation["category"] {
  switch (category) {
    case "http":
    case "db":
    case "redis":
    case "s3":
    case "ses":
    case "worker":
    case "runtime":
    case "external":
    case "unknown": {
      return category;
    }
    default: {
      return "unknown";
    }
  }
}

function normalizeTechnicalOperationSource(
  source: string | undefined
): TechnicalOperation["source"] {
  return source === "remote_proxy" ? "remote_proxy" : "otel";
}

function normalizeRuntimeHeatmapCell(
  cell: ApiRuntimeHeatmapCell
): RuntimeHeatmapCell {
  const bucketStart = normalizeTimestamp(cell.bucket_start);
  const avgDurationMs = normalizeOptionalPositiveDuration(cell.avg_duration_ms);
  const maxDurationMs = normalizeOptionalPositiveDuration(cell.max_duration_ms);
  return {
    bucketEnd: normalizeTimestamp(cell.bucket_end, bucketStart),
    bucketStart,
    deadCount: normalizeInteger(cell.dead_count, 0),
    errorCount: normalizeInteger(cell.error_count, 0),
    ...(avgDurationMs === undefined ? {} : { avgDurationMs }),
    ...(maxDurationMs === undefined ? {} : { maxDurationMs }),
    nodeType: normalizeHeatmapNodeType(cell.node_type),
    service: safeString(cell.service, "runtime"),
    totalCount: normalizeInteger(cell.total_count, 0),
  };
}

function normalizeHeatmapNodeType(type: string | undefined) {
  switch (type) {
    case "event":
    case "outbox_event": {
      return "event";
    }
    case "http":
    case "http_request": {
      return "http";
    }
    default: {
      return "function";
    }
  }
}

function normalizePageInfoPartial(page: DeepPartial<ApiPageInfo>): PageInfo {
  return {
    limit: normalizeInteger(page.limit, 0),
    ...(maybeTimestamp(page.next_created_before)
      ? { nextCreatedBefore: maybeTimestamp(page.next_created_before)! }
      : {}),
  };
}

function placeholderNode(input: {
  correlationId: string;
  durationMs: number;
  error?: string | null;
  id: string;
  index: number;
  kind: ExecutionNode["kind"];
  service: string;
  status: RuntimeStatus;
  timestamp: string;
}): ExecutionNode {
  return {
    attributes: {},
    context: { correlation_id: input.correlationId },
    durationMs: input.durationMs,
    events: [],
    id: input.id,
    kind: input.kind,
    logs: input.error ? [input.error] : [],
    name: "Runtime Work",
    retryable: isRetryable(input.status),
    service: input.service,
    startMs: input.index === 0 ? 0 : input.index,
    status: input.status,
  };
}

function toExecutionNodeKind(type: string | undefined): ExecutionNode["kind"] {
  switch (type) {
    case "http":
    case "http_request":
    case "request": {
      return "http";
    }
    case "command": {
      return "command";
    }
    case "database": {
      return "database";
    }
    case "event":
    case "outbox_event": {
      return "event";
    }
    case "handler": {
      return "handler";
    }
    case "function":
    case "function_run":
    case "flow_step":
    case "agent_tool_call": {
      return "function";
    }
    case "external":
    case "remote_proxy_call":
    case "external_provider_call": {
      return "external";
    }
    case "worker":
    case "runtime": {
      return "runtime";
    }
    default: {
      return "runtime";
    }
  }
}

function normalizeRuntimeStatus(status: string | undefined): RuntimeStatus {
  switch (status) {
    case "pending":
    case "processing":
    case "running":
    case "published":
    case "completed":
    case "failed":
    case "dead": {
      return status;
    }
    default: {
      return "pending";
    }
  }
}

function normalizeTimestamp(value: unknown, fallback = fallbackTimestamp) {
  if (typeof value === "string" && Number.isFinite(Date.parse(value))) {
    return value;
  }
  return fallback;
}

function maybeTimestamp(value: unknown) {
  if (typeof value === "string" && Number.isFinite(Date.parse(value))) {
    return value;
  }
  return undefined;
}

function timelineItemOffset(baseTimestamp: string, timestamp: string) {
  const base = Date.parse(baseTimestamp);
  const value = Date.parse(timestamp);
  if (Number.isFinite(base) && Number.isFinite(value)) {
    return Math.max(0, value - base);
  }
  return 0;
}

function normalizeDuration(value: unknown) {
  return normalizeInteger(value, 0);
}

function normalizeOptionalPositiveDuration(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) && value >= 0
    ? Math.trunc(value)
    : undefined;
}

function normalizeInteger(value: unknown, fallback: number) {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.max(0, Math.trunc(value))
    : fallback;
}

function normalizeOptionalInteger(value: unknown) {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.max(0, Math.trunc(value))
    : undefined;
}

function safeString(value: unknown, fallback: string) {
  return typeof value === "string" && value.trim().length > 0
    ? value
    : fallback;
}

function normalizeStringArray(value: unknown) {
  return Array.isArray(value)
    ? value.filter(
        (item): item is string => typeof item === "string" && item.length > 0
      )
    : [];
}

function objectRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}
