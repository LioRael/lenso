import type {
  ExecutionEdge,
  ExecutionNode,
  RuntimeStatus,
  RuntimeStory,
  TimelineItem,
} from "../data/mock-runtime";
import { isRetryable } from "../data/mock-runtime";

export type RuntimeHeatmapCell = {
  bucketStart: string;
  bucketEnd: string;
  service: string;
  nodeType: "event" | "function";
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

export type ApiRuntimeStoryListResponse = {
  data?: ApiRuntimeStoryListItem[];
  page?: ApiPageInfo;
  order?: string;
};

export type ApiRuntimeStoryListItem = {
  title?: string;
  correlation_id?: string;
  status?: string;
  duration?: number;
  node_count?: number;
  error_count?: number;
  services?: string[];
  pattern?: string[];
  root_error?: string | null;
  created_at?: string;
  updated_at?: string;
};

export type ApiRuntimeStoryDetailResponse = {
  data?: ApiRuntimeStoryDetail;
};

export type ApiRuntimeStoryDetail = {
  summary?: ApiRuntimeStoryListItem;
  nodes?: ApiRuntimeStoryNode[];
  edges?: ApiRuntimeStoryEdge[];
  timeline_items?: ApiTimelineItem[];
};

export type ApiRuntimeStoryNode = {
  id?: string;
  type?: string;
  name?: string;
  status?: string;
  service?: string;
  timestamp?: string;
  duration_ms?: number;
  error?: string | null;
  metadata?: unknown;
};

export type ApiRuntimeStoryEdge = {
  id?: string;
  source?: string;
  target?: string;
  type?: string;
  label?: string | null;
};

export type ApiTimelineResponse = {
  data?: ApiTimelineItem[];
  page?: ApiPageInfo;
  order?: string;
};

export type ApiTimelineItem = {
  type?: string;
  id?: string;
  name?: string;
  status?: string;
  attempts?: number;
  max_attempts?: number;
  created_at?: string;
  started_at?: string | null;
  completed_at?: string | null;
  last_error?: string | null;
  correlation_id?: string;
};

export type ApiRuntimeHeatmapResponse = {
  data?: ApiRuntimeHeatmapCell[];
  bucket_seconds?: number;
  page?: ApiPageInfo;
};

export type ApiRuntimeHeatmapCell = {
  bucket_start?: string;
  bucket_end?: string;
  service?: string;
  node_type?: string;
  total_count?: number;
  error_count?: number;
  dead_count?: number;
  avg_duration_ms?: number | null;
  max_duration_ms?: number | null;
};

type ApiPageInfo = {
  limit?: number;
  next_created_before?: string | null;
};

const fallbackTimestamp = "1970-01-01T00:00:00.000Z";

export function normalizeRuntimeStoryListResponse(
  response: ApiRuntimeStoryListResponse
): { stories: RuntimeStory[]; page?: PageInfo } {
  return {
    ...(response.page ? { page: normalizePageInfo(response.page) } : {}),
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

    return {
      ...(attempts === undefined ? {} : { attempts }),
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
      name: safeString(node.name, "Runtime Work"),
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
    ...(response.page ? { page: normalizePageInfo(response.page) } : {}),
  };
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
    nodeType: cell.node_type === "event" ? "event" : "function",
    service: safeString(cell.service, "runtime"),
    totalCount: normalizeInteger(cell.total_count, 0),
  };
}

function normalizePageInfo(page: ApiPageInfo): PageInfo {
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
