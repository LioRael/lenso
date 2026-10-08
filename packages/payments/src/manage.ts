import { definePlugin, type Plugin } from "@lenso/core/plugin";
import { defineOperation } from "@lenso/engine/operations";
import { defineManage } from "@lenso/manage";
import { z } from "zod";
import type { PaymentsRuntime } from "./index";

/** Explicit opt-in, read-only. The entry binding supplies an Auth-produced actor, not input JSON. */
export function createPaymentsManage<A>(options: {
  id: string;
  payments: Plugin<PaymentsRuntime<A>>;
}) {
  const plugin = definePlugin({
    id: options.id,
    requires: [options.payments],
    setup(context) {
      const payments = context.get(options.payments).payments;
      return {
        status(input: { paymentId: string }, invocation: { actor: A }) {
          return payments.get(input, invocation.actor);
        },
      };
    },
  });
  const operations = [
    defineOperation({
      plugin,
      method: "status",
      context: true,
      input: z.object({ paymentId: z.string().min(1).max(256) }).strict(),
      description: "Read an authorized payment and refund status, without payment secrets.",
      effect: "read",
      destructive: false,
      retry: "safe",
      cancellation: "none",
      outputDescription: "Safe payment status; no client secret or provider credentials.",
      source: { file: "packages/payments/src/manage.ts", export: "createPaymentsManage" },
    }),
  ];
  return { plugin, operations, manage: defineManage({ plugin, operations }) };
}
