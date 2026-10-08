import { defineTask, type Task, type TaskQueue } from "@lenso/tasks";
import { z } from "zod";
import { NotificationError } from "./errors";
import type { NotificationInput } from "./contracts";
import type { NotificationService } from "./service";

export const notificationTaskInput = z.strictObject({ notificationId: z.uuid() });

export function createNotificationTask(options: {
  name: string;
  service: Pick<NotificationService, "deliver">;
  maxAttempts?: number;
  retry?: Task["retry"];
}) {
  return defineTask({
    name: options.name,
    input: notificationTaskInput,
    maxAttempts: options.maxAttempts ?? 3,
    retry: options.retry ?? { delaySeconds: 2, backoff: true, maxDelaySeconds: 60 },
    async handler(input, context) {
      const status = await options.service.deliver(input.notificationId, {
        signal: context.signal,
      });
      if (status?.retryable) throw new NotificationError("delivery-retry");
      return status;
    },
    result: (status) =>
      status
        ? {
            notificationId: status.id,
            state: status.state,
            attemptCount: status.attemptCount,
          }
        : null,
  });
}

export function createNotificationDispatcher(options: {
  service: NotificationService;
  queue: Pick<TaskQueue, "enqueue" | "get" | "retry">;
  task: ReturnType<typeof createNotificationTask>;
}) {
  async function enqueue(id: string) {
    const jobId = await options.queue.enqueue(
      options.task,
      { notificationId: id },
      {
        deduplicationKey: `notification/${id}`,
      },
    );
    // No delivery revision is touched: a worker may already be settling this notification.
    await options.service.recordTaskJob(id, jobId);
    return jobId;
  }
  return {
    async submit(input: NotificationInput) {
      const notification = await options.service.create(input);
      // The committed notification is the outbox. A failed enqueue leaves it recoverable.
      const jobId = notification.retryable ? await enqueue(notification.id) : null;
      return { notification, jobId };
    },
    async requeue(id: string): Promise<boolean> {
      if (!(await options.service.retryEligible(id))) return false;
      const jobId = await enqueue(id);
      const job = await options.queue.get(jobId);
      if (job?.state === "failed") return options.queue.retry(jobId);
      return job?.state === "pending" || job?.state === "running";
    },
    /** Call at worker/producer startup or an existing task. Does not start a timer or new queue. */
    async reconcile(limit = 100) {
      const records = await options.service.recoverable(limit);
      const result: { notificationId: string; jobId: string }[] = [];
      for (const record of records) {
        // Do not silently extend the Tasks attempt budget of a final failed job.
        result.push({ notificationId: record.id, jobId: await enqueue(record.id) });
      }
      return result;
    },
  };
}
