import { expect, test } from "bun:test";
import { trace, SpanStatusCode } from "@opentelemetry/api";
import { InMemorySpanExporter } from "@opentelemetry/sdk-trace-base";
import { InMemoryMetricExporter, AggregationTemporality } from "@opentelemetry/sdk-metrics";
import { definePlugin, lifecycleFailure, startApp, type Logger } from "../../lenso/src/index";
import { createTaskQueue, defineTask } from "../../tasks/src/index";
import type {
  ClaimedJob,
  ExecutionResult,
  ProviderJob,
  TaskProvider,
} from "../../tasks/src/contracts";
import { producerLinks, producerMetadata, traceMetadata } from "../../tasks/src/telemetry";
import { bootstrapTelemetry } from "../src/bun";
import { executeOperation } from "../../engine/src/operations";

test("explicit operation spans isolate concurrent calls and metrics have only bounded outcome labels", async () => {
  const exporter = new InMemorySpanExporter();
  const metricExporter = new InMemoryMetricExporter(AggregationTemporality.CUMULATIVE);
  const telemetry = await bootstrapTelemetry({ traceExporter: exporter, metricExporter });
  const original = new Error("secret business failure");
  const plugin = definePlugin({
    id: "operations",
    setup() {
      return {
        async execute(input: { fail: boolean }) {
          const active = trace.getActiveSpan()!.spanContext().spanId;
          await Bun.sleep(1);
          expect(trace.getActiveSpan()!.spanContext().spanId).toBe(active);
          if (input.fail) throw original;
          return "safe";
        },
      };
    },
  });
  const app = await startApp({ plugins: [plugin], instanceId: "operation-instance" });
  try {
    const operation = {
      plugin,
      method: "execute",
      description: "test",
      input: {
        "~standard": {
          version: 1 as const,
          vendor: "test",
          validate: (value: unknown) => ({ value }),
        },
      },
    };
    const results = await Promise.allSettled([
      executeOperation(app, operation, { fail: false, password: "do-not-export" }),
      executeOperation(app, operation, { fail: true, password: "do-not-export" }),
    ]);
    expect(results[0]).toMatchObject({ status: "fulfilled", value: "safe" });
    expect(results[1]).toMatchObject({ status: "rejected", reason: original });
    await telemetry.forceFlush();
    const spans = exporter.getFinishedSpans().filter((span) => span.name === "lenso.operation");
    expect(spans).toHaveLength(2);
    expect(
      spans.every((span) => span.attributes["lenso.instance.id"] === "operation-instance"),
    ).toBe(true);
    expect(spans.every((span) => span.attributes["lenso.plugin.id"] === "operations")).toBe(true);
    expect(
      JSON.stringify(
        spans.map((span) => ({
          attributes: span.attributes,
          status: span.status,
          events: span.events,
        })),
      ),
    ).not.toContain("secret");
    const measurements = metricExporter
      .getMetrics()
      .flatMap((resource) => resource.scopeMetrics.flatMap((scope) => scope.metrics))
      .filter((metric) => metric.descriptor.name.startsWith("lenso.operation."));
    expect(measurements.map((metric) => metric.descriptor.name)).toContain(
      "lenso.operation.errors",
    );
    for (const measurement of measurements)
      for (const point of measurement.dataPoints)
        expect(Object.keys(point.attributes).every((key) => key === "outcome")).toBe(true);
  } finally {
    await app.stop();
    await telemetry.shutdown();
  }
});

test("lifecycle spans/logs preserve LIFO, primitive/object failure identity and exclude secrets", async () => {
  const exporter = new InMemorySpanExporter();
  const telemetry = await bootstrapTelemetry({
    traceExporter: exporter,
    metricExporter: new InMemoryMetricExporter(AggregationTemporality.CUMULATIVE),
  });
  const logs: unknown[] = [];
  const logger: Logger = {
    child() {
      return this;
    },
    debug(fields) {
      logs.push(fields);
    },
    info() {},
    warn() {},
    error(fields) {
      logs.push(fields);
    },
  };
  const original = new Error("Bearer secret error payload");
  const order: number[] = [];
  const plugin = definePlugin({
    id: "resource",
    setup(context) {
      expect(context.instanceId).toBe("chosen-instance");
      context.onCleanup(() => {
        order.push(1);
        throw original;
      });
      context.onCleanup(() => {
        order.push(2);
        throw "private primitive";
      });
      throw original;
    },
  });
  try {
    await expect(
      startApp({ plugins: [plugin], logger, instanceId: "chosen-instance" }),
    ).rejects.toMatchObject({
      errors: [original, "private primitive", original],
    });
    expect(order).toEqual([2, 1]);
    expect(lifecycleFailure(original)).toMatchObject({ phase: "cleanup", pluginId: "resource" });
    await telemetry.forceFlush();
    const spans = exporter.getFinishedSpans();
    expect(spans.map((span) => span.name)).toEqual([
      "lenso.plugin.setup",
      "lenso.plugin.cleanup",
      "lenso.plugin.cleanup",
    ]);
    expect(spans.every((span) => span.status.code === SpanStatusCode.ERROR)).toBe(true);
    expect(
      JSON.stringify(
        spans.map((span) => ({
          attributes: span.attributes,
          status: span.status,
          events: span.events,
        })),
      ),
    ).not.toContain("secret");
    expect(JSON.stringify(logs)).not.toContain("private");
  } finally {
    await telemetry.shutdown();
  }
});

