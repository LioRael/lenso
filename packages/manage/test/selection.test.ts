import { expect, test } from "bun:test";
import { definePlugin, startApp } from "@lenso/core";
import { defineOperation, type Operation } from "@lenso/engine/operations";
import { call } from "@orpc/server";
import { z } from "zod";
import { createAgentTools, createManageAdapter, createManageSelection } from "../src";
import { createManageRouter } from "../src/orpc";

test("100-operation selection reads declaration metadata once, not per adapter invocation", async () => {
  let descriptions = 0;
  const plugin = definePlugin({
    id: "count",
    setup: () =>
      Object.fromEntries(Array.from({ length: 100 }, (_, index) => [`m${index}`, () => index])),
  });
  const operations: Operation[] = Array.from({ length: 100 }, (_, index) => ({
    plugin,
    method: `m${index}`,
    get description() {
      descriptions++;
      return "Count declaration validation.";
    },
    input: {
      "~standard": { version: 1, vendor: "count", validate: (value) => ({ value }) },
    },
  }));
  const running = await startApp({ plugins: [plugin] });
  const options = {
    running,
    plugins: [plugin],
    operations,
    binding: () => ({}),
    canList: () => true,
  };
  try {
    const adapter = createManageAdapter(options);
    const preparation = descriptions;
    descriptions = 0;
    expect(await adapter.invoke("count", "m0", null)).toBe(0);
    const invocation = descriptions;
    console.log(`adapter declaration reads: prepare=${preparation}, invoke=${invocation}`);
    expect(preparation).toBe(100);
    expect(invocation).toBe(0);
  } finally {
    await running.stop();
  }
});

test("100-operation router prepares once and reuses selection across requests", async () => {
  let descriptions = 0;
  const plugin = definePlugin({
    id: "router-count",
    setup: () =>
      Object.fromEntries(Array.from({ length: 100 }, (_, index) => [`m${index}`, () => index])),
  });
  const operations: Operation[] = Array.from({ length: 100 }, (_, index) => ({
    plugin,
    method: `m${index}`,
    get description() {
      descriptions++;
      return "Count declaration validation.";
    },
    input: {
      "~standard": { version: 1, vendor: "count", validate: (value) => ({ value }) },
    },
  }));
  const running = await startApp({ plugins: [plugin] });
  try {
    const router = createManageRouter({
      running,
      plugins: [plugin],
      operations,
      evidence: () => ({ evidence: null }),
      binding: () => ({}),
      canList: () => true,
    });
    const preparation = descriptions;
    descriptions = 0;
    for (let request = 0; request < 2; request++)
      expect(
        await call(
          router.invoke,
          { pluginId: plugin.id, method: "m0", input: null },
          { context: { request: new Request("http://local/manage") } },
        ),
      ).toBe(0);
    const requests = descriptions;
    console.log(`router declaration reads: prepare=${preparation}, two requests=${requests}`);
    expect(preparation).toBe(100);
    expect(requests).toBe(0);
  } finally {
    await running.stop();
  }
});

test("selection snapshots declarations and source, preserves executable identities, and replaces keys", async () => {
  let reads = 0;
  let hidden = 0;
  const plugin = definePlugin({
    id: "snapshot",
    setup: () => ({
      read: (_input: unknown) => ++reads,
      hidden: (_input: unknown) => ++hidden,
    }),
  });
  const schema = z.object({});
  const source = { file: "original.ts" };
  const operation = {
    plugin,
    method: "read",
    input: schema,
    description: "Original.",
    source,
  };
  const plugins = [plugin];
  const operations = [operation];
  const running = await startApp({ plugins });
  const selection = createManageSelection({ running, plugins, operations });
  const policy = {
    canList: () => true,
    binding: (snapshot: Operation) => {
      expect(Object.isFrozen(snapshot)).toBe(true);
      expect(snapshot.plugin).toBe(plugin);
      expect(snapshot.input).toBe(schema);
      return {};
    },
  };
  try {
    const adapter = createManageAdapter({ selection, ...policy });
    const catalog = await adapter.catalog();
    const key = catalog[0]!.key;
    operation.method = "hidden";
    operation.description = "Edited.";
    source.file = "edited.ts";
    plugins.length = 0;
    operations.length = 0;
    expect(await adapter.invokeEntry(key, {})).toBe(1);
    expect((await adapter.catalog())[0]).toMatchObject({
      method: "read",
      description: "Original.",
      source: { file: "original.ts" },
      key,
    });
    await expect(adapter.invoke(plugin.id, "hidden", {})).rejects.toThrow();
    const replacement = createManageSelection({
      running,
      plugins: [plugin],
      operations: [operation],
    });
    try {
      const next = createManageAdapter({ selection: replacement, ...policy });
      const nextKey = (await next.catalog())[0]!.key;
      expect(nextKey).not.toBe(key);
      await expect(next.invokeEntry(key, {})).rejects.toThrow();
      selection.close();
      selection.close();
      await expect(adapter.invokeEntry(key, {})).rejects.toThrow("closed");
      await expect(adapter.catalog()).rejects.toThrow("closed");
      expect(() => selection.createAdapter(policy)).toThrow("closed");
      expect(await next.invokeEntry(nextKey, {})).toBe(1);
      expect(reads).toBe(1);
      expect(hidden).toBe(1);
    } finally {
      replacement.close();
    }
  } finally {
    selection.close();
    await running.stop();
  }
});

