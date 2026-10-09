import { defineTask } from "@lenso/tasks";
import { z } from "zod";

export const webhookTaskInput = z.object({
  deliveryId: z.uuid(),
  generation: z.number().int().min(1).max(Number.MAX_SAFE_INTEGER),
}).strict();

export function defineWebhookTask(
  name: string,
  execute: (input: z.infer<typeof webhookTaskInput>, signal: AbortSignal) => Promise<void>,
) {
  return defineTask({
    name,
    input: webhookTaskInput,
    maxAttempts: 1,
    handler: (input, context) => execute(input, context.signal),
  });
}

export type WebhookTask = ReturnType<typeof defineWebhookTask>;
