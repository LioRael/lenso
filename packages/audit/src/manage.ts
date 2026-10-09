import { definePlugin, type Plugin } from "@lenso/core";
import { defineOperation } from "@lenso/engine/operations";
import { defineManage } from "@lenso/manage";
import { z } from "zod";
import { AuditError, type AuditScope } from "./contracts";
import type { AuditService } from "./service";
import { eventIdPattern } from "./validation";

const identifier = (max = 128) =>
  z
    .string()
    .min(1)
    .max(max)
    .regex(/^[A-Za-z0-9][A-Za-z0-9._:/-]*$/);
const scope = z.strictObject({ tenantId: identifier().nullable(), scopeId: identifier() });
const queryInput = z.strictObject({
  scope,
  limit: z.number().int().min(1).max(500).optional(),
  cursor: z
    .strictObject({
      recordedAt: z.number().int().nonnegative(),
      id: z.string().regex(eventIdPattern),
    })
    .optional(),
  action: identifier().optional(),
  target: z
    .strictObject({
      type: identifier(64),
      id: identifier(256),
    })
    .optional(),
  result: z.enum(["intent", "success", "failure", "denied", "unknown"]).optional(),
  correlationId: identifier().optional(),
  recordedFrom: z.number().int().nonnegative().optional(),
  recordedTo: z.number().int().nonnegative().optional(),
});

const readMetadata = {
  effect: "read",
  destructive: false,
  retry: "safe",
  cancellation: "none",
  mapError(error: unknown) {
    if (!(error instanceof AuditError)) return undefined;
    return {
      code: error.code === "unauthorized" ? "forbidden" : error.code,
      phase: "invoke" as const,
      message: error.code === "unauthorized" ? "Audit access denied." : "Audit read failed.",
    };
  },
} as const;

/** Creating this companion does not install it or open a transport. No append tool is declared. */
export function createAuditManage<P>(options: {
  id: string;
  audit: Plugin<AuditService<P>>;
  scoped?: { id: string };
}) {
  const plugin = definePlugin({
    id: options.id,
    requires: [options.audit],
    setup(lifecycle) {
      const audit = lifecycle.get(options.audit);
      return {
        query(input: z.input<typeof queryInput>, context: { principal: P }) {
          return audit.query(input, context.principal);
        },
      };
    },
  });
  const operation = defineOperation({
    plugin,
    method: "query",
    context: true,
    input: queryInput,
    ...readMetadata,
    description: "Query audit events within an explicitly authorized tenant and scope.",
    outputDescription: "A bounded page of whitelist-only audit events, with no global count.",
  });
  const manage = defineManage({ plugin, operations: [operation] });
  return {
    plugin,
    operation,
    manage,
    scoped: options.scoped
      ? createScopedAuditManage({ id: options.scoped.id, audit: options.audit })
      : undefined,
  };
}

/** Optional browser-facing companion. Only the trusted entry supplies scope and principal. */
export function createScopedAuditManage<P>(options: {
  id: string;
  audit: Plugin<AuditService<P>>;
}) {
  const scopedQueryInput = queryInput.omit({ scope: true });
  const getInput = z.strictObject({ id: z.string().regex(eventIdPattern) });
  const plugin = definePlugin({
    id: options.id,
    requires: [options.audit],
    setup(lifecycle) {
      const audit = lifecycle.get(options.audit);
      return {
        query(
          input: z.input<typeof scopedQueryInput>,
          context: { principal: P; scope: AuditScope },
        ) {
          return audit.query(
            { ...scopedQueryInput.parse(input), scope: context.scope },
            context.principal,
          );
        },
        get(input: z.input<typeof getInput>, context: { principal: P; scope: AuditScope }) {
          return audit.get({ ...getInput.parse(input), scope: context.scope }, context.principal);
        },
      };
    },
  });
  const query = defineOperation({
    plugin,
    method: "query",
    context: true,
    input: scopedQueryInput,
    ...readMetadata,
    description: "Query audit events in the scope bound by the trusted entry.",
    outputDescription: "A bounded page of whitelist-only audit events, with no global count.",
  });
  const get = defineOperation({
    plugin,
    method: "get",
    context: true,
    input: getInput,
    ...readMetadata,
    description: "Read one audit event in the scope bound by the trusted entry.",
    outputDescription: "An authorized whitelist-only audit event, or null.",
  });
  const operations = [query, get] as const;
  const manage = defineManage({ plugin, operations });
  return { plugin, operations, manage };
}
