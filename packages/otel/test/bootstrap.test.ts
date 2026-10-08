import { afterEach, expect, test } from "bun:test";
import { context, createContextKey, metrics, trace } from "@opentelemetry/api";
import { AsyncLocalStorageContextManager } from "@opentelemetry/context-async-hooks";
import { InMemorySpanExporter, SimpleSpanProcessor } from "@opentelemetry/sdk-trace-base";
import { NodeTracerProvider } from "@opentelemetry/sdk-trace-node";
import { InMemoryMetricExporter, AggregationTemporality } from "@opentelemetry/sdk-metrics";
import { bootstrapTelemetry } from "../src/bun";
import { createORPCInstrumentation } from "../src/orpc";

afterEach(() => {
  trace.disable();
  metrics.disable();
  context.disable();
});
const metricExporter = () => new InMemoryMetricExporter(AggregationTemporality.CUMULATIVE);

test("rejecting an external context manager never disables it, including caller-supplied same instance", async () => {
  const manager = new AsyncLocalStorageContextManager().enable();
  context.setGlobalContextManager(manager);
  const key = createContextKey("owner");
  await context.with(context.active().setValue(key, "preserved"), async () => {
    await expect(
      bootstrapTelemetry({
        contextManager: manager,
        traceExporter: new InMemorySpanExporter(),
        metricExporter: metricExporter(),
      }),
    ).rejects.toThrow("external context manager");
    await Bun.sleep(1);
    expect(context.active().getValue(key)).toBe("preserved");
  });
  manager.disable();
});

test("Bun ALS isolates concurrent spans, owned duplicate init and idempotent shutdown", async () => {
  const exporter = new InMemorySpanExporter();
  const telemetry = await bootstrapTelemetry({
    traceExporter: exporter,
    metricExporter: metricExporter(),
  });
  try {
    await expect(bootstrapTelemetry()).rejects.toThrow("already initialized");
    const seen = await Promise.all(
      ["first", "second"].map((name) =>
        trace.getTracer("test").startActiveSpan(name, async (span) => {
          await Bun.sleep(name === "first" ? 10 : 1);
          const active = trace.getActiveSpan();
          span.end();
          return active?.spanContext().spanId === span.spanContext().spanId;
        }),
      ),
    );
    expect(seen).toEqual([true, true]);
    expect(trace.getActiveSpan()).toBeUndefined();
    await telemetry.forceFlush();
    expect(
      exporter
        .getFinishedSpans()
        .map((span) => span.name)
        .sort(),
    ).toEqual(["first", "second"]);
  } finally {
    const first = telemetry.shutdown();
    expect(telemetry.shutdown()).toBe(first);
    await first;
  }
});

test("external mode preserves the owner's provider and does not flush or close it", async () => {
  const exporter = new InMemorySpanExporter();
  const provider = new NodeTracerProvider({ spanProcessors: [new SimpleSpanProcessor(exporter)] });
  trace.setGlobalTracerProvider(provider);
  const current = trace.getTracerProvider();
  try {
    await expect(
      bootstrapTelemetry({
        traceExporter: new InMemorySpanExporter(),
        metricExporter: metricExporter(),
      }),
    ).rejects.toThrow("external tracer provider");
    const telemetry = await bootstrapTelemetry({ mode: "external" });
    await telemetry.forceFlush();
    await telemetry.shutdown();
    expect(trace.getTracerProvider()).toBe(current);
    const span = trace.getTracer("external").startSpan("still alive");
    span.end();
    await provider.forceFlush();
    expect(exporter.getFinishedSpans()).toHaveLength(1);
  } finally {
    await provider.shutdown();
  }
});

test("flush deadline bounds a hung exporter; shutdown retains ownership until completion", async () => {
  const release = Promise.withResolvers<void>();
  const telemetry = await bootstrapTelemetry({
    timeoutMs: 20,
    takeOwnership: true,
    traceExporter: {
      export() {},
      shutdown: () => release.promise,
    },
    metricExporter: metricExporter(),
  });
  trace.getTracer("test").startSpan("pending").end();
  await expect(telemetry.forceFlush()).rejects.toBeDefined();
  await expect(telemetry.shutdown()).rejects.toThrow("deadline");
  await expect(bootstrapTelemetry()).rejects.toThrow("already initialized");
  release.resolve();
  await Bun.sleep(30);
});

test("supplied exporters and context manager remain borrowed by default", async () => {
  const manager = new AsyncLocalStorageContextManager();
  let traceClosed = 0;
  let metricClosed = 0;
  let contextDisabled = 0;
  const originalDisable = manager.disable.bind(manager);
  manager.disable = () => {
    contextDisabled++;
    return originalDisable();
  };
  const traces = new InMemorySpanExporter();
  const measurements = metricExporter();
  const traceShutdown = traces.shutdown.bind(traces);
  const metricShutdown = measurements.shutdown.bind(measurements);
  traces.shutdown = async () => {
    traceClosed++;
    await traceShutdown();
  };
  measurements.shutdown = async () => {
    metricClosed++;
    await metricShutdown();
  };
  const telemetry = await bootstrapTelemetry({
    contextManager: manager,
    traceExporter: traces,
    metricExporter: measurements,
  });
  await telemetry.shutdown();
  expect([traceClosed, metricClosed, contextDisabled]).toEqual([0, 0, 0]);
  await traces.shutdown();
  await measurements.shutdown();
  manager.disable();
});

test("oRPC owner guard rejects a Workers tracer and competing instrumentation", () => {
  const key = Symbol.for("@lenso/otel/orpc-tracer-owner");
  const host = globalThis as unknown as Record<symbol, unknown>;
  host[key] = { kind: "workers", owner: {} };
  expect(() => createORPCInstrumentation()).toThrow("already active");
  delete host[key];
  const first = createORPCInstrumentation();
  expect(() => createORPCInstrumentation()).toThrow("already active");
  first.disable();
  const next = createORPCInstrumentation({ propagationEnabled: false });
  expect(next.getConfig().propagationEnabled).toBe(false);
  next.disable();
});

test("export failure is observable and bootstrap works after owned shutdown", async () => {
  const telemetry = await bootstrapTelemetry({
    traceExporter: {
      export(_spans, done) {
        done({ code: 1, error: new Error("export failed") });
      },
      async shutdown() {},
    },
    metricExporter: metricExporter(),
  });
  trace.getTracer("test").startSpan("failure").end();
  await expect(telemetry.forceFlush()).rejects.toBeDefined();
  await telemetry.shutdown();
  const next = await bootstrapTelemetry({
    traceExporter: new InMemorySpanExporter(),
    metricExporter: metricExporter(),
  });
  await next.shutdown();
});

test("duplicate guard is shared across separately imported module copies", async () => {
  const telemetry = await bootstrapTelemetry({
    traceExporter: new InMemorySpanExporter(),
    metricExporter: metricExporter(),
  });
  try {
    const path = "../src/bun.ts?duplicate-copy";
    const copy = await import(path);
    await expect(copy.bootstrapTelemetry()).rejects.toThrow("already initialized");
  } finally {
    await telemetry.shutdown();
  }
});
