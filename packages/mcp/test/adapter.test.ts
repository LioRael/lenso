import { expect, spyOn, test } from "bun:test";
import { definePlugin, startApp } from "@lenso/core";
import { defineOperation, type Operation } from "@lenso/engine/operations";
import { z } from "zod";
import { createMcpAdapter, operationToolName } from "../src/adapter";
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

test("tool names survive catalog reordering/redaction and distinguish exact identity tuples", async () => {
  const f = await harness();
  const reordered = await (async () => {
    const previous = process.env.MCP_TOOL_NAME_TEST_SECRET;
    process.env.MCP_TOOL_NAME_TEST_SECRET = f.plugin.id;
    try {
      return await createMcpAdapter({
        running: f.running,
        plugins: [f.plugin],
        operations: [...f.operations].reverse(),
        canList: () => true,
        authorize: () => true,
        binding: () => ({}),
      });
    } finally {
      if (previous === undefined) delete process.env.MCP_TOOL_NAME_TEST_SECRET;
      else process.env.MCP_TOOL_NAME_TEST_SECRET = previous;
    }
  })();
  try {
    const first = (await f.adapter.listTools(request())).tools;
    const second = (await reordered.listTools(request())).tools;
    expect(second.map((tool) => tool.name)).toEqual(first.map((tool) => tool.name).reverse());
    expect(await reordered.callTool(first[0]!.name, { value: "same" }, request())).toMatchObject({
      structuredContent: { value: "same" },
    });
    const names = [
      operationToolName("a.b", "c"),
      operationToolName("a", "b.c"),
      operationToolName("a-b", "c"),
      operationToolName("a_b", "c"),
      operationToolName("長".repeat(1000), "é/".repeat(1000)),
    ];
    expect(new Set(names).size).toBe(names.length);
    for (const name of [...names, ...first.map((tool) => tool.name)])
      expect(name).toMatch(/^[A-Za-z0-9_.-]{1,128}$/);
    await expect(f.adapter.callTool("operation_0", {}, request())).rejects.toThrow("Unknown tool");
  } finally {
    await reordered.close();
    await f.adapter.close();
    await f.running.stop();
  }
});

test("pagination filters first and binds forward cursors to identity, adapter and fresh visibility", async () => {
  let visible = new Set(["read", "write"]);
  let checks = 0;
  const f = await harness(
    undefined,
    {
      canList: (operation) => {
        checks++;
        return visible.has(operation.method);
      },
    },
    { listPageSize: 1, identityKey: (identity) => identity.user },
  );
  const other = await harness(
    undefined,
    {},
    { listPageSize: 1, identityKey: (identity) => identity.user },
  );
  try {
    const first = await f.adapter.listTools(request());
    expect(first.tools.map((tool) => tool.title)).toEqual(["adapter-test.read"]);
    expect(first.nextCursor).toBeTruthy();
    await expect(
      f.adapter.listTools({ ...request(), identity: { user: "other" } }, first.nextCursor),
    ).rejects.toThrow("Invalid or stale");
    await expect(other.adapter.listTools(request(), first.nextCursor)).rejects.toThrow(
      "Invalid or stale",
    );
    const second = await f.adapter.listTools(request(), first.nextCursor);
    expect(second.tools.map((tool) => tool.title)).toEqual(["adapter-test.write"]);
    expect(second.nextCursor).toBeUndefined();
    expect(checks).toBe(6);
    await expect(f.adapter.listTools(request(), first.nextCursor)).rejects.toThrow(
      "Invalid or stale",
    );
    const expiring = await f.adapter.listTools(request());
    const clock = spyOn(Date, "now").mockReturnValue(Date.now() + 5 * 60_000 + 1);
    try {
      await expect(f.adapter.listTools(request(), expiring.nextCursor)).rejects.toThrow(
        "Invalid or stale",
      );
    } finally {
      clock.mockRestore();
    }
    const stale = await f.adapter.listTools(request());
    visible = new Set(["read"]);
    await expect(f.adapter.listTools(request(), stale.nextCursor)).rejects.toThrow(
      "Invalid or stale",
    );
    expect(
      text(await f.adapter.callTool(second.tools[0]!.name, { value: "denied" }, request())),
    ).toContain("forbidden-operation");
    visible = new Set(["write"]);
    const filtered = await f.adapter.listTools(request());
    expect(filtered.tools.map((tool) => tool.title)).toEqual(["adapter-test.write"]);
    expect(filtered.nextCursor).toBeUndefined();
  } finally {
    await Promise.all([f.adapter.close(), other.adapter.close()]);
    await Promise.all([f.running.stop(), other.running.stop()]);
  }
});

