import { expect, test } from "bun:test";
import { trace } from "@opentelemetry/api";
import { InMemorySpanExporter } from "@opentelemetry/sdk-trace-base";
import { InMemoryMetricExporter, AggregationTemporality } from "@opentelemetry/sdk-metrics";
import { startApp } from "../../lenso/src/index";
import {
  invokeValidatedOperation,
  type OperationExecutionEvidence,
} from "../../engine/src/operations";
import { createMcpAdapter } from "../../mcp/src/adapter";
import app, { operations } from "../../../examples/greeting/lenso.config";
import { bootstrapTelemetry } from "../src/bun";

const request = () => ({
  identity: "developer",
  requestId: crypto.randomUUID(),
  signal: new AbortController().signal,
});

test("execution observation is optional, immutable and cannot change no-op telemetry calls", async () => {
  trace.disable();
  const running = await startApp(app);
  const observed: OperationExecutionEvidence[] = [];
  try {
    for (const observer of [
      (value: OperationExecutionEvidence) => {
        observed.push(value);
        throw new Error("observer failure");
      },
      async (value: OperationExecutionEvidence) => {
        observed.push(value);
        throw new Error("async observer failure");
      },
    ]) {
      expect(
        await invokeValidatedOperation(
          running,
          operations[0]!,
          { name: "Ada" },
          {
            onExecutionEvidence: observer,
          },
        ),
      ).toMatchObject({ message: "Hello, Ada!" });
    }
    expect(observed).toHaveLength(2);
    for (const evidence of observed) {
      expect(evidence.trace).toBeNull();
      expect(Object.isFrozen(evidence)).toBe(true);
      expect(Object.isFrozen(evidence.operation)).toBe(true);
    }
    const adapter = await createMcpAdapter({
      running,
      plugins: app.plugins,
      operations,
      canList: () => true,
      authorize: () => true,
      binding: () => ({}),
    });
    try {
      const tool = (await adapter.listTools(request())).tools[0]!;
      const result = await adapter.callTool(tool.name, { name: "Ada" }, request());
      const { _meta: metadata } = result;
      expect(metadata?.["lenso/execution"]).toMatchObject({
        instanceId: running.instanceId,
        operation: { pluginId: "greeting", method: "greet" },
        trace: null,
      });
      const previous = process.env.LENSO_EXECUTION_EVIDENCE_SECRET;
      process.env.LENSO_EXECUTION_EVIDENCE_SECRET = running.instanceId;
      try {
        const redacted = await adapter.callTool(tool.name, { name: "Ada" }, request());
        const { _meta: safeMetadata } = redacted;
        expect(JSON.stringify(redacted)).not.toContain(running.instanceId);
        expect(safeMetadata?.["lenso/execution"]).toMatchObject({
          instanceId: "[REDACTED]",
          trace: null,
        });
      } finally {
        if (previous === undefined) delete process.env.LENSO_EXECUTION_EVIDENCE_SECRET;
        else process.env.LENSO_EXECUTION_EVIDENCE_SECRET = previous;
      }
    } finally {
      await adapter.close();
    }
  } finally {
    await running.stop();
  }
});

test("Greeting MCP evidence matches real operation spans for concurrent success and safe failure", async () => {
  const exporter = new InMemorySpanExporter();
  const telemetry = await bootstrapTelemetry({
    traceExporter: exporter,
    metricExporter: new InMemoryMetricExporter(AggregationTemporality.CUMULATIVE),
  });
  const running = await startApp(app);
  let authorized = true;
  const options = {
    running,
    plugins: app.plugins,
    operations,
    canList: () => true,
    authorize: () => authorized,
    binding: () => ({
      onExecutionEvidence: () => {
        throw new Error("private observer detail");
      },
    }),
  };
  const adapter = await createMcpAdapter(options);
  const operation = operations[0]!;
  const failing = await createMcpAdapter({
    ...options,
    operations: [
      {
        ...operation,
        output: {
          "~standard": {
            ...operation.output!["~standard"],
            validate: () => ({ issues: [{ message: "private output detail" }] }),
          },
        },
      },
    ],
  });
  const bounded = await createMcpAdapter({
    ...options,
    maxOutputBytes: 256,
    running: { instanceId: "x".repeat(1000), get: (plugin) => running.get(plugin) },
  });
  try {
    const tool = (await adapter.listTools(request())).tools[0]!;
    const results = await Promise.all([
      adapter.callTool(tool.name, { name: "Ada" }, request()),
      adapter.callTool(tool.name, { name: "Lin" }, request()),
      failing.callTool(tool.name, { name: "Ada" }, request()),
    ]);
    expect(results[0]!.structuredContent).toMatchObject({ message: "Hello, Ada!" });
    expect(results[1]!.structuredContent).toMatchObject({ message: "Hello, Lin!" });
    expect(results[2]!.isError).toBe(true);
    expect(results[2]!.structuredContent).toBeUndefined();
    expect(JSON.stringify(results)).not.toContain("private");
    const invalid = await adapter.callTool(tool.name, { name: "x" }, request());
    authorized = false;
    const denied = await adapter.callTool(tool.name, { name: "Ada" }, request());
    for (const refused of [invalid, denied]) {
      const { _meta: metadata } = refused;
      expect(metadata?.["lenso/execution"]).toBeUndefined();
    }
    await telemetry.forceFlush();
    const spans = exporter.getFinishedSpans().filter((span) => span.name === "lenso.operation");
    expect(spans).toHaveLength(3);
    const returnedIds = new Set<string>();
    for (const result of results) {
      const { _meta: metadata } = result;
      const evidence = metadata?.["lenso/execution"] as OperationExecutionEvidence;
      const actual = spans.find((span) => span.spanContext().spanId === evidence.trace?.spanId);
      expect(actual).toBeDefined();
      expect(evidence.trace).toEqual({
        traceId: actual!.spanContext().traceId,
        spanId: actual!.spanContext().spanId,
      });
      expect(actual!.attributes["lenso.instance.id"]).toBe(evidence.instanceId);
      expect(actual!.attributes["lenso.plugin.id"]).toBe(evidence.operation.pluginId);
      expect(actual!.attributes["lenso.operation"]).toBe(evidence.operation.method);
      expect(Object.keys(evidence.trace!).sort()).toEqual(["spanId", "traceId"]);
      returnedIds.add(evidence.trace!.spanId);
    }
    expect(returnedIds.size).toBe(3);
    authorized = true;
    const oversizedMetadata = await bounded.callTool(tool.name, { name: "Ada" }, request());
    const { _meta: boundedMetadata } = oversizedMetadata;
    expect(oversizedMetadata.isError).not.toBe(true);
    expect(oversizedMetadata.structuredContent).toMatchObject({ message: "Hello, Ada!" });
    expect(boundedMetadata?.["lenso/execution"]).toBeUndefined();
  } finally {
    await Promise.all([adapter.close(), failing.close(), bounded.close()]);
    await running.stop();
    await telemetry.shutdown();
  }
});