test("durable retries are fresh roots linked to original producer, never unrelated worker request", async () => {
  const exporter = new InMemorySpanExporter();
  const telemetry = await bootstrapTelemetry({
    traceExporter: exporter,
    metricExporter: new InMemoryMetricExporter(AggregationTemporality.CUMULATIVE),
  });
  let stored: ProviderJob | undefined;
  let execute: ((job: ClaimedJob) => Promise<ExecutionResult>) | undefined;
  const provider: TaskProvider = {
    async enqueue(job) {
      stored = structuredClone(job);
      return crypto.randomUUID();
    },
    async get() {
      return null;
    },
    async cancel() {
      return "missing";
    },
    async retry() {
      return false;
    },
    async startWorker(run) {
      execute = run;
      return { done: Promise.resolve(), async stop() {} };
    },
    async close() {},
  };
  const active: string[] = [];
  const task = defineTask({
    name: "retry-fixture",
    input: {
      "~standard": {
        version: 1 as const,
        vendor: "test",
        validate: (value: unknown) => ({ value }),
      },
    },
    async handler(_input, context) {
      await Bun.sleep(1);
      active.push(trace.getActiveSpan()!.spanContext().spanId);
      if (context.attempt === 1) throw new Error("private payload");
    },
  });
  const queue = createTaskQueue({ provider, tasks: [task] });
  try {
    await queue.enqueue(task, { credential: "do-not-export" });
    expect(stored?.traceMetadata?.traceparent).toBeDefined();
    await queue.startWorker();
    await trace.getTracer("test").startActiveSpan("unrelated", async (span) => {
      await execute!({
        ...stored!,
        jobId: crypto.randomUUID(),
        attempt: 1,
        signal: new AbortController().signal,
      });
      await execute!({
        ...stored!,
        jobId: crypto.randomUUID(),
        attempt: 2,
        signal: new AbortController().signal,
      });
      span.end();
    });
    await telemetry.forceFlush();
    const spans = exporter.getFinishedSpans();
    const producer = spans.find((span) => span.name === "lenso.task.enqueue")!;
    const attempts = spans.filter((span) => span.name === "lenso.task.attempt");
    expect(attempts).toHaveLength(2);
    expect(new Set(active).size).toBe(2);
    for (const attempt of attempts) {
      expect(attempt.parentSpanContext).toBeUndefined();
      expect(attempt.links[0]?.context.spanId).toBe(producer.spanContext().spanId);
      expect(attempt.spanContext().traceId).not.toBe(producer.spanContext().traceId);
    }
    expect(
      JSON.stringify(
        spans.map((span) => ({
          attributes: span.attributes,
          events: span.events,
          status: span.status,
        })),
      ),
    ).not.toContain("private");
  } finally {
    await queue.close();
    await telemetry.shutdown();
  }
});

test("metadata is a bounded allowlist and never baggage or authentication", () => {
  const parent = "00-11111111111111111111111111111111-2222222222222222-01";
  expect(
    traceMetadata({
      traceparent: parent,
      tracestate: "vendor=ok",
      baggage: "authorization=secret",
      actor: "admin",
    }),
  ).toEqual({ traceparent: parent, tracestate: "vendor=ok" });
  expect(traceMetadata({ traceparent: parent, tracestate: "x".repeat(513) })).toEqual({
    traceparent: parent,
  });
  expect(
    traceMetadata({ traceparent: parent, tracestate: "vendor=ok\r\nAuthorization: secret" }),
  ).toEqual({ traceparent: parent });
  expect(
    traceMetadata({ traceparent: "00-" + "0".repeat(32) + "-2222222222222222-01" }),
  ).toBeUndefined();
});

test("durable W3C metadata is independent of a custom host HTTP propagator", async () => {
  const telemetry = await bootstrapTelemetry({
    traceExporter: new InMemorySpanExporter(),
    metricExporter: new InMemoryMetricExporter(AggregationTemporality.CUMULATIVE),
    propagator: {
      inject() {
        throw new Error("Durable messages must not use the HTTP carrier format");
      },
      extract(context) {
        return context;
      },
      fields: () => ["b3"],
    },
  });
  try {
    await trace.getTracer("test").startActiveSpan("producer", (span) => {
      const metadata = producerMetadata();
      expect(metadata?.traceparent).toBeDefined();
      expect(producerLinks(metadata)[0]?.context.spanId).toBe(span.spanContext().spanId);
      span.end();
    });
  } finally {
    await telemetry.shutdown();
  }
});
