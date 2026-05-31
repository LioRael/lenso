import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  correlationId,
  functionRuns,
  queueHealth,
  runtimeEvents,
  timelineItems,
  traceRuns,
  type RuntimeStatus,
} from "../data/mock-runtime";

export const runtimeQueryKeys = {
  summary: ["runtime", "summary"] as const,
  events: ["runtime", "mock", "events"] as const,
  functions: ["runtime", "mock", "functions"] as const,
  timeline: (id: string) => ["runtime", "mock", "timeline", id] as const,
  traces: ["runtime", "mock", "traces"] as const,
  deadLetters: ["runtime", "mock", "dead-letters"] as const,
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

export function useRuntimeSummary() {
  return useQuery({
    queryKey: runtimeQueryKeys.summary,
    queryFn: async () => mockSummary(),
  });
}

export function useRuntimeEvents() {
  return useQuery({
    queryKey: runtimeQueryKeys.events,
    queryFn: async () => runtimeEvents,
  });
}

export function useRuntimeFunctions() {
  return useQuery({
    queryKey: runtimeQueryKeys.functions,
    queryFn: async () => functionRuns,
  });
}

export function useRuntimeTimeline(activeCorrelationId: string) {
  return useQuery({
    queryKey: runtimeQueryKeys.timeline(activeCorrelationId),
    queryFn: async () =>
      timelineItems.filter(
        (item) => item.correlationId === (activeCorrelationId || correlationId)
      ),
  });
}

export function useDeadLetters() {
  return useQuery({
    queryKey: runtimeQueryKeys.deadLetters,
    queryFn: async () => [
      ...runtimeEvents
        .filter((event) => event.status === "failed" || event.status === "dead")
        .map((item) => ({ kind: "event" as const, item })),
      ...functionRuns
        .filter((run) => run.status === "failed" || run.status === "dead")
        .map((item) => ({ kind: "function" as const, item })),
    ],
  });
}

export function useRuntimeTraces() {
  return useQuery({
    queryKey: runtimeQueryKeys.traces,
    queryFn: async () => traceRuns,
  });
}

export function useRetryRuntimeWork() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (_input: {
      kind: "event" | "function" | "timeline";
      id: string;
    }) => {
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
