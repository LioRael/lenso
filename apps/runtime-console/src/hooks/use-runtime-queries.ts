import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  correlationId,
  functionRuns,
  runtimeEvents,
  timelineItems,
} from "../data/mock-runtime";

export const runtimeQueryKeys = {
  events: ["runtime", "mock", "events"] as const,
  functions: ["runtime", "mock", "functions"] as const,
  timeline: (id: string) => ["runtime", "mock", "timeline", id] as const,
  deadLetters: ["runtime", "mock", "dead-letters"] as const,
};

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
        queryClient.invalidateQueries({ queryKey: runtimeQueryKeys.events }),
        queryClient.invalidateQueries({ queryKey: runtimeQueryKeys.functions }),
        queryClient.invalidateQueries({
          queryKey: runtimeQueryKeys.deadLetters,
        }),
      ]);
    },
  });
}
