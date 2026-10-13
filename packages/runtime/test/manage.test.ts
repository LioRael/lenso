import { expect, test } from "bun:test";
import { defineApp, definePlugin, startApp } from "@lenso/core";
import type { RuntimeHost } from "@lenso/runtime-sdk";
import { createManageAdapter } from "@lenso/manage";
import { createRuntimeManage } from "../src/manage";

// Prevents a selected read operation bypassing service authorization when canList admits it.
// Manage's own tests cannot prove the runtime sidecar checks trusted context before SDK dispatch.
test("runtime status requires current trusted context, not a payload principal", async () => {
  let reads = 0;
  const proof = {};
  const runtime = definePlugin({
    id: "test/runtime",
    setup: () => ({
      host: {
        status: async () => {
          reads++;
          return { services: [], calls: [], limits: {} };
        },
      } as unknown as RuntimeHost,
      clients: undefined,
    }),
  });
  const declaration = createRuntimeManage({
    id: "test/status",
    runtime,
    authorize(context: unknown) {
      if (context !== proof) throw new Error("Unauthorized.");
    },
  });
  const app = await startApp(defineApp({ plugins: [runtime, declaration.plugin] }));
  let context: unknown;
  const adapter = createManageAdapter({
    running: app,
    plugins: [declaration.plugin],
    operations: [declaration.status],
    canList: () => true,
    binding: () => ({ context: { evidence: context } }),
  });
  try {
    await expect(adapter.invoke("test/status", "status", {})).rejects.toThrow();
    expect(reads).toBe(0);
    context = proof;
    await expect(adapter.invoke("test/status", "status", { principal: "admin" })).rejects.toThrow();
    expect(reads).toBe(0);
    expect(await adapter.invoke("test/status", "status", {})).toEqual({
      services: [],
      calls: [],
      limits: {},
    });
    expect(reads).toBe(1);
  } finally {
    await app.stop();
  }
});
