import { createTaskQueue, type TaskProvider } from "@lenso/tasks";
import {
  createWebhooks,
  createPinnedHttpsTransport,
  defineWebhookTask,
  webhookConfig,
  type WebhookOptions,
  type Webhooks,
} from "@lenso/webhooks";

/** Host passes already provisioned resources, verified authority and its secret resolver. */
export function assembleWebhooks<P>(
  input: Omit<WebhookOptions<P>, "queue" | "task" | "transport"> & {
    provider: TaskProvider;
  },
) {
  const config = webhookConfig(input.config);
  let service: Webhooks<P>;
  const task = defineWebhookTask("partner.webhooks.deliver", (job, signal) =>
    service.execute(job, signal),
  );
  const queue = createTaskQueue({ provider: input.provider, tasks: [task] });
  service = createWebhooks({
    ...input,
    config,
    queue,
    task,
    transport: createPinnedHttpsTransport(config.outbound),
  });
  return { webhooks: service, queue };
}

// In the host's explicit startup:
// 1. Apply reviewed Webhooks, Tasks, Audit and Limits migrations to its database.
// 2. Create/borrow the repository, audit, limits, secret resolver and Tasks provider.
// 3. Assemble, await webhooks.recover({limit:100}), then start the queue worker.
// 4. Call recover periodically through the host's existing maintenance/Scheduler entry.
// 5. Stop/drain the worker before closing queue/provider, Limits and database.
//
// After the host commits an order:
// await webhooks.publish({type:"order.completed", data:{orderId}}, trustedContext);
// Record eventId/dispatch in the host's reconciliation workflow.
// The order commit and this publish are NOT one atomic transaction.
