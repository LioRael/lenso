import { definePlugin, type Plugin } from "@lenso/core/plugin";
import { defineOperation } from "@lenso/engine/operations";
import { defineManage } from "@lenso/manage";
import { z } from "zod";
import type { ApiKeys, KeyQuery, ListKeysInput } from "./index";

const identifier = z
  .string()
  .min(1)
  .max(256)
  .refine((value) => !!value.trim());
const subject = z
  .object({
    namespace: identifier,
    tenantId: identifier,
    subjectId: identifier,
  })
  .strict();
const query = z.object({ subject, id: identifier }).strict();
const list = z
  .object({
    subject,
    after: identifier.optional(),
    limit: z.number().int().min(1).max(100).optional(),
  })
  .strict();

/** Opt-in safe metadata operations. Issuance/rotation require a protected secret delivery channel. */
export function createApiKeyManage<C, R>(keys: Plugin<ApiKeys<C, R>>, id: string) {
  const plugin = definePlugin({
    id,
    requires: [keys],
    setup(context) {
      const service = context.get(keys);
      return {
        list: (input: ListKeysInput, invocation: { caller: C }) =>
          service.list(input, invocation.caller),
        read: (input: KeyQuery, invocation: { caller: C }) =>
          service.read(input, invocation.caller),
        revoke: (input: KeyQuery, invocation: { caller: C }) =>
          service.revoke(input, invocation.caller),
      };
    },
  });
  const operations = [
    defineOperation({
      plugin,
      method: "list",
      input: list,
      context: true,
      description: "List authorized subject API key metadata.",
      effect: "read",
      retry: "safe",
      cancellation: "none",
    }),
    defineOperation({
      plugin,
      method: "read",
      input: query,
      context: true,
      description: "Read authorized API key metadata without credential material.",
      effect: "read",
      retry: "safe",
      cancellation: "none",
    }),
    defineOperation({
      plugin,
      method: "revoke",
      input: query,
      context: true,
      description: "Revoke an authorized API key and its overlapping predecessor.",
      effect: "write",
      destructive: true,
      retry: "safe",
      cancellation: "none",
    }),
  ];
  return { plugin, operations, manage: defineManage({ plugin, operations }) };
}
