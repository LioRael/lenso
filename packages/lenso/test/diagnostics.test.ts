import { expect, test } from "bun:test";
import { DiagnosticError, validatePlugins } from "../src/diagnostics";
import { lifecycleFailure, startApp } from "../src/lifecycle";

test("assembly diagnostics include declaring sources and missing dependency IDs", () => {
  const consumer = {
    id: "consumer",
    source: { file: "consumer.ts", export: "consumer" },
    requires: [{ id: "dependency", source: { file: "dependency.ts" }, setup() {} }],
    setup() {},
  };
  const first = { id: "same", source: { file: "first.ts" }, setup() {} };
  const second = { id: "same", source: { file: "second.ts" }, setup() {} };
  try {
    validatePlugins([consumer, first, second]);
    throw new Error("Expected invalid assembly");
  } catch (error) {
    expect(error).toBeInstanceOf(DiagnosticError);
    const diagnostics = (error as DiagnosticError).diagnostics;
    expect(diagnostics[0]).toMatchObject({
      code: "missing-dependency",
      pluginId: "consumer",
      dependencyId: "dependency",
      source: consumer.source,
    });
    expect(diagnostics.find((item) => item.code === "duplicate-id")).toMatchObject({
      source: second.source,
      details: { declaringSources: [first.source, second.source] },
    });
  }
});

test("invalid source metadata fails assembly before setup", async () => {
  let started = false;
  const plugin = {
    id: "invalid",
    source: { file: " ", line: -1 },
    setup() {
      started = true;
    },
  };
  expect(() => validatePlugins([plugin])).toThrow(DiagnosticError);
  await expect(startApp({ plugins: [plugin] })).rejects.toBeInstanceOf(DiagnosticError);
  expect(started).toBe(false);
});

test("lifecycle attribution preserves the declared source and original errors", async () => {
  const source = { file: "src/resource.ts", export: "resource", line: 12 };
  const setupFailure = new Error("setup");
  const cleanupFailure = new Error("cleanup");
  const plugin = {
    id: "resource",
    source,
    setup({ onCleanup }: import("../src/plugin").PluginContext) {
      onCleanup(() => {
        throw cleanupFailure;
      });
      throw setupFailure;
    },
  };
  const error = await startApp({ plugins: [plugin] }).catch((error: unknown) => error);
  expect((error as AggregateError).errors).toEqual([setupFailure, cleanupFailure]);
  expect(lifecycleFailure(error)).toEqual({ phase: "setup", pluginId: "resource", source });
  expect(lifecycleFailure(setupFailure)).toEqual({ phase: "setup", pluginId: "resource", source });
  expect(lifecycleFailure(cleanupFailure)).toEqual({
    phase: "cleanup",
    pluginId: "resource",
    source,
  });
});
