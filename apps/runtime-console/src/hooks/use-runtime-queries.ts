import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  correlationId,
  type Actor,
  type FunctionRun,
  functionRuns,
  queueHealth,
  type RuntimeEvent,
  runtimeEvents,
  type TimelineItem,
  timelineItems,
  type TraceRun,
  type TraceSpan,
  traceRuns,
  type RuntimeStatus,
} from "../data/mock-runtime";
import { httpClient, isApiMode } from "../lib/http-client";

export const runtimeQueryKeys = {
  summary: ["runtime", "summary"] as const,
  events: ["runtime", "events"] as const,
  functions: ["runtime", "functions"] as const,
  heatmap: ["runtime", "heatmap"] as const,
  timeline: (id: string) => ["runtime", "timeline", id] as const,
  traces: ["runtime", "traces"] as const,
  deadLetters: ["runtime", "dead-letters"] as const,
};

export type RuntimeSummaryStatus = "healthy" | "degraded" | "failing";

export type RuntimeSummaryItem = {
  type: "outbox_event" | "function_run";
  id: string;
  name: string;
  status: RuntimeStatus;
  attempts: number;
  maxAttempts: number;
  correlationId: string;
  createdAt: string;
  lastError?: string;
};

export type RuntimeSummary = {
  status: RuntimeSummaryStatus;
  outbox: {
    pending: number;
    processing: number;
    published: number;
    failed: number;
    dead: number;
    oldestPendingAgeSeconds?: number;
    oldestFailedAgeSeconds?: number;
  };
  functions: {
    pending: number;
    running: number;
    completed: number;
    failed: number;
    dead: number;
    oldestPendingAgeSeconds?: number;
    oldestFailedAgeSeconds?: number;
  };
  recentActivity: RuntimeSummaryItem[];
  recentFailures: RuntimeSummaryItem[];
};

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
};

export function useRuntimeSummary() {
  return useQuery({
    queryKey: runtimeQueryKeys.summary,
    queryFn: async () => (isApiMode() ? fetchRuntimeSummary() : mockSummary()),
  });
}

export function useRuntimeEvents() {
  return useQuery({
    queryKey: runtimeQueryKeys.events,
    queryFn: async () => (isApiMode() ? fetchRuntimeEvents() : runtimeEvents),
  });
}

export function useRuntimeFunctions() {
  return useQuery({
    queryKey: runtimeQueryKeys.functions,
    queryFn: async () => (isApiMode() ? fetchRuntimeFunctions() : functionRuns),
  });
}

export function useRuntimeTimeline(activeCorrelationId: string) {
  return useQuery({
    queryKey: runtimeQueryKeys.timeline(activeCorrelationId),
    queryFn: async () => {
      const id = activeCorrelationId || correlationId;
      return isApiMode()
        ? fetchRuntimeTimeline(id)
        : timelineItems.filter((item) => item.correlationId === id);
    },
  });
}

export function useRuntimeHeatmap() {
  return useQuery({
    queryKey: runtimeQueryKeys.heatmap,
    queryFn: async () =>
      isApiMode() ? fetchRuntimeHeatmap() : mockRuntimeHeatmap(),
  });
}

export function useDeadLetters() {
  return useQuery({
    queryKey: runtimeQueryKeys.deadLetters,
    queryFn: async () => {
      const [events, runs] = isApiMode()
        ? await Promise.all([fetchRuntimeEvents(), fetchRuntimeFunctions()])
        : [runtimeEvents, functionRuns];
      return [
        ...events
          .filter(
            (event) => event.status === "failed" || event.status === "dead"
          )
          .map((item) => ({ kind: "event" as const, item })),
        ...runs
          .filter((run) => run.status === "failed" || run.status === "dead")
          .map((item) => ({ kind: "function" as const, item })),
      ];
    },
  });
}

export function useRuntimeTraces() {
  return useQuery({
    queryKey: runtimeQueryKeys.traces,
    queryFn: async () => (isApiMode() ? fetchRuntimeStories() : traceRuns),
  });
}

