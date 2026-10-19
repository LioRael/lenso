import { expect, test } from "bun:test";
import { definePlugin, startApp } from "@lenso/core";
import { z } from "zod";
import { defineOperation } from "@lenso/engine/operations";
import { diagnostic } from "@lenso/engine/diagnostics";
import { invoke } from "@lenso/cli";
import { createMcpAdapter, operationToolName } from "../src/adapter";

function detail(result: { content: readonly { type: string }[] }) {
  const block = result.content[0];
  if (!block || !("text" in block) || typeof block.text !== "string")
    throw new Error("Expected diagnostic text.");
  return JSON.parse(block.text);
}

test("CLI and borrowed MCP project one output/error contract without changing tool identity", async () => {
  class Missing extends Error {}
  let mode = "ok";
  let calls = 0;
  const plugin = definePlugin({
    id: "output-contract",
    setup: () => ({
      run(_input: { value: string }) {
        calls++;
        if (mode === "error") throw new Missing("private-domain-detail");
        return mode === "invalid" ? { value: 42 } : { value: "safe" };
      },
    }),
  });
  const output = z.object({ value: z.string() });
  const operation = defineOperation({
    plugin,
    method: "run",
    input: z.object({ value: z.string() }),
    output,
    description: "Contract",
    source: { file: "src/service.ts", export: "service" },
    errors: [{ code: "record-missing", message: "Record not found." }],
    mapError: (error) =>
      error instanceof Missing
        ? { code: "record-missing", phase: "invoke", message: "Record not found." }
        : undefined,
  });
  const app = { plugins: [plugin], operations: [operation] };
  const running = await startApp(app);
  const adapter = await createMcpAdapter({
    running,
    plugins: app.plugins,
    operations: app.operations,
    canList: () => true,
    authorize: () => true,
    binding: () => ({}),
  });
  const request = () => ({
    identity: "fixture",
    requestId: crypto.randomUUID(),
    signal: new AbortController().signal,
  });
  const name = operationToolName(plugin.id, "run");
  try {
    const tools = await adapter.listTools(request());
    expect(tools.tools[0]).toMatchObject({
      name,
      outputSchema: { type: "object", properties: { value: { type: "string" } } },
      _meta: {
        "lenso/operation": { errors: operation.errors, outputSchemaAvailability: "available" },
      },
    });
    expect(await invoke(app, plugin.id, "run", { value: "input" })).toEqual({ value: "safe" });
    expect((await adapter.callTool(name, { value: "input" }, request())).structuredContent).toEqual(
      { value: "safe" },
    );
    mode = "invalid";
    const cli = await invoke(app, plugin.id, "run", { value: "input" }).catch((error) =>
      diagnostic(error),
    );
    expect(cli).toMatchObject({
      code: "invalid-output",
      phase: "output",
      pluginId: plugin.id,
      operation: `${plugin.id}.run`,
      source: operation.source,
      effectState: "unknown",
      details: { paths: [["value"]] },
    });
    const invalid = await adapter.callTool(name, { value: "input" }, request());
    expect(invalid.isError).toBe(true);
    expect(detail(invalid)).toMatchObject({
      code: "invalid-output",
      phase: "output",
      effectState: "unknown",
      details: { paths: [["value"]] },
    });
    mode = "error";
    const failure = await adapter.callTool(name, { value: "input" }, request());
    expect(detail(failure)).toMatchObject({
      code: "record-missing",
      operation: `${plugin.id}.run`,
      phase: "invoke",
      effectState: "unknown",
    });
    expect(JSON.stringify(failure)).not.toContain("private-domain-detail");
    const before = calls;
    const bad = await adapter.callTool(name, { value: 42 }, request());
    expect(detail(bad)).toMatchObject({
      code: "invalid-input",
      effectState: "not-started",
      details: { paths: [["value"]] },
    });
    expect(calls).toBe(before);
  } finally {
    await adapter.close();
    await running.stop();
  }
});
