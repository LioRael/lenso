import { defineApp, definePlugin, startApp } from "@lenso/core";
import { EngineError } from "@lenso/engine/diagnostics";
import { defineOperation } from "@lenso/engine/operations";
import { defineManage, selectManageOperations } from "@lenso/manage";
import type { McpAdapterOptions, McpIdentity } from "@lenso/mcp";
import { z } from "zod";

export const testOnlyResource = "http://127.0.0.1/mcp";
export const testOnlyIdentity: McpIdentity = Object.freeze({
  subject: "test-only-local-owner",
  tenant: "test-only-tenant",
  issuer: "https://test-only-issuer.invalid/",
  audience: Object.freeze([testOnlyResource]),
  scopes: Object.freeze(["notes:read", "notes:write"]),
  expiresAt: 4_102_444_800,
});

const lookupInput = z.strictObject({
  tenant: z.string().min(1).max(100),
  id: z.string().min(1).max(100),
});
const writeInput = lookupInput.extend({ text: z.string().min(1).max(1000) });
type ServiceContext = { identity: McpIdentity; signal: AbortSignal };

function authorizeTenant(tenant: string, scope: string, context: ServiceContext): void {
  context.signal.throwIfAborted();
  if (tenant !== context.identity.tenant || !context.identity.scopes.includes(scope))
    throw new EngineError({ code: "FORBIDDEN", phase: "invoke", message: "Access denied." });
}

/** The host starts once; every entry borrows this exact plugin and runtime. */
export async function startHost() {
  const lifecycle = { starts: 0, stops: 0 };
  const notes = definePlugin({
    id: "host-notes",
    setup(context) {
      lifecycle.starts++;
      context.onCleanup(() => {
        lifecycle.stops++;
      });
      const records = new Map<string, string>();
      return {
        async read(input: z.input<typeof lookupInput>, request: ServiceContext) {
          const value = lookupInput.parse(input);
          authorizeTenant(value.tenant, "notes:read", request);
          await Promise.resolve();
          request.signal.throwIfAborted();
          return {
            id: value.id,
            text: records.get(JSON.stringify([value.tenant, value.id])) ?? null,
          };
        },
        async write(input: z.input<typeof writeInput>, request: ServiceContext) {
          const value = writeInput.parse(input);
          authorizeTenant(value.tenant, "notes:write", request);
          await Promise.resolve();
          // Cooperate before committing; cancellation is not rollback.
          request.signal.throwIfAborted();
          records.set(JSON.stringify([value.tenant, value.id]), value.text);
          return { id: value.id, text: value.text };
        },
      };
    },
  });
  const manage = defineManage({
    plugin: notes,
    operations: [
      defineOperation({
        plugin: notes,
        method: "read",
        context: true,
        input: lookupInput,
        description: "Read one note in the caller's tenant.",
        effect: "read",
        destructive: false,
        retry: "safe",
        cancellation: "cooperative",
        source: { file: "src/host.ts", export: "startHost" },
      }),
      defineOperation({
        plugin: notes,
        method: "write",
        context: true,
        input: writeInput,
        description: "Replace one note in the caller's tenant.",
        effect: "write",
        destructive: false,
        retry: "safe",
        cancellation: "cooperative",
        source: { file: "src/host.ts", export: "startHost" },
      }),
    ],
  });
  const operations = selectManageOperations(manage, ["read", "write"]);
  const plugins = [notes];
  const app = await startApp(defineApp({ plugins }));
  const options: McpAdapterOptions<McpIdentity, (typeof operations)[number]> = {
    running: app,
    plugins,
    operations,
    canList: (operation, request) =>
      request.identity.scopes.includes(operation.effect === "read" ? "notes:read" : "notes:write"),
    authorize: (operation, request) =>
      request.identity.scopes.includes(operation.effect === "read" ? "notes:read" : "notes:write"),
    binding: (_operation, _input, request) => ({
      context: { identity: request.identity, signal: request.signal },
    }),
  };
  return { app, notes, manage, options, lifecycle };
}
