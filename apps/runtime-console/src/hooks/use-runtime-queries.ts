import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import type {
  AdminFunctionRunListResponse,
  AdminRuntimeFunctionRunItem,
  AdminRuntimeOutboxItem,
  AdminRuntimeSummaryItem as ApiRuntimeSummaryItem,
  AdminRuntimeSummaryResponse as ApiRuntimeSummaryResponse,
  AdminOutboxListResponse,
} from "../../../../packages/ts-sdk/src/generated/types";
import {
  correlationId,
  type Actor,
  type ExecutionPayload,
  type FunctionRun,
  functionRuns,
  queueHealth,
  type RuntimeEvent,
  runtimeEvents,
  type TimelineItem,
  type TechnicalOperation,
  timelineItems,
  type RuntimeStory,
  runtimeStories,
  type RuntimeStatus,
} from "../data/mock-runtime";
import { httpClient, isApiMode } from "../lib/http-client";
import {
  normalizeRuntimeHeatmap,
  normalizeExecutionPayload,
  normalizeRuntimeStory,
  normalizeRuntimeStoryListResponse,
  normalizeTechnicalOperations,
  normalizeTimelineItems,
  type ApiRuntimeHeatmapResponse,
  type ApiExecutionPayloadResponse,
  type ApiRuntimeStoryDetailResponse,
  type ApiRuntimeStoryListResponse,
  type ApiTimelineResponse,
  type ApiTechnicalOperationResponse,
  type RuntimeHeatmap,
  type RuntimeHeatmapCell,
} from "./runtime-api-model";

