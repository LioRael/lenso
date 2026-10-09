import { definePlugin, type Plugin } from "@lenso/core/plugin";
import { defineOperation } from "@lenso/engine/operations";
import { defineManage } from "@lenso/manage";
import { z } from "zod";
import type { WebhookContext } from "./contracts";
import type { Webhooks } from "./service";
import { WebhookError } from "./contracts";

export function createWebhooksManage<P>(
  options?:
    | {
        readonly enabled: true;
        readonly id: string;
        readonly webhooks: Plugin<Webhooks<P>>;
      }
    | { readonly enabled?: false },
) {
  if (!options || options.enabled !== true) return undefined;
  const plugin = definePlugin({
    id: options.id,
    requires: [options.webhooks],
    setup(context) {
      const service = context.get(options.webhooks);
      return {
        list(
          input: { limit?: number; cursor?: { createdAt: number; id: string } },
          invocation: WebhookContext<P>,
        ) {
          return service.listDeliveries(input, invocation);
        },
        detail(input: { id: string }, invocation: WebhookContext<P>) {
          return service.getDelivery(input, invocation);
        },
        attempts(
          input: { deliveryId: string; limit?: number; cursor?: { createdAt: number; id: string } },
          invocation: WebhookContext<P>,
        ) {
          return service.listAttempts(input, invocation);
        },
        async replay(input: { id: string }, invocation: WebhookContext<P>) {
          try {
            return { status: "created" as const, ...(await service.replay(input, invocation)) };
          } catch (error) {
            if (
              error instanceof WebhookError &&
              error.code === "replay-outcome-unknown" &&
              error.referenceId &&
              z.uuid().safeParse(error.referenceId).success
            ) {
              return { status: "reconciliation-required" as const, referenceId: error.referenceId };
            }
            throw error;
          }
        },
      };
    },
  });
  const page = z
    .object({
      limit: z.number().int().min(1).max(100).optional(),
      cursor: z
        .object({ createdAt: z.number().int().nonnegative(), id: z.uuid() })
        .strict()
        .optional(),
    })
    .strict();
  const metadata = {
    plugin,
    context: true as const,
    cancellation: "none" as const,
    source: { file: "packages/webhooks/src/manage.ts", export: "createWebhooksManage" },
    mapError(error: unknown) {
      if (error instanceof WebhookError)
        return { code: `webhooks-${error.code}`, phase: "invoke" as const, message: error.message };
      return undefined;
    },
  };
  const operations = [
    defineOperation({
      ...metadata,
      method: "list",
      input: page,
      effect: "read",
      retry: "safe",
      description: "List authorized webhook delivery status without payloads or credentials.",
    }),
    defineOperation({
      ...metadata,
      method: "detail",
      input: z.object({ id: z.uuid() }).strict(),
      effect: "read",
      retry: "safe",
      description: "Read authorized webhook delivery status.",
    }),
    defineOperation({
      ...metadata,
      method: "attempts",
      input: page.extend({ deliveryId: z.uuid() }),
      effect: "read",
      retry: "safe",
      description: "Read bounded, redacted delivery attempts.",
    }),
    defineOperation({
      ...metadata,
      method: "replay",
      input: z.object({ id: z.uuid() }).strict(),
      effect: "write",
      retry: "unsafe",
      confirmation: "required",
      approval: "required",
      description: "Create an audited webhook replay with the original event identity.",
      outputDescription: "New delivery reference; queue admission does not prove HTTP delivery.",
    }),
  ];
  return {
    plugin,
    operations,
    manage: defineManage({
      plugin,
      operations,
      views: [
        {
          key: "webhooks.deliveries",
          title: "Webhook deliveries",
          columns: ["id", "eventId", "state", "attemptCount", "updatedAt"],
          detail: "detail",
          action: "replay",
        },
      ],
    }),
  };
}