test("retained agent handles cannot execute a closed selection and close does not stop the runtime", async () => {
  let calls = 0;
  let cleanups = 0;
  const plugin = definePlugin({
    id: "retained",
    setup: ({ onCleanup }) => {
      onCleanup(() => {
        cleanups++;
      });
      return { read: (_input: unknown) => ++calls };
    },
  });
  const operation = defineOperation({
    plugin,
    method: "read",
    input: z.object({}),
    description: "Read",
  });
  const running = await startApp({ plugins: [plugin] });
  const selection = createManageSelection({ running, plugins: [plugin], operations: [operation] });
  try {
    const adapter = selection.createAdapter({ canList: () => true, binding: () => ({}) });
    const tools = await createAgentTools(adapter);
    selection.close();
    await expect(tools[0]!.invoke({})).rejects.toThrow("closed");
    expect(calls).toBe(0);
    expect(cleanups).toBe(0);
    expect(running.get(plugin).read({})).toBe(1);
  } finally {
    selection.close();
    await running.stop();
  }
  expect(cleanups).toBe(1);
});

for (const boundary of [
  "canList",
  "input",
  "binding",
  "confirm",
  "approve",
  "final-canList",
] as const)
  test(`close during asynchronous ${boundary} prevents dispatch`, async () => {
    let calls = 0;
    let visibility = 0;
    const waiting = Promise.withResolvers<void>();
    const resume = Promise.withResolvers<void>();
    async function pause() {
      waiting.resolve();
      await resume.promise;
    }
    const plugin = definePlugin({
      id: `close-${boundary}`,
      setup: () => ({ run: (_input: unknown) => ++calls }),
    });
    const operation = defineOperation({
      plugin,
      method: "run",
      description: "Close while admission is pending.",
      input: {
        "~standard": {
          version: 1,
          vendor: "close",
          async validate(value: unknown) {
            if (boundary === "input") await pause();
            return { value };
          },
        },
      },
      confirmation: "required",
      approval: "required",
    });
    const running = await startApp({ plugins: [plugin] });
    const selection = createManageSelection({
      running,
      plugins: [plugin],
      operations: [operation],
    });
    const adapter = selection.createAdapter({
      async canList() {
        visibility++;
        if (boundary === "canList" || (boundary === "final-canList" && visibility === 2))
          await pause();
        return true;
      },
      async binding() {
        if (boundary === "binding") await pause();
        return {
          async confirm() {
            if (boundary === "confirm") await pause();
            return true;
          },
          async approve() {
            if (boundary === "approve") await pause();
            return true;
          },
        };
      },
    });
    try {
      const pending = adapter.invoke(plugin.id, "run", null);
      await waiting.promise;
      selection.close();
      resume.resolve();
      await expect(pending).rejects.toThrow("closed");
      expect(calls).toBe(0);
    } finally {
      resume.resolve();
      selection.close();
      await running.stop();
    }
  });

test("final visibility and target validity are rechecked without rebinding or revalidating input", async () => {
  let calls = 0;
  let validations = 0;
  let bindings = 0;
  let allowed = true;
  const plugin = definePlugin({
    id: "final-validity",
    setup: () => ({ run: (_input: unknown) => ++calls }),
  });
  const operation = defineOperation({
    plugin,
    method: "run",
    input: {
      "~standard": {
        version: 1,
        vendor: "validity",
        validate(value: unknown) {
          validations++;
          return { value };
        },
      },
    },
    description: "Validate the selected target after waits.",
    confirmation: "required",
  });
  const running = await startApp({ plugins: [plugin] });
  const selection = createManageSelection({ running, plugins: [plugin], operations: [operation] });
  const waiting = Promise.withResolvers<void>();
  const resume = Promise.withResolvers<boolean>();
  const adapter = selection.createAdapter({
    canList: () => allowed,
    binding: () => {
      bindings++;
      return {
        confirm() {
          waiting.resolve();
          return resume.promise;
        },
      };
    },
  });
  try {
    const pending = adapter.invoke(plugin.id, "run", null);
    await waiting.promise;
    allowed = false;
    resume.resolve(true);
    await expect(pending).rejects.toThrow("not available");
    expect(validations).toBe(1);
    expect(bindings).toBe(1);
    expect(calls).toBe(0);
    allowed = true;
    const invalidTarget = selection.createAdapter({
      canList: () => true,
      binding: () => ({
        confirm() {
          Object.assign(plugin, { id: "retargeted" });
          return true;
        },
      }),
    });
    await expect(invalidTarget.invoke("final-validity", "run", null)).rejects.toThrow(
      "no longer valid",
    );
    expect(calls).toBe(0);
  } finally {
    Object.assign(plugin, { id: "final-validity" });
    selection.close();
    await running.stop();
  }
});