test("a cursor can be consumed by only one simultaneous listing", async () => {
  const f = await harness(undefined, {}, { listPageSize: 1 });
  try {
    const context = request();
    const first = await f.adapter.listTools(context);
    const results = await Promise.allSettled([
      f.adapter.listTools(context, first.nextCursor),
      f.adapter.listTools(context, first.nextCursor),
    ]);
    expect(results.filter((result) => result.status === "fulfilled")).toHaveLength(1);
    const rejected = results.filter((result) => result.status === "rejected");
    expect(rejected).toHaveLength(1);
    const error = rejected[0]!.reason as { code: number; message: string };
    expect(error.code).toBe(-32602);
    expect(error.message).toContain("Invalid or stale tool cursor.");
  } finally {
    await f.adapter.close();
    await f.running.stop();
  }
});

test("cursor capacity rejects issuance without eviction and consuming releases a slot", async () => {
  const f = await harness(undefined, {}, { listPageSize: 1 });
  try {
    const context = request();
    const retained = await f.adapter.listTools(context);
    for (let i = 1; i < 1024; i++) {
      expect((await f.adapter.listTools(context)).nextCursor).toBeTruthy();
    }
    await expect(f.adapter.listTools(context)).rejects.toThrow("Tool cursor capacity reached.");
    expect(
      (await f.adapter.listTools(context, retained.nextCursor)).tools.map((tool) => tool.title),
    ).toEqual(["adapter-test.write"]);
    expect((await f.adapter.listTools(context)).nextCursor).toBeTruthy();
  } finally {
    await f.adapter.close();
    await f.running.stop();
  }
});

test("structured content uses bounded redacted finite output and wraps non-objects", async () => {
  const outputs: Record<string, unknown> = {
    object: { password: "private", value: "safe" },
    array: [1, "two"],
    null: null,
    boolean: true,
    string: "hello",
    wrapperLimit: "x".repeat(244),
  };
  const f = await harness(
    () => {
      const plugin = definePlugin({
        id: "structured",
        setup: () => ({ run: ({ value }: z.infer<typeof input>) => outputs[value] }),
      });
      return {
        plugin,
        operations: [defineOperation({ plugin, method: "run", input, description: "Output" })],
      };
    },
    {},
    { maxOutputBytes: 256 },
  );
  try {
    const name = (await f.adapter.listTools(request())).tools[0]!.name;
    for (const value of ["object", "array", "null", "boolean", "string"]) {
      const result = await f.adapter.callTool(name, { value }, request());
      expect(result.isError).not.toBe(true);
      const finite: unknown = JSON.parse(text(result));
      expect(result.structuredContent).toEqual(
        value === "object" ? (finite as Record<string, unknown>) : { result: finite },
      );
      expect(JSON.stringify(result)).not.toContain("private");
    }
    const oversized = await f.adapter.callTool(name, { value: "wrapperLimit" }, request());
    expect(text(oversized)).toContain("output-too-large");
    expect(oversized.structuredContent).toEqual(JSON.parse(text(oversized)));
  } finally {
    await f.adapter.close();
    await f.running.stop();
  }
});