export function useRetryRuntimeWork() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (_input: {
      kind: "event" | "function" | "timeline";
      id: string;
    }) => {
      if (isApiMode()) {
        await retryRuntimeWork(_input);
        return { ok: true };
      }

      await new Promise((resolve) => window.setTimeout(resolve, 320));
      return { ok: true };
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: runtimeQueryKeys.summary }),
        queryClient.invalidateQueries({ queryKey: runtimeQueryKeys.events }),
        queryClient.invalidateQueries({ queryKey: runtimeQueryKeys.functions }),
        queryClient.invalidateQueries({
          queryKey: runtimeQueryKeys.deadLetters,
        }),
      ]);
    },
  });
}

function mockSummary(): RuntimeSummary {
  const recentActivity = [
    ...runtimeEvents.map<RuntimeSummaryItem>((event) => ({
      type: "outbox_event",
      id: event.id,
      name: event.eventName,
      status: event.status,
      attempts: event.attempts,
      maxAttempts: event.maxAttempts,
      correlationId: event.correlationId,
      createdAt: event.createdAt,
      ...(event.lastError ? { lastError: event.lastError } : {}),
    })),
    ...functionRuns.map<RuntimeSummaryItem>((run) => ({
      type: "function_run",
      id: run.id,
      name: run.functionName,
      status: run.status,
      attempts: run.attempts,
      maxAttempts: run.maxAttempts,
      correlationId: run.correlationId,
      createdAt: run.createdAt,
      ...(run.lastError ? { lastError: run.lastError } : {}),
    })),
  ]
    .sort((a, b) => b.createdAt.localeCompare(a.createdAt))
    .slice(0, 10);

  const recentFailures = recentActivity.filter(
    (item) => item.status === "failed" || item.status === "dead"
  );
  const deadCount = recentActivity.filter(
    (item) => item.status === "dead"
  ).length;
  const failedCount = recentActivity.filter(
    (item) => item.status === "failed"
  ).length;

  return {
    status:
      deadCount > 0 ? "failing" : failedCount > 0 ? "degraded" : "healthy",
    outbox: {
      pending: runtimeEvents.filter((event) => event.status === "pending")
        .length,
      processing: runtimeEvents.filter((event) => event.status === "processing")
        .length,
      published: runtimeEvents.filter((event) => event.status === "published")
        .length,
      failed: runtimeEvents.filter((event) => event.status === "failed").length,
      dead: runtimeEvents.filter((event) => event.status === "dead").length,
      ...optionalAge("oldestPendingAgeSeconds", ageFromQueue("outbox")),
    },
    functions: {
      pending: functionRuns.filter((run) => run.status === "pending").length,
      running: functionRuns.filter((run) => run.status === "running").length,
      completed: functionRuns.filter((run) => run.status === "completed")
        .length,
      failed: functionRuns.filter((run) => run.status === "failed").length,
      dead: functionRuns.filter((run) => run.status === "dead").length,
      ...optionalAge(
        "oldestPendingAgeSeconds",
        ageFromQueue("runtime.functions")
      ),
    },
    recentActivity,
    recentFailures,
  };
}

function optionalAge(
  key: "oldestPendingAgeSeconds" | "oldestFailedAgeSeconds",
  value: number | null | undefined
) {
  return value === null || value === undefined ? {} : { [key]: value };
}

function ageFromQueue(queueName: string) {
  const queue = queueHealth.find((item) => item.name === queueName);
  if (!queue) {
    return undefined;
  }

  if (queue.oldest.endsWith("s")) {
    return Number(queue.oldest.replace("s", ""));
  }
  if (queue.oldest.endsWith("m")) {
    return Number(queue.oldest.replace("m", "")) * 60;
  }
  return undefined;
}

type ApiRuntimeSummaryResponse = {
  status: RuntimeSummaryStatus;
  outbox: {
    pending: number;
    processing: number;
    published: number;
    failed: number;
    dead: number;
    oldest_pending_age_seconds?: number | null;
    oldest_failed_age_seconds?: number | null;
  };
  functions: {
    pending: number;
    running: number;
    completed: number;
    failed: number;
    dead: number;
    oldest_pending_age_seconds?: number | null;
    oldest_failed_age_seconds?: number | null;
  };
  recent_activity: ApiRuntimeSummaryItem[];
  recent_failures: ApiRuntimeSummaryItem[];
};

