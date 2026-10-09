import { expect, test } from "bun:test";
import { definePlugin, startApp } from "@lenso/core";
import { defineOperation } from "@lenso/engine/operations";
import { z } from "zod";
import { createMcpAdapter } from "../src/adapter";

const input = z.object({});
const request = (signal = new AbortController().signal) => ({
  identity: "test",
  requestId: "cancellation-test",
  signal,
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function resultCode(
  result: Awaited<ReturnType<Awaited<ReturnType<typeof createMcpAdapter>>["callTool"]>>,
) {
  const content = result.content[0];
  if (!content || content.type !== "text") throw new Error("Expected MCP text result");
  return JSON.parse(content.text).code as string;
}

test("MCP binding signal cancellation is mapped by Engine, including async binding waits", async () => {
  for (const asynchronous of [false, true]) {
    const controller = new AbortController();
    const entered = deferred<void>();
    const finish = deferred<void>();
    let calls = 0;
    let cleanup = 0;
    const plugin = definePlugin({
      id: "mcp-binding",
      setup(lifecycle) {
        lifecycle.onCleanup(() => {
          cleanup++;
        });
        return {
          run(_input: unknown) {
            calls++;
            return true;
          },
        };
      },
    });
    const operation = defineOperation({ plugin, method: "run", input, description: "Run" });
    const running = await startApp({ plugins: [plugin] });
    const adapter = await createMcpAdapter({
      running,
      plugins: [plugin],
      operations: [operation],
      canList: () => true,
      authorize: () => true,
      binding: async () => {
        entered.resolve();
        if (asynchronous) await finish.promise;
        return { signal: controller.signal };
      },
    });
    try {
      if (!asynchronous) controller.abort({ message: "private-reason" });
      const call = adapter.callTool("operation_0", {}, request());
      await entered.promise;
      if (asynchronous) controller.abort("private-reason");
      finish.resolve();
      const result = await call;
      expect(resultCode(result)).toBe("aborted");
      expect(JSON.stringify(result)).not.toContain("private-reason");
      expect(calls).toBe(0);
      expect(cleanup).toBe(0);
    } finally {
      finish.resolve();
      await adapter.close();
      await running.stop();
    }
    expect(cleanup).toBe(1);
  }
});

test("MCP binding-only executing abort preserves real business failures and rejects spoofed cancellation", async () => {
  for (const outcome of ["success", "reason", "business", "spoofed"] as const) {
    const controller = new AbortController();
    const entered = deferred<void>();
    const finish = deferred<void>();
    const plugin = definePlugin({
      id: "mcp-executing",
      setup: () => ({
        async run(_input: unknown) {
          entered.resolve();
          await finish.promise;
          if (outcome === "reason") controller.signal.throwIfAborted();
          if (outcome === "business") throw new Error("private-business");
          if (outcome === "spoofed") throw { name: "AbortError", code: "aborted" };
          return true;
        },
      }),
    });
    const operation = defineOperation({ plugin, method: "run", input, description: "Run" });
    const running = await startApp({ plugins: [plugin] });
    const adapter = await createMcpAdapter({
      running,
      plugins: [plugin],
      operations: [operation],
      canList: () => true,
      authorize: () => true,
      binding: () => ({ signal: controller.signal }),
      maxConcurrentCalls: 1,
    });
    try {
      let settled = false;
      const call = adapter.callTool("operation_0", {}, request());
      void call.then(() => {
        settled = true;
      });
      await entered.promise;
      controller.abort({ message: "private-reason" });
      await Bun.sleep(0);
      expect(settled).toBe(false);
      expect(resultCode(await adapter.callTool("operation_0", {}, request()))).toBe("adapter-busy");
      finish.resolve();
      const result = await call;
      expect(resultCode(result)).toBe(
        outcome === "success" || outcome === "reason" ? "aborted" : "invocation-failed",
      );
      expect(JSON.stringify(result)).not.toContain("private-");
    } finally {
      finish.resolve();
      await adapter.close();
      await running.stop();
    }
  }
});

test("MCP request cancellation returns early but retains the slot and drains actual settlement", async () => {
  const controller = new AbortController();
  const entered = deferred<void>();
  const finish = deferred<void>();
  const business = new Error("private-business");
  let observedFailure: unknown;
  let cleanup = 0;
  const plugin = definePlugin({
    id: "mcp-drain",
    setup(lifecycle) {
      lifecycle.onCleanup(() => {
        cleanup++;
      });
      return {
        async run(_input: unknown) {
          entered.resolve();
          await finish.promise;
          throw business;
        },
      };
    },
  });
  const operation = defineOperation({
    plugin,
    method: "run",
    input,
    description: "Run",
    mapError(error) {
      observedFailure = error;
      return undefined;
    },
  });
  const running = await startApp({ plugins: [plugin] });
  const adapter = await createMcpAdapter({
    running,
    plugins: [plugin],
    operations: [operation],
    canList: () => true,
    authorize: () => true,
    binding: () => ({}),
    maxConcurrentCalls: 1,
  });
  try {
    const call = adapter.callTool("operation_0", {}, request(controller.signal));
    await entered.promise;
    controller.abort("private-reason");
    expect(resultCode(await call)).toBe("request-cancelled");
    expect(resultCode(await adapter.callTool("operation_0", {}, request()))).toBe("adapter-busy");
    let drained = false;
    const close = adapter.close();
    expect(adapter.close()).toBe(close);
    void close.then(() => {
      drained = true;
    });
    await Bun.sleep(0);
    expect(drained).toBe(false);
    expect(cleanup).toBe(0);
    finish.resolve();
    await close;
    expect(observedFailure).toBe(business);
    expect(cleanup).toBe(0);
  } finally {
    finish.resolve();
    await adapter.close();
    await running.stop();
  }
  expect(cleanup).toBe(1);
});