test.each([false, true])(
  "structured failures locate invalid fields and permit corrected calls (output contract: %s)",
  async (declaredOutput) => {
    const f = await harness((calls) => {
      const plugin = makePlugin(calls);
      return {
        plugin,
        operations: makeOperations(plugin).map((operation) => ({
          ...operation,
          ...(declaredOutput ? { output: z.object({ value: z.string() }) } : {}),
        })),
      };
    });
    try {
      const tool = (await f.adapter.listTools(request())).tools[0]!;
      const invalid = await f.adapter.callTool(tool.name, { value: 123 }, request());
      const { _meta: failureMetadata } = invalid;
      expect(invalid.isError).toBe(true);
      expect(failureMetadata?.["lenso/diagnostic"]).toMatchObject({
        code: "invalid-input",
        phase: "input",
        operation: "adapter-test.read",
        details: { paths: [["value"]] },
        effectState: "not-started",
      });
      expect(failureMetadata?.["lenso/diagnostic"]).toEqual(JSON.parse(text(invalid)));
      if (declaredOutput) expect(invalid.structuredContent).toBeUndefined();
      else expect(invalid.structuredContent).toEqual(JSON.parse(text(invalid)));
      expect(f.calls.read).toBe(0);
      const corrected = await f.adapter.callTool(tool.name, { value: "fixed" }, request());
      expect(corrected.isError).not.toBe(true);
      expect(corrected.structuredContent).toEqual({ value: "fixed" });
      expect(f.calls.read).toBe(1);
    } finally {
      await f.adapter.close();
      await f.running.stop();
    }
  },
);

test("starts the app once and invokes real read and write services", async () => {
  const { calls, adapter, running } = await harness();
  try {
    const listed = await adapter.listTools(request());
    expect(listed.tools).toHaveLength(2);
    expect(await adapter.callTool(listed.tools[0]!.name, { value: "a" }, request())).toMatchObject({
      content: [{ text: '{"value":"a"}' }],
      structuredContent: { value: "a" },
    });
    expect(await adapter.callTool(listed.tools[1]!.name, { value: "b" }, request())).toMatchObject({
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
    expect(
      text(
        await denied.adapter.callTool(
          operationToolName("adapter-test", "read"),
          { value: "x" },
          request(),
        ),
      ),
    ).toContain('"code":"forbidden-operation"');
  } finally {
    await denied.adapter.close();
    await denied.running.stop();
  }

  const hidden = await harness(undefined, { authorize: () => false });
  try {
    expect((await hidden.adapter.listTools(request())).tools).toHaveLength(2);
    expect(
      text(
        await hidden.adapter.callTool(
          operationToolName("adapter-test", "read"),
          { value: "x" },
          request(),
        ),
      ),
    ).toContain('"code":"forbidden-operation"');
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
    expect(
      text(
        await adapter.callTool(
          operationToolName("adapter-test", "read"),
          { value: "x" },
          request(),
        ),
      ),
    ).toContain('"code":"forbidden-operation"');
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
    expect(
      text(await adapter.callTool(operationToolName("limits", "run"), { value: 123 }, request())),
    ).toContain('"code":"invalid-input"');
    const oversized = text(
      await adapter.callTool(
        operationToolName("limits", "run"),
        { value: "x".repeat(100) },
        request(),
      ),
    );
    expect(oversized).toContain('"code":"input-too-large"');
    const output = text(
      await adapter.callTool(
        operationToolName("limits", "run"),
        { value: "secret-error" },
        request(),
      ),
    );
    expect(output).toContain('"code":"invocation-failed"');
    expect(output).not.toContain("private arbitrary failure");
    expect(calls.read).toBe(1);
    expect(
      text(
        await adapter.callTool(operationToolName("limits", "run"), { value: "normal" }, request()),
      ),
    ).toContain('"code":"output-too-large"');
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
    const result = await adapter.callTool(
      operationToolName("adapter-test", "read"),
      { value: "x" },
      supplied,
    );
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
    const slow = adapter.callTool(
      operationToolName("timeout-test", "run"),
      { value: "x" },
      request(),
    );
    while (!finish) await Bun.sleep(1);
    expect(text(await slow)).toContain('"code":"request-timeout"');
    expect(
      text(
        await adapter.callTool(operationToolName("timeout-test", "run"), { value: "x" }, request()),
      ),
    ).toContain('"code":"adapter-busy"');
    finish();
    await Bun.sleep(0);
    expect(
      text(
        await adapter.callTool(operationToolName("timeout-test", "run"), { value: "x" }, request()),
      ),
    ).toContain('"code":"request-timeout"');
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
    const call = adapter.callTool(
      operationToolName("close-test", "run"),
      { value: "x" },
      request(),
    );
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
