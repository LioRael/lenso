import { expect, test } from "bun:test";
import { definePlugin, startApp } from "@lenso/core";
import type { Operation } from "@lenso/engine/operations";
import { createMcpAdapter, type McpRequestContext } from "../src/adapter";

test("MCP discovery and invocation share prepared immutable declarations and stable tool names", async () => {
  let descriptions = 0;
  const plugin = definePlugin({
    id: "mcp-selection",
    setup: () =>
      Object.fromEntries(Array.from({ length: 100 }, (_, index) => [`m${index}`, () => index])),
  });
  const operations: Operation[] = Array.from({ length: 100 }, (_, index) => ({
    plugin,
    method: `m${index}`,
    get description() {
      descriptions++;
      return "Prepared MCP operation.";
    },
    input: {
      "~standard": {
        version: 1,
        vendor: "mcp-selection",
        validate: (value) => ({ value }),
        jsonSchema: { input: () => ({ type: "object" }), output: () => ({ type: "integer" }) },
      },
    },
  }));
  const running = await startApp({ plugins: [plugin] });
  const observed = new Map<string, Operation>();
  const request = (): McpRequestContext<null> => ({
    identity: null,
    requestId: crypto.randomUUID(),
    signal: new AbortController().signal,
  });
  const adapter = await createMcpAdapter({
    running,
    plugins: [plugin],
    operations,
    canList(operation) {
      const previous = observed.get(operation.method);
      if (previous) expect(operation).toBe(previous);
      else observed.set(operation.method, operation);
      expect(Object.isFrozen(operation)).toBe(true);
      return true;
    },
    authorize(operation) {
      expect(operation).toBe(observed.get(operation.method)!);
      return true;
    },
    binding(operation) {
      expect(operation).toBe(observed.get(operation.method)!);
      return {};
    },
  });
  try {
    expect(descriptions).toBe(100);
    descriptions = 0;
    const first = await adapter.listTools(request());
    expect(first.tools).toHaveLength(100);
    expect(first.tools[0]!.name).toBe("operation_0");
    expect(first.tools[99]!.name).toBe("operation_99");
    Object.assign(operations[0]!, { method: "hidden" });
    expect((await adapter.listTools(request())).tools).toHaveLength(100);
    expect(await adapter.callTool("operation_0", {}, request())).toMatchObject({
      content: [{ type: "text", text: "0" }],
    });
    expect(descriptions).toBe(0);
    await adapter.close();
    expect(await adapter.callTool("operation_0", {}, request())).toMatchObject({
      isError: true,
    });
  } finally {
    await adapter.close();
    await running.stop();
  }
});
