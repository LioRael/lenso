import { expect, test } from "bun:test";
import { definePlugin, startApp } from "@lenso/core";
import { defineOperation, type Operation } from "@lenso/engine/operations";
import { EngineError } from "@lenso/engine/diagnostics";
import { call, ORPCError, type RouterClient } from "@orpc/server";
import { RPCHandler } from "@orpc/server/fetch";
import { createORPCClient } from "@orpc/client";
import { RPCLink } from "@orpc/client/fetch";
import { z } from "zod";
import { createManageAdapter } from "../src";
import { createManageRouter } from "../src/orpc";

function httpInvocation<E, O extends Operation>(
  router: ReturnType<typeof createManageRouter<E, O>>,
  input: { pluginId: string; method: string; input: unknown },
) {
  const handler = new RPCHandler(router);
  let status = 0;
  const client = createORPCClient<RouterClient<typeof router>>(
    new RPCLink({
      origin: "https://manage.test",
      url: "/rpc",
      async fetch(url, init) {
        const request = new Request(url, init);
        const { response } = await handler.handle(request, {
          prefix: "/rpc",
          context: { request },
        });
        status = response!.status;
        return response!;
      },
    }),
  );
  return { result: client.invoke(input), status: () => status };
}

test("Manage classifies an actual preaborted binding signal without dispatch", async () => {
  let calls = 0;
  const plugin = definePlugin({
    id: "manage-cancel",
    setup: () => ({
      run: (_input: unknown) => {
        calls++;
        return null;
      },
    }),
  });
  const operation = defineOperation({
    plugin,
    method: "run",
    input: z.unknown(),
    description: "Test request cancellation.",
  });
  const running = await startApp({ plugins: [plugin] });
  const reason = new Error("private cancellation reason");
  const controller = new AbortController();
  controller.abort(reason);
  const options = {
    running,
    plugins: [plugin],
    operations: [operation],
    binding: () => ({ signal: controller.signal }),
    canList: () => true,
  };
  try {
    const error = await createManageAdapter(options)
      .invoke(plugin.id, "run", null)
      .catch((e) => e);
    expect(error).toBeInstanceOf(EngineError);
    if (!(error instanceof EngineError)) throw error;
    expect(error.diagnostic.code).toBe("aborted");
    expect(error.cause).toBe(reason);
    const router = createManageRouter({ ...options, evidence: () => ({ evidence: null }) });
    const protocol = await call(
      router.invoke,
      { pluginId: plugin.id, method: "run", input: null },
      { context: { request: new Request("http://local/manage") } },
    ).catch((e) => e);
    expect(protocol).toBeInstanceOf(ORPCError);
    if (!(protocol instanceof ORPCError)) throw protocol;
    expect(protocol.code).toBe("CLIENT_CLOSED_REQUEST");
    expect(JSON.stringify(protocol.data)).not.toContain(reason.message);
    const transport = httpInvocation(router, { pluginId: plugin.id, method: "run", input: null });
    const wireError = await transport.result.catch((cause: unknown) => cause);
    expect(wireError).toBeInstanceOf(ORPCError);
    if (!(wireError instanceof ORPCError)) throw wireError;
    expect(wireError.code).toBe("CLIENT_CLOSED_REQUEST");
    expect(transport.status()).toBe(499);
    expect(calls).toBe(0);
  } finally {
    await running.stop();
  }
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((complete, fail) => {
    resolve = complete;
    reject = fail;
  });
  return { promise, resolve, reject };
}

test("Manage abort during asynchronous binding waits for binding and prevents dispatch", async () => {
  let calls = 0;
  const plugin = definePlugin({
    id: "manage-binding-cancel",
    setup: () => ({
      run: (_input: unknown) => {
        calls++;
        return null;
      },
    }),
  });
  const operation = defineOperation({
    plugin,
    method: "run",
    input: z.unknown(),
    description: "Test asynchronous binding cancellation.",
  });
  const running = await startApp({ plugins: [plugin] });
  const controller = new AbortController();
  const entered = deferred<void>();
  const binding = deferred<void>();
  const reason = { private: "binding cancellation reason" };
  const router = createManageRouter({
    running,
    plugins: [plugin],
    operations: [operation],
    evidence: () => ({ evidence: null }),
    canList: () => true,
    async binding() {
      entered.resolve();
      await binding.promise;
      return { signal: controller.signal };
    },
  });
  let settled = false;
  const transport = httpInvocation(router, { pluginId: plugin.id, method: "run", input: null });
  const pending = transport.result.then(
    () => {
      settled = true;
      throw new Error("Cancelled binding unexpectedly dispatched.");
    },
    (error: unknown) => {
      settled = true;
      return error;
    },
  );
  try {
    await entered.promise;
    controller.abort(reason);
    await Promise.resolve();
    expect(settled).toBe(false);
    expect(calls).toBe(0);
    binding.resolve();
    const error = await pending;
    expect(error).toBeInstanceOf(ORPCError);
    if (!(error instanceof ORPCError)) throw error;
    expect(error.code).toBe("CLIENT_CLOSED_REQUEST");
    expect(transport.status()).toBe(499);
    expect(calls).toBe(0);
  } finally {
    binding.resolve();
    await pending;
    await running.stop();
  }
});

for (const outcome of ["success", "abort-reason", "business-failure"] as const) {
  test(`Manage execution cancellation awaits settlement and preserves ${outcome}`, async () => {
    const entered = deferred<void>();
    const execution = deferred<null>();
    const controller = new AbortController();
    const reason = new Error("private abort reason");
    const failure = new Error("private business failure");
    let calls = 0;
    const plugin = definePlugin({
      id: `manage-executing-${outcome}`,
      setup: () => ({
        async run(_input: unknown) {
          calls++;
          entered.resolve();
          return await execution.promise;
        },
      }),
    });
    const operation = defineOperation({
      plugin,
      method: "run",
      input: z.unknown(),
      description: "Test execution cancellation ownership.",
    });
    const running = await startApp({ plugins: [plugin] });
    const router = createManageRouter({
      running,
      plugins: [plugin],
      operations: [operation],
      evidence: () => ({ evidence: null }),
      canList: () => true,
      binding: () => ({ signal: controller.signal }),
    });
    let settled = false;
    const transport = httpInvocation(router, { pluginId: plugin.id, method: "run", input: null });
    const pending = transport.result.then(
      () => {
        settled = true;
        throw new Error("Cancelled execution unexpectedly returned success.");
      },
      (error: unknown) => {
        settled = true;
        return error;
      },
    );
    try {
      await entered.promise;
      controller.abort(reason);
      await Promise.resolve();
      expect(settled).toBe(false);
      expect(calls).toBe(1);
      if (outcome === "success") execution.resolve(null);
      else execution.reject(outcome === "abort-reason" ? reason : failure);
      const error = await pending;
      expect(error).toBeInstanceOf(ORPCError);
      if (!(error instanceof ORPCError)) throw error;
      expect(error.code).toBe(
        outcome === "business-failure" ? "MANAGE_FAILED" : "CLIENT_CLOSED_REQUEST",
      );
      expect(transport.status()).toBe(outcome === "business-failure" ? 500 : 499);
      expect(JSON.stringify(error.data)).not.toContain("private");
    } finally {
      execution.resolve(null);
      await pending;
      await running.stop();
    }
  });
}