type ApiRuntimeSummaryItem = {
  type: "outbox_event" | "function_run";
  id: string;
  name: string;
  status: RuntimeStatus;
  attempts: number;
  max_attempts: number;
  correlation_id?: string | null;
  created_at: string;
  last_error?: string | null;
};

type ApiOutboxListResponse = {
  data: ApiOutboxEvent[];
};

type ApiOutboxEvent = {
  id: string;
  event_name: string;
  status: RuntimeStatus;
  attempts: number;
  max_attempts: number;
  aggregate_id?: string;
  aggregate_type?: string;
  correlation_id: string;
  causation_id?: string | null;
  created_at: string;
  locked_by?: string | null;
  published_at?: string | null;
  last_error?: string | null;
  payload?: Record<string, unknown>;
  actor?: unknown;
};

type ApiFunctionRunListResponse = {
  data: ApiFunctionRun[];
};

type ApiFunctionRun = {
  id: string;
  function_name: string;
  status: RuntimeStatus;
  attempts: number;
  max_attempts: number;
  correlation_id: string;
  created_at: string;
  locked_by?: string | null;
  started_at?: string | null;
  completed_at?: string | null;
  last_error?: string | null;
  input_json?: Record<string, unknown>;
  actor?: unknown;
};

type ApiTimelineResponse = {
  data: ApiTimelineItem[];
};

type ApiTimelineItem = {
  type: TimelineItem["type"];
  id: string;
  name: string;
  status: RuntimeStatus;
  attempts: number;
  max_attempts: number;
  created_at: string;
  started_at?: string | null;
  completed_at?: string | null;
  last_error?: string | null;
  correlation_id: string;
};

type ApiRuntimeStoryListResponse = {
  data: ApiRuntimeStoryListItem[];
};

type ApiRuntimeStoryListItem = {
  correlation_id: string;
};

type ApiRuntimeStoryDetailResponse = {
  data: ApiRuntimeStoryDetail;
};

type ApiRuntimeStoryDetail = {
  summary: {
    title: string;
    correlation_id: string;
    status: RuntimeStatus;
    duration: number;
    created_at: string;
  };
  nodes: ApiRuntimeStoryNode[];
};

type ApiRuntimeStoryNode = {
  id: string;
  type: "request" | "function" | "event" | "worker" | "external" | "unknown";
  name: string;
  status: RuntimeStatus;
  service: string;
  timestamp: string;
  duration_ms: number;
  error?: string | null;
  metadata?: Record<string, unknown>;
};

type ApiRuntimeHeatmapResponse = {
  data: ApiRuntimeHeatmapCell[];
  bucket_seconds: number;
};

type ApiRuntimeHeatmapCell = {
  bucket_start: string;
  bucket_end: string;
  service: string;
  node_type: "event" | "function";
  total_count: number;
  error_count: number;
  dead_count: number;
  avg_duration_ms?: number | null;
  max_duration_ms?: number | null;
};

async function fetchRuntimeSummary(): Promise<RuntimeSummary> {
  const response = await httpClient
    .get("admin/runtime/summary")
    .json<ApiRuntimeSummaryResponse>();

  return {
    status: response.status,
    outbox: {
      pending: response.outbox.pending,
      processing: response.outbox.processing,
      published: response.outbox.published,
      failed: response.outbox.failed,
      dead: response.outbox.dead,
      ...optionalAge(
        "oldestPendingAgeSeconds",
        response.outbox.oldest_pending_age_seconds
      ),
      ...optionalAge(
        "oldestFailedAgeSeconds",
        response.outbox.oldest_failed_age_seconds
      ),
    },
    functions: {
      pending: response.functions.pending,
      running: response.functions.running,
      completed: response.functions.completed,
      failed: response.functions.failed,
      dead: response.functions.dead,
      ...optionalAge(
        "oldestPendingAgeSeconds",
        response.functions.oldest_pending_age_seconds
      ),
      ...optionalAge(
        "oldestFailedAgeSeconds",
        response.functions.oldest_failed_age_seconds
      ),
    },
    recentActivity: response.recent_activity.map(toSummaryItem),
    recentFailures: response.recent_failures.map(toSummaryItem),
  };
}

async function fetchRuntimeEvents(): Promise<RuntimeEvent[]> {
  const response = await httpClient
    .get("admin/runtime/outbox")
    .json<ApiOutboxListResponse>();
  return response.data.map(toRuntimeEvent);
}

