import { expect, test } from "bun:test";
import { definePlugin, startApp } from "@lenso/core";
import { defineOperation, type Operation } from "@lenso/engine/operations";
import { z } from "zod";
import { createMcpAdapter } from "../src/adapter";
import type { McpAdapterOptions, McpRequestContext } from "../src/adapter";

const input = z.strictObject({ value: z.string() });
type Identity = { user: string };

function request(signal = new AbortController().signal): McpRequestContext<Identity> {
  return { identity: { user: "test" }, requestId: crypto.randomUUID(), signal };
}

async function harness(
  setup: (calls: { read: number; write: number }) => {
    plugin: import("@lenso/core").Plugin<unknown>;
    operations: Operation[];
  } = (calls) => {
    const plugin = makePlugin(calls);
    return { plugin, operations: makeOperations(plugin) };
  },
  policies: Partial<McpAdapterOptions<Identity, Operation>> = {},
  limits: Partial<McpAdapterOptions<Identity, Operation>> = {},
) {
  const calls = { read: 0, write: 0 };
  const { plugin, operations } = setup(calls);
  const running = await startApp({ plugins: [plugin] });
  const adapter = await createMcpAdapter({
    running,
    plugins: [plugin],
    operations,
    canList: policies.canList ?? (() => true),
    authorize: policies.authorize ?? (() => true),
    binding: policies.binding ?? (() => ({})),
    ...limits,
  });
  return { calls, plugin, operations, running, adapter };
}

function makePlugin(calls: { read: number; write: number }) {
  return definePlugin({
    id: "adapter-test",
    setup: () => ({
      read({ value }: z.infer<typeof input>) {
        calls.read++;
        return { value };
      },
      write({ value }: z.infer<typeof input>) {
        calls.write++;
        return { value };
      },
    }),
  });
}

function makeOperations(plugin: ReturnType<typeof makePlugin>) {
  return [
    defineOperation({ plugin, method: "read", input, description: "Read", effect: "read" }),
    defineOperation({ plugin, method: "write", input, description: "Write", effect: "write" }),
  ];
}

function text(result: { content: { type: string; text?: string }[] }) {
  return result.content[0]?.text ?? "";
}

test("starts the app once and invokes real read and write services", async () => {
  const { calls, adapter, running } = await harness();
  try {
    const listed = await adapter.listTools(request());
    expect(listed.tools).toHaveLength(2);
    expect(await adapter.callTool("operation_0", { value: "a" }, request())).toMatchObject({
      content: [{ text: '{"value":"a"}' }],
    });
    expect(await adapter.callTool("operation_1", { value: "b" }, request())).toMatchObject({
      content: [{ text: '{"value":"b"}' }],
    });
    expect(calls).toEqual({ read: 1, write: 1 });
  } finally {
    await adapter.close();
    await running.stop();
  }
});

test("validates schema input and keeps listing and invocation policy separate", async () => {
  const denied = await harness(undefined, { canList: () => false });
  try {
    expect((await denied.adapter.listTools(request())).tools).toHaveLength(0);
    expect(text(await denied.adapter.callTool("operation_0", { value: "x" }, request()))).toContain(
      '"code":"forbidden-operation"',
    );
  } finally {
    await denied.adapter.close();
    await denied.running.stop();
  }

  const hidden = await harness(undefined, { authorize: () => false });
  try {
    expect((await hidden.adapter.listTools(request())).tools).toHaveLength(2);
    expect(text(await hidden.adapter.callTool("operation_0", { value: "x" }, request()))).toContain(
      '"code":"forbidden-operation"',
    );
    expect(hidden.calls.read).toBe(0);
  } finally {
    await hidden.adapter.close();
    await hidden.running.stop();
  }
});

test("rechecks visibility and authorization after discovery", async () => {
  let permitted = true;
  const { adapter, running, calls } = await harness(undefined, {
    canList: () => permitted,
    authorize: () => permitted,
  });
  try {
    expect((await adapter.listTools(request())).tools).toHaveLength(2);
    permitted = false;
    expect(text(await adapter.callTool("operation_0", { value: "x" }, request()))).toContain(
      '"code":"forbidden-operation"',
    );
    expect(calls.read).toBe(0);
  } finally {
    await adapter.close();
    await running.stop();
  }
});

test("rejects non-object and runtime-only schemas without taking app ownership", async () => {
  const plugin = definePlugin({
    id: "schema-test",
    setup: () => ({ run: (_value: unknown) => 1 }),
  });
  const running = await startApp({ plugins: [plugin] });
  const operation = defineOperation({
    plugin,
    method: "run",
    input: {
      "~standard": {
        version: 1 as const,
        vendor: "test",
        validate: (value: unknown) => ({ value }),
      },
    },
    description: "Runtime only",
  });
  await expect(
    createMcpAdapter({
      running,
      plugins: [plugin],
      operations: [operation],
      canList: () => true,
      authorize: () => true,
      binding: () => ({}),
    }),
  ).rejects.toThrow();
  expect(running.status()).toEqual([{ id: "schema-test", state: "ready" }]);
  await running.stop();
});

