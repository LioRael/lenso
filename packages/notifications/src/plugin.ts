import { bindConfig, definePluginConfig, type ConfigSource } from "@lenso/core/config";
import type { Plugin } from "@lenso/core/plugin";
import { z } from "zod";
import type { NotificationChannel, NotificationStore, NotificationTemplate } from "./contracts";
import { createNotificationService } from "./service";

export const notificationConfig = definePluginConfig({
  description: "Notification consent default and delivery claim lifetime",
  schema: z.strictObject({
    optionalDefault: z.enum(["enabled", "disabled"]).default("disabled"),
    leaseMs: z.number().int().min(1000).max(3_600_000).default(120_000),
  }),
});
export type NotificationRuntimeConfig = z.input<typeof notificationConfig.schema>;

export function createNotificationPlugin<D>(options: {
  id: string;
  database: Plugin<D>;
  store: (database: D) => NotificationStore;
  channels: readonly Plugin<NotificationChannel>[];
  templates: readonly NotificationTemplate[];
  config?: NotificationRuntimeConfig | readonly ConfigSource[];
}) {
  return bindConfig(notificationConfig, options.config ?? {}, {
    id: options.id,
    requires: [options.database, ...options.channels],
    setup(context, config) {
      // All resources are borrowed from their exact owning plugin instances.
      return createNotificationService({
        store: options.store(context.get(options.database)),
        channels: options.channels.map((channel) => context.get(channel)),
        templates: options.templates,
        ...config,
        onDelivery(event) {
          context.logger?.info(event, "Notification delivery state changed");
        },
      });
    },
  });
}
