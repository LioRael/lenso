import { defineTask } from "@lenso/tasks";
import { z } from "zod";
import type { AuditScope } from "./contracts";
import type { AuditService } from "./service";
import { eventIdPattern } from "./validation";

const reconciliationInput = z.strictObject({
  scope: z.strictObject({
    tenantId: z.string().min(1).max(128).nullable(),
    scopeId: z.string().min(1).max(128),
  }),
  intentId: z.string().regex(eventIdPattern),
});

/**
 * The job carries only a locator. An application-owned resolver must check the
 * real effect and supply a stable new event ID; queue payloads grant no authority.
 */
export function createAuditReconciliationTask<P>(options: {
  name: string;
  audit: AuditService<P>;
  principal(): Promise<P>;
  reconcile(
    intentId: string,
    scope: AuditScope,
  ): Promise<{
    id: string;
    occurredAt: number;
    result: "success" | "failure" | "unknown";
    reasonCode: string;
  }>;
}) {
  return defineTask({
    name: options.name,
    input: reconciliationInput,
    async handler(input) {
      const principal = await options.principal();
      const intent = await options.audit.get({ scope: input.scope, id: input.intentId }, principal);
      if (!intent || intent.result !== "intent")
        throw new Error("Audit reconciliation intent missing");
      const outcome = await options.reconcile(intent.id, intent.scope);
      const result = await options.audit.append(
        {
          ...outcome,
          scope: intent.scope,
          action: intent.action,
          target: intent.target,
          ...(intent.correlationId ? { correlationId: intent.correlationId } : {}),
          relation: { kind: "outcome", eventId: intent.id },
        },
        principal,
      );
      return { eventId: result.event.id };
    },
    result: (value) => value,
  });
}
