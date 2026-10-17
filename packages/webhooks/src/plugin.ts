import {
  createWebhookReceiver,
  type WebhookReceiverOptions,
  type WebhookReceiver,
  type WebhookInbox,
} from "./receiver";
import { definePlugin, type Plugin, type PluginContext } from "@lenso/core/plugin";
import type { TaskQueue } from "@lenso/tasks";
import type { WebhookOptions, WebhookRepository } from "./contracts";
import { createWebhooks, type Webhooks } from "./service";

/** Assembly only: all resources and worker lifetime remain with their exact host instances. */
export function createWebhooksPlugin<D, P>(options: {
  readonly id: string;
  readonly database: Plugin<D>;
  readonly queue: Plugin<TaskQueue>;
  readonly requires?: readonly Plugin<unknown>[];
  readonly connect: (
    database: D,
    context: PluginContext,
  ) => WebhookRepository | Promise<WebhookRepository>;
  readonly dependencies: (
    context: PluginContext,
  ) => Omit<WebhookOptions<P>, "repository" | "queue">;
}): Plugin<Webhooks<P>> {
  return definePlugin({
    id: options.id,
    requires: [options.database, options.queue, ...(options.requires ?? [])],
    async setup(context) {
      return createWebhooks({
        ...options.dependencies(context),
        repository: await options.connect(context.get(options.database), context),
        queue: context.get(options.queue),
      });
    },
  });
}

/** Optional inbound assembly. The inbox and all key dependencies belong to the host. */
export function createWebhookReceiverPlugin(options: {
  readonly id: string;
  readonly inbox: Plugin<WebhookInbox>;
  readonly requires?: readonly Plugin<unknown>[];
  readonly options: (context: PluginContext) => Omit<WebhookReceiverOptions, "inbox">;
}): Plugin<WebhookReceiver> {
  return definePlugin({
    id: options.id,
    requires: [options.inbox, ...(options.requires ?? [])],
    setup(context) {
      return createWebhookReceiver({
        ...options.options(context),
        inbox: context.get(options.inbox),
      });
    },
  });
}
