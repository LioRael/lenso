import { definePluginConfig, type ConfigBinding } from "@lenso/core/config";
import { z } from "zod";

export const paymentsConfigSchema = z
  .object({
    leaseMs: z.number().int().min(1_000).max(3_600_000).default(60_000),
    reconcileDelayMs: z.number().int().min(1_000).max(86_400_000).default(60_000),
    maxRefunds: z.number().int().min(1).max(1_000).default(100),
  })
  .strict();

export const paymentsConfig = definePluginConfig({
  schema: paymentsConfigSchema,
  description: "Payments concurrency and bounded reconciliation, not provider credentials.",
  jsonSchema: () => z.toJSONSchema(paymentsConfigSchema),
});
export type PaymentsConfigBinding = ConfigBinding<typeof paymentsConfigSchema>;