test("router reuses a selection but isolates fresh request evidence and closes all entry handles", async () => {
  const observed: string[] = [];
  let extracted = 0;
  const plugin = definePlugin({
    id: "request-evidence",
    setup: () => ({
      read: async (_input: unknown, context: { actor: string }) => {
        await Promise.resolve();
        observed.push(context.actor);
        return context.actor;
      },
    }),
  });
  const operation = defineOperation({
    plugin,
    method: "read",
    input: z.unknown(),
    description: "Use current request evidence.",
    context: true,
  });
  const running = await startApp({ plugins: [plugin] });
  const selection = createManageSelection({ running, plugins: [plugin], operations: [operation] });
  const router = createManageRouter({
    selection,
    evidence({ request }) {
      extracted++;
      return { evidence: request.headers.get("x-test-actor")! };
    },
    canList: (_operation, evidence) => evidence.evidence !== "hidden",
    binding: async (_operation, _input, evidence) => {
      await Promise.resolve();
      return { context: { actor: evidence.evidence } };
    },
  });
  function invoke(actor: string) {
    return call(
      router.invoke,
      { pluginId: plugin.id, method: "read", input: null },
      {
        context: {
          request: new Request("http://local/manage", { headers: { "x-test-actor": actor } }),
        },
      },
    );
  }
  try {
    expect(await Promise.all([invoke("alice"), invoke("bob")])).toEqual(["alice", "bob"]);
    await expect(invoke("hidden")).rejects.toMatchObject({ code: "NOT_FOUND" });
    expect(extracted).toBe(3);
    expect(observed).toEqual(["alice", "bob"]);
    selection.close();
    await expect(invoke("alice")).rejects.toMatchObject({ code: "SERVICE_UNAVAILABLE" });
    expect(observed).toEqual(["alice", "bob"]);
  } finally {
    selection.close();
    await running.stop();
  }
});

test("closing after dispatch does not interrupt settlement or stop borrowed services", async () => {
  const entered = Promise.withResolvers<void>();
  const result = Promise.withResolvers<number>();
  const plugin = definePlugin({
    id: "settlement",
    setup: () => ({
      async read(_input: unknown) {
        entered.resolve();
        return await result.promise;
      },
    }),
  });
  const operation = defineOperation({
    plugin,
    method: "read",
    input: z.unknown(),
    description: "Read",
  });
  const running = await startApp({ plugins: [plugin] });
  const selection = createManageSelection({ running, plugins: [plugin], operations: [operation] });
  try {
    const adapter = selection.createAdapter({ binding: () => ({}), canList: () => true });
    const pending = adapter.invoke(plugin.id, "read", null);
    await entered.promise;
    selection.close();
    result.resolve(7);
    expect(await pending).toBe(7);
    await expect(adapter.invoke(plugin.id, "read", null)).rejects.toThrow("closed");
  } finally {
    result.resolve(7);
    selection.close();
    await running.stop();
  }
});

test("a close queued after the final async visibility check still revokes synchronous dispatch", async () => {
  let calls = 0;
  let visibility = 0;
  const plugin = definePlugin({
    id: "dispatch-gap",
    setup: () => ({ run: (_input: unknown) => ++calls }),
  });
  const operation = defineOperation({
    plugin,
    method: "run",
    input: z.unknown(),
    description: "Run",
  });
  const running = await startApp({ plugins: [plugin] });
  const selection = createManageSelection({ running, plugins: [plugin], operations: [operation] });
  try {
    const adapter = selection.createAdapter({
      binding: () => ({}),
      canList() {
        if (++visibility === 2) queueMicrotask(() => queueMicrotask(() => selection.close()));
        return true;
      },
    });
    await expect(adapter.invoke(plugin.id, "run", null)).rejects.toThrow("closed");
    expect(calls).toBe(0);
  } finally {
    selection.close();
    await running.stop();
  }
});
