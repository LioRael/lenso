import { defineTask, type Task, type TaskQueue } from "@lenso/tasks";
import { z } from "zod";
import type { PaymentsRuntime } from "./index";
import { PaymentsError } from "./contracts";

/** Register this exact Task object in the existing queue; credentials never enter the payload. */
export function createPaymentsReconciliationTask(options: {
  name: string;
  runtime: () => PaymentsRuntime<unknown>["reconciliation"];
  queue: () => Pick<TaskQueue, "enqueue">;
  /** Disable when Scheduler owns periodic recovery; avoids accumulating parallel polling chains. */
  continuation?: boolean;
}) {
  if (options.continuation !== undefined && typeof options.continuation !== "boolean")
    throw new PaymentsError("invalid-input");
  const input = z.object({ limit: z.number().int().min(1).max(200).default(50) }).strict();
  type Result = Awaited<ReturnType<PaymentsRuntime<unknown>["reconciliation"]["drain"]>>;
  const task: Task<typeof input, Result> = defineTask({
    name: options.name,
    input,
    maxAttempts: 5,
    retry: { delaySeconds: 30, backoff: true, maxDelaySeconds: 300 },
    async handler({ limit }, context) {
      context.signal.throwIfAborted();
      const result = await options.runtime().drain(limit);
      context.signal.throwIfAborted();
      if (options.continuation !== false && result.nextRunAt !== null) {
        await options.queue().enqueue(
          task,
          { limit },
          {
            runAt: new Date(Math.max(result.nextRunAt, Date.now() + 1_000)),
            deduplicationKey: `payments.continue:${context.jobId}:${result.nextRunAt}`,
          },
        );
      }
      return result;
    },
    result: (value) => ({
      payments: value.payments,
      events: value.events,
      unresolved: value.unresolved,
      nextRunAt: value.nextRunAt,
    }),
  });
  return task;
}