test("bounds input and output and redacts arbitrary service errors", async () => {
  const { adapter, running, calls } = await harness(
    (counts) => {
      const plugin = definePlugin({
        id: "limits",
        setup: () => ({
          run({ value }: z.infer<typeof input>) {
            counts.read++;
            if (value === "secret-error") throw new Error("private arbitrary failure");
            return { value: "x".repeat(400) };
          },
        }),
      });
      const operation = defineOperation({ plugin, method: "run", input, description: "Run" });
      return { plugin, operations: [operation] };
    },
    {},
    { maxInputBytes: 64, maxOutputBytes: 256 },
  );
  try {
    expect(text(await adapter.callTool("operation_0", { value: 123 }, request()))).toContain(
      '"code":"invalid-input"',
    );
    const oversized = text(
      await adapter.callTool("operation_0", { value: "x".repeat(100) }, request()),
    );
    expect(oversized).toContain('"code":"input-too-large"');
    const output = text(
      await adapter.callTool("operation_0", { value: "secret-error" }, request()),
    );
    expect(output).toContain('"code":"invocation-failed"');
    expect(output).not.toContain("private arbitrary failure");
    expect(calls.read).toBe(1);
    expect(text(await adapter.callTool("operation_0", { value: "normal" }, request()))).toContain(
      '"code":"output-too-large"',
    );
  } finally {
    await adapter.close();
    await running.stop();
  }
});

test("startup refuses non-object schemas and excessive metadata, app remains owned by host", async () => {
  const plugin = definePlugin({
    id: "catalog-budget",
    setup: () => ({ run: async (_input: unknown) => 1 }),
  });
  const app = await startApp({ plugins: [plugin] });
  const options = {
    running: app,
    plugins: [plugin],
    canList: () => true,
    authorize: () => true,
    binding: () => ({}),
  };
  try {
    await expect(
      createMcpAdapter({
        ...options,
        operations: [
          defineOperation({
            plugin,
            method: "run",
            input: z.string(),
            description: "Scalar input.",
          }),
        ],
      }),
    ).rejects.toThrow();
    await expect(
      createMcpAdapter({
        ...options,
        maxCatalogBytes: 256,
        operations: [
          defineOperation({
            plugin,
            method: "run",
            input: z.object({}),
            description: "x".repeat(2048),
          }),
        ],
      }),
    ).rejects.toThrow();
    expect(await app.get(plugin).run({})).toBe(1);
  } finally {
    await app.stop();
  }
});

test("passes identity, request ID and cooperative signal to the binding context", async () => {
  let observed: unknown;
  const { adapter, running } = await harness(undefined, {
    binding: (_operation, _input, context) => {
      observed = context;
      return { context: { evidence: context.identity.user, requestId: context.requestId } };
    },
  });
  try {
    const controller = new AbortController();
    const supplied = request(controller.signal);
    const result = await adapter.callTool("operation_0", { value: "x" }, supplied);
    expect(result.isError).not.toBe(true);
    expect(observed).toMatchObject({ identity: supplied.identity, requestId: supplied.requestId });
    controller.abort();
    expect(observed).toMatchObject({ signal: { aborted: true } });
  } finally {
    await adapter.close();
    await running.stop();
  }
});

test("timeout keeps the execution slot until noncooperative work settles", async () => {
  let finish!: () => void;
  const plugin = definePlugin({
    id: "timeout-test",
    setup: () => ({
      run: async (_value: z.infer<typeof input>) => {
        await new Promise<void>((resolve) => {
          finish = resolve;
        });
        return { done: true };
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
    binding: () => ({}),
    maxConcurrentCalls: 1,
    requestTimeoutMs: 10,
  });
  try {
    const slow = adapter.callTool("operation_0", { value: "x" }, request());
    while (!finish) await Bun.sleep(1);
    expect(text(await slow)).toContain('"code":"request-timeout"');
    expect(text(await adapter.callTool("operation_0", { value: "x" }, request()))).toContain(
      '"code":"adapter-busy"',
    );
    finish();
    await Bun.sleep(0);
    expect(text(await adapter.callTool("operation_0", { value: "x" }, request()))).toContain(
      '"code":"request-timeout"',
    );
    finish();
  } finally {
    await adapter.close();
    await running.stop();
  }
});

test("close aborts and drains admitted work and returns the same promise", async () => {
  let seenSignal: AbortSignal | undefined;
  let finish!: () => void;
  const { adapter, running } = await harness(
    (calls) => {
      const service = definePlugin({
        id: "close-test",
        setup: () => ({
          async run(_value: z.infer<typeof input>) {
            calls.read++;
            await new Promise<void>((resolve) => {
              finish = resolve;
            });
            return { done: true };
          },
        }),
      });
      return {
        plugin: service,
        operations: [
          defineOperation({ plugin: service, method: "run", input, description: "Run" }),
        ],
      };
    },
    {
      binding: (_operation, _input, context) => {
        seenSignal = context.signal;
        return {};
      },
    },
    { requestTimeoutMs: 5_000 },
  );
  try {
    const call = adapter.callTool("operation_0", { value: "x" }, request());
    while (!seenSignal) await Bun.sleep(1);
    const first = adapter.close();
    const second = adapter.close();
    expect(first).toBe(second);
    expect(seenSignal?.aborted).toBe(true);
    finish();
    await first;
    await call;
  } finally {
    await adapter.close();
    await running.stop();
  }
});
