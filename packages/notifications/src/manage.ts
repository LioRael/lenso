import type { Access } from "@lenso/auth";
import { definePlugin, type Plugin } from "@lenso/core/plugin";
import { defineOperation } from "@lenso/engine/operations";
import { defineManage } from "@lenso/manage";
import {
  createAuthorizedNotificationService,
  notificationQueryInput,
  notificationListInput,
  notificationPreferenceInput,
  notificationSetPreferenceInput,
  type AuthorizedNotificationOptions,
  type NotificationResource,
} from "./auth";
import type { NotificationService } from "./service";

export interface NotificationsManageOptions<
  R extends string,
  E,
  S extends string,
  A extends string,
  M = undefined,
> {
  readonly enabled: true;
  readonly id?: string;
  readonly notifications: Plugin<NotificationService>;
  readonly authentication: Plugin<Access<R, E, S, A, NotificationResource, M>>;
  readonly tenantFor: AuthorizedNotificationOptions<R, E, S, A, M>["tenantFor"];
  readonly managePolicy: NonNullable<AuthorizedNotificationOptions<R, E, S, A, M>["managePolicy"]>;
  readonly requeue: NonNullable<AuthorizedNotificationOptions<R, E, S, A, M>["requeue"]>;
}

/** Disabled by default: undefined means there is no plugin or exposure to install. */
export function createNotificationsManage<
  R extends string,
  E,
  S extends string,
  A extends string,
  M = undefined,
>(options?: NotificationsManageOptions<R, E, S, A, M> | { readonly enabled?: false }) {
  if (!options || options.enabled !== true) return undefined;
  if (
    !options.notifications ||
    !options.authentication ||
    typeof options.tenantFor !== "function" ||
    typeof options.managePolicy !== "function" ||
    typeof options.requeue !== "function"
  ) {
    throw new TypeError("Notification management requires trusted dependencies and policy.");
  }
  const source = {
    file: "packages/notifications/src/manage.ts",
    export: "createNotificationsManage",
  };
  const plugin = definePlugin({
    id: options.id ?? `${options.notifications.id}.manage`,
    source,
    requires: [options.notifications, options.authentication],
    setup(context) {
      return createAuthorizedNotificationService({
        service: context.get(options.notifications),
        access: context.get(options.authentication),
        tenantFor: options.tenantFor,
        managePolicy: options.managePolicy,
        requeue: options.requeue,
      });
    },
  });
  const metadata = { plugin, context: true as const, source, cancellation: "none" as const };
  const operations = [
    defineOperation({
      ...metadata,
      method: "query",
      input: notificationQueryInput,
      description: "Read owned notification status.",
      effect: "read",
      retry: "safe",
    }),
    defineOperation({
      ...metadata,
      method: "list",
      input: notificationListInput,
      description: "List notification status for the current tenant and subject.",
      effect: "read",
      retry: "safe",
    }),
    defineOperation({
      ...metadata,
      method: "getPreference",
      input: notificationPreferenceInput,
      description: "Read the current subject's notification preference.",
      effect: "read",
      retry: "safe",
    }),
    defineOperation({
      ...metadata,
      method: "setPreference",
      input: notificationSetPreferenceInput,
      description: "Write the current subject's notification preference.",
      effect: "write",
      retry: "safe",
    }),
    defineOperation({
      ...metadata,
      method: "adminQuery",
      input: notificationQueryInput,
      description: "Read notification status with explicit tenant management policy.",
      effect: "read",
      retry: "safe",
    }),
    defineOperation({
      ...metadata,
      method: "attempts",
      input: notificationQueryInput,
      description: "Read redacted delivery attempts with explicit management policy.",
      effect: "read",
      retry: "safe",
    }),
    defineOperation({
      ...metadata,
      method: "retry",
      input: notificationQueryInput,
      description: "Requeue a retryable notification through the durable dispatcher.",
      effect: "write",
      retry: "unsafe",
      outputDescription: "queued indicates dispatch, not delivery or acceptance.",
    }),
  ];
  return { plugin, operations, manage: defineManage({ plugin, operations }) };
}