async function fetchRuntimeFunctions(): Promise<FunctionRun[]> {
  const response = await httpClient
    .get("admin/runtime/functions")
    .json<ApiFunctionRunListResponse>();
  return response.data.map(toFunctionRun);
}

async function fetchRuntimeTimeline(id: string): Promise<TimelineItem[]> {
  const response = await httpClient
    .get(`admin/runtime/timeline/${encodeURIComponent(id)}`)
    .json<ApiTimelineResponse>();
  return response.data.map(toTimelineItem);
}

async function fetchRuntimeHeatmap(): Promise<RuntimeHeatmap> {
  const response = await httpClient
    .get("admin/runtime/heatmap")
    .json<ApiRuntimeHeatmapResponse>();
  return {
    bucketSeconds: response.bucket_seconds,
    cells: response.data.map(toRuntimeHeatmapCell),
  };
}

async function fetchRuntimeStories(): Promise<TraceRun[]> {
  const response = await httpClient
    .get("admin/runtime/stories")
    .json<ApiRuntimeStoryListResponse>();

  const details = await Promise.all(
    response.data.map((story) =>
      fetchRuntimeStory(story.correlation_id).catch(() => null)
    )
  );

  return details.filter((trace): trace is TraceRun => trace !== null);
}

async function fetchRuntimeStory(
  storyCorrelationId: string
): Promise<TraceRun> {
  const response = await httpClient
    .get(`admin/runtime/stories/${encodeURIComponent(storyCorrelationId)}`)
    .json<ApiRuntimeStoryDetailResponse>();
  return toTraceRun(response.data);
}

async function retryRuntimeWork(input: {
  kind: "event" | "function" | "timeline";
  id: string;
}) {
  const route =
    input.kind === "function"
      ? `admin/runtime/functions/${encodeURIComponent(input.id)}/retry`
      : `admin/runtime/outbox/${encodeURIComponent(input.id)}/retry`;
  await httpClient.post(route).json();
}

function toSummaryItem(item: ApiRuntimeSummaryItem): RuntimeSummaryItem {
  return {
    type: item.type,
    id: item.id,
    name: item.name,
    status: item.status,
    attempts: item.attempts,
    maxAttempts: item.max_attempts,
    correlationId: item.correlation_id ?? "-",
    createdAt: item.created_at,
    ...(item.last_error ? { lastError: item.last_error } : {}),
  };
}

function toRuntimeEvent(event: ApiOutboxEvent): RuntimeEvent {
  return {
    id: event.id,
    eventName: event.event_name,
    status: event.status,
    attempts: event.attempts,
    maxAttempts: event.max_attempts,
    aggregateId: event.aggregate_id ?? "-",
    aggregateType: event.aggregate_type ?? "-",
    correlationId: event.correlation_id,
    causationId: event.causation_id ?? "-",
    createdAt: event.created_at,
    ...(event.published_at ? { publishedAt: event.published_at } : {}),
    ...(event.last_error ? { lastError: event.last_error } : {}),
    actor: toActor(event.actor),
    payload: event.payload ?? {},
  };
}

function toFunctionRun(run: ApiFunctionRun): FunctionRun {
  return {
    id: run.id,
    functionName: run.function_name,
    status: run.status,
    attempts: run.attempts,
    maxAttempts: run.max_attempts,
    correlationId: run.correlation_id,
    createdAt: run.created_at,
    ...(run.started_at ? { startedAt: run.started_at } : {}),
    ...(run.completed_at ? { completedAt: run.completed_at } : {}),
    ...(run.locked_by ? { lockedBy: run.locked_by } : {}),
    ...(run.last_error ? { lastError: run.last_error } : {}),
    actor: toActor(run.actor),
    input: run.input_json ?? {},
    logs: run.last_error ? [run.last_error] : [],
  };
}

function toTimelineItem(item: ApiTimelineItem): TimelineItem {
  return {
    id: item.id,
    type: item.type,
    name: item.name,
    status: item.status,
    attempts: item.attempts,
    maxAttempts: item.max_attempts,
    createdAt: item.created_at,
    ...(item.started_at ? { startedAt: item.started_at } : {}),
    ...(item.completed_at ? { completedAt: item.completed_at } : {}),
    ...(item.last_error ? { lastError: item.last_error } : {}),
    correlationId: item.correlation_id,
    detailId: item.id,
  };
}