export const runtimeQueryKeys = {
  summary: ["runtime", "summary"] as const,
  events: ["runtime", "events"] as const,
  functions: ["runtime", "functions"] as const,
  heatmap: ["runtime", "heatmap"] as const,
  timeline: (id: string) => ["runtime", "timeline", id] as const,
  technicalOperationsForStory: (id: string) =>
    ["runtime", "stories", id, "technical-operations"] as const,
  technicalOperationsForExecution: (id: string) =>
    ["runtime", "executions", id, "technical-operations"] as const,
  executionPayload: (id: string) =>
    ["runtime", "executions", id, "payload"] as const,
  stories: ["runtime", "stories"] as const,
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

export type { RuntimeHeatmap, RuntimeHeatmapCell };
export type { ExecutionPayload, TechnicalOperation };

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

export function useRuntimeStories() {
  return useQuery({
    queryKey: runtimeQueryKeys.stories,
    queryFn: async () => (isApiMode() ? fetchRuntimeStories() : runtimeStories),
  });
}

export function useStoryTechnicalOperations(storyCorrelationId: string) {
  return useQuery({
    enabled: Boolean(storyCorrelationId),
    queryKey: runtimeQueryKeys.technicalOperationsForStory(storyCorrelationId),
    queryFn: async () =>
      isApiMode()
        ? fetchStoryTechnicalOperations(storyCorrelationId)
        : ([] satisfies TechnicalOperation[]),
  });
}

export function useExecutionTechnicalOperations(nodeId: string) {
  return useQuery({
    enabled: Boolean(nodeId),
    queryKey: runtimeQueryKeys.technicalOperationsForExecution(nodeId),
    queryFn: async () =>
      isApiMode()
        ? fetchExecutionTechnicalOperations(nodeId)
        : ([] satisfies TechnicalOperation[]),
  });
}

export function useExecutionPayload(
  story: RuntimeStory,
  nodeId: string,
  enabled: boolean
) {
  return useQuery({
    enabled: Boolean(nodeId) && enabled,
    queryKey: runtimeQueryKeys.executionPayload(nodeId),
    queryFn: async () =>
      isApiMode()
        ? fetchExecutionPayload(nodeId)
        : mockExecutionPayload(story, nodeId),
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

async function fetchRuntimeSummary(): Promise<RuntimeSummary> {
  const response = await httpClient
    .get("admin/runtime/summary")
    .json<ApiRuntimeSummaryResponse>();

  return {
    status: normalizeSummaryStatus(response.status),
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
    .json<AdminOutboxListResponse>();
  return response.data.map(toRuntimeEvent);
}

async function fetchRuntimeFunctions(): Promise<FunctionRun[]> {
  const response = await httpClient
    .get("admin/runtime/functions")
    .json<AdminFunctionRunListResponse>();
  return response.data.map(toFunctionRun);
}

async function fetchRuntimeTimeline(id: string): Promise<TimelineItem[]> {
  const response = await httpClient
    .get(`admin/runtime/timeline/${encodeURIComponent(id)}`)
    .json<ApiTimelineResponse>();
  return normalizeTimelineItems(response, id);
}

async function fetchRuntimeHeatmap(): Promise<RuntimeHeatmap> {
  const response = await httpClient
    .get("admin/runtime/heatmap")
    .json<ApiRuntimeHeatmapResponse>();
  return normalizeRuntimeHeatmap(response);
}

async function fetchRuntimeStories(): Promise<RuntimeStory[]> {
  const response = await httpClient
    .get("admin/runtime/stories")
    .json<ApiRuntimeStoryListResponse>();
  const { stories } = normalizeRuntimeStoryListResponse(response);

  const details = await Promise.all(
    stories.map((story) =>
      fetchRuntimeStory(story.correlationId)
        .then((detail) => mergeStoryDetail(story, detail))
        .catch(() => story)
    )
  );

  return details;
}

async function fetchRuntimeStory(
  storyCorrelationId: string
): Promise<RuntimeStory> {
  const response = await httpClient
    .get(`admin/runtime/stories/${encodeURIComponent(storyCorrelationId)}`)
    .json<ApiRuntimeStoryDetailResponse>();
  if (!response.data) {
    throw new Error("Runtime story detail response did not include data");
  }
  return normalizeRuntimeStory(response.data);
}

async function fetchStoryTechnicalOperations(
  storyCorrelationId: string
): Promise<TechnicalOperation[]> {
  const response = await httpClient
    .get(
      `admin/runtime/stories/${encodeURIComponent(storyCorrelationId)}/technical-operations`
    )
    .json<ApiTechnicalOperationResponse>();
  return normalizeTechnicalOperations(response);
}

async function fetchExecutionTechnicalOperations(
  nodeId: string
): Promise<TechnicalOperation[]> {
  const response = await httpClient
    .get(
      `admin/runtime/executions/${encodeURIComponent(nodeId)}/technical-operations`
    )
    .json<ApiTechnicalOperationResponse>();
  return normalizeTechnicalOperations(response);
}

async function fetchExecutionPayload(
  nodeId: string
): Promise<ExecutionPayload> {
  const response = await httpClient
    .get(`admin/runtime/executions/${encodeURIComponent(nodeId)}/payload`)
    .json<ApiExecutionPayloadResponse>();
  return normalizeExecutionPayload(response);
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

function mockExecutionPayload(
  story: RuntimeStory,
  nodeId: string
): ExecutionPayload {
  const node = story.nodes.find((item) => item.id === nodeId);
  return {
    input: node?.payload,
    metadata: node?.attributes,
    output: undefined,
    redactedFields: [],
  };
}

function toSummaryItem(item: ApiRuntimeSummaryItem): RuntimeSummaryItem {
  return {
    type: normalizeSummaryItemType(item.type),
    id: item.id,
    name: item.name,
    status: normalizeRuntimeStatus(item.status),
    attempts: item.attempts,
    maxAttempts: item.max_attempts,
    correlationId: item.correlation_id ?? "-",
    createdAt: item.created_at,
    ...(item.last_error ? { lastError: item.last_error } : {}),
  };
}

function toRuntimeEvent(event: AdminRuntimeOutboxItem): RuntimeEvent {
  return {
    id: event.id,
    eventName: event.event_name,
    status: normalizeRuntimeStatus(event.status),
    attempts: event.attempts,
    maxAttempts: event.max_attempts,
    aggregateId: "-",
    aggregateType: "-",
    correlationId: event.correlation_id,
    causationId: "-",
    createdAt: event.created_at,
    ...(event.published_at ? { publishedAt: event.published_at } : {}),
    ...(event.last_error ? { lastError: event.last_error } : {}),
    actor: toActor(undefined),
    payload: {},
  };
}

function toFunctionRun(run: AdminRuntimeFunctionRunItem): FunctionRun {
  return {
    id: run.id,
    functionName: run.function_name,
    status: normalizeRuntimeStatus(run.status),
    attempts: run.attempts,
    maxAttempts: run.max_attempts,
    correlationId: run.correlation_id,
    createdAt: run.created_at,
    ...(run.started_at ? { startedAt: run.started_at } : {}),
    ...(run.completed_at ? { completedAt: run.completed_at } : {}),
    ...(run.locked_by ? { lockedBy: run.locked_by } : {}),
    ...(run.last_error ? { lastError: run.last_error } : {}),
    actor: toActor(undefined),
    input: {},
    logs: run.last_error ? [run.last_error] : [],
  };
}

function normalizeSummaryStatus(status: string): RuntimeSummaryStatus {
  return status === "healthy" || status === "degraded" || status === "failing"
    ? status
    : "degraded";
}

function normalizeSummaryItemType(type: string): RuntimeSummaryItem["type"] {
  return type === "function_run" ? "function_run" : "outbox_event";
}

function normalizeRuntimeStatus(status: string): RuntimeStatus {
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

function mockRuntimeHeatmap(): RuntimeHeatmap {
  return {
    bucketSeconds: 300,
    cells: runtimeStories.flatMap((story) =>
      story.nodes
        .filter((node) => node.kind === "event" || node.kind === "function")
        .map<RuntimeHeatmapCell>((node) => ({
          bucketEnd: story.timestamp,
          bucketStart: story.timestamp,
          deadCount: node.status === "dead" ? 1 : 0,
          errorCount:
            node.status === "failed" || node.status === "dead" ? 1 : 0,
          maxDurationMs: node.durationMs,
          nodeType: node.kind === "event" ? "event" : "function",
          service: node.service,
          totalCount: 1,
        }))
    ),
  };
}

function mergeStoryDetail(summary: RuntimeStory, detail: RuntimeStory) {
  return {
    ...detail,
    name: detail.name || summary.name,
  };
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