function toRuntimeHeatmapCell(
  cell: ApiRuntimeHeatmapCell
): RuntimeHeatmapCell {
  return {
    bucketStart: cell.bucket_start,
    bucketEnd: cell.bucket_end,
    service: cell.service,
    nodeType: cell.node_type,
    totalCount: cell.total_count,
    errorCount: cell.error_count,
    deadCount: cell.dead_count,
    ...(cell.avg_duration_ms === null || cell.avg_duration_ms === undefined
      ? {}
      : { avgDurationMs: cell.avg_duration_ms }),
    ...(cell.max_duration_ms === null || cell.max_duration_ms === undefined
      ? {}
      : { maxDurationMs: cell.max_duration_ms }),
  };
}

function mockRuntimeHeatmap(): RuntimeHeatmap {
  return {
    bucketSeconds: 300,
    cells: traceRuns.flatMap((trace) =>
      trace.spans
        .filter((span) => span.kind === "event" || span.kind === "function")
        .map<RuntimeHeatmapCell>((span) => ({
          bucketEnd: trace.timestamp,
          bucketStart: trace.timestamp,
          deadCount: span.status === "dead" ? 1 : 0,
          errorCount: span.status === "failed" || span.status === "dead" ? 1 : 0,
          maxDurationMs: span.durationMs,
          nodeType: span.kind === "event" ? "event" : "function",
          service: span.service,
          totalCount: 1,
        }))
    ),
  };
}

function toTraceRun(story: ApiRuntimeStoryDetail): TraceRun {
  const baseTimestamp = Date.parse(story.summary.created_at);
  const spans = story.nodes.map((node, index): TraceSpan => {
    const timestamp = Date.parse(node.timestamp);
    const error = node.error ?? undefined;
    const metadata = node.metadata ?? {};
    const attempts =
      typeof metadata.attempts === "number" ? metadata.attempts : undefined;
    const maxAttempts =
      typeof metadata.max_attempts === "number"
        ? metadata.max_attempts
        : undefined;

    return {
      ...(index > 0 ? { parentId: story.nodes[index - 1]!.id } : {}),
      ...(attempts === undefined ? {} : { attempts }),
      ...(maxAttempts === undefined ? {} : { maxAttempts }),
      attributes: metadata,
      context: {
        correlation_id: story.summary.correlation_id,
      },
      durationMs: node.duration_ms,
      events: [],
      id: node.id,
      kind: toTraceSpanKind(node.type),
      logs: error ? [error] : [],
      name: node.name,
      retryable: node.status === "failed" || node.status === "dead",
      service: node.service,
      startMs: Number.isFinite(timestamp)
        ? Math.max(0, timestamp - baseTimestamp)
        : index,
      status: node.status,
    };
  });

  return {
    correlationId: story.summary.correlation_id,
    durationMs: story.summary.duration,
    id: story.summary.correlation_id,
    name: story.summary.title,
    service: spans[0]?.service ?? "runtime",
    source: "runtime-story",
    spans,
    status: story.summary.status,
    timestamp: story.summary.created_at,
  };
}

function toTraceSpanKind(type: ApiRuntimeStoryNode["type"]): TraceSpan["kind"] {
  switch (type) {
    case "request": {
      return "http";
    }
    case "function": {
      return "function";
    }
    case "event": {
      return "event";
    }
    case "worker": {
      return "runtime";
    }
    case "external": {
      return "external";
    }
    case "unknown": {
      return "runtime";
    }
    default: {
      const exhaustive: never = type;
      return exhaustive;
    }
  }
}

function toActor(value: unknown): Actor {
  if (!value || typeof value !== "object" || !("kind" in value)) {
    return { kind: "system" };
  }

  const actor = value as Partial<Actor>;
  if (actor.kind === "anonymous" || actor.kind === "system") {
    return { kind: actor.kind };
  }
  if (actor.kind === "user" || actor.kind === "service") {
    return {
      kind: actor.kind,
      id: "id" in actor && typeof actor.id === "string" ? actor.id : "-",
      scopes: Array.isArray(actor.scopes) ? actor.scopes : [],
    };
  }
  return { kind: "system" };
}
