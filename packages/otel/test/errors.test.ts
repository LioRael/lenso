import { expect, test } from "bun:test";
import { trace, SpanStatusCode } from "@opentelemetry/api";
import { InMemorySpanExporter, SimpleSpanProcessor } from "@opentelemetry/sdk-trace-base";
import { NodeTracerProvider } from "@opentelemetry/sdk-trace-node";
import { AggregationTemporality, InMemoryMetricExporter } from "@opentelemetry/sdk-metrics";
import { call, os } from "@orpc/server";
import { bootstrapTelemetry, createSafeSpanExporter } from "../src/bun";
import { createORPCInstrumentation } from "../src/orpc";

function exportedDetails(exporter: InMemorySpanExporter): string {
  return JSON.stringify(
    exporter.getFinishedSpans().map((span) => ({
      attributes: span.attributes,
      events: span.events,
      status: span.status,
      links: span.links,
    })),
  );
}

function captureFailure() {
  const span = trace.getTracer("test").startSpan("safe-operation");
  const error = new Error("private-message", { cause: new Error("private-cause") });
  error.stack = "private-stack private-cause";
  span.recordException(error);
  const unknownError = { message: "private-unknown", code: "private-code" };
  span.recordException(unknownError);
  span.setStatus({ code: SpanStatusCode.ERROR, message: "private-status" });
  span.setAttributes({
    "exception.message": "private-attribute",
    "error.code": "private-code",
    "lenso.error.code": "PUBLIC_FAILURE",
    "lenso.phase": "handler",
    "lenso.instance.id": "safe-instance",
  });
  span.addEvent("safe-event", { "exception.cause": "private-cause", "lenso.phase": "handler" });
  span.addLink({ context: span.spanContext(), attributes: { "error.message": "private-link" } });
  span.end();
  return span.spanContext();
}

test("owned bootstrap defaults to omission in actual exported finished spans", async () => {
  const exporter = new InMemorySpanExporter();
  const telemetry = await bootstrapTelemetry({
    traceExporter: exporter,
    metricExporter: new InMemoryMetricExporter(AggregationTemporality.CUMULATIVE),
  });
  try {
    const ids = captureFailure();
    await telemetry.forceFlush();
    const [span] = exporter.getFinishedSpans();
    expect(span!.spanContext()).toEqual(ids);
    expect(span!.status).toEqual({ code: SpanStatusCode.ERROR });
    expect(span!.attributes).toEqual({
      "lenso.error.code": "PUBLIC_FAILURE",
      "lenso.phase": "handler",
      "lenso.instance.id": "safe-instance",
    });
    expect(span!.events).toHaveLength(1);
    expect(JSON.stringify(span)).not.toContain("private-");
  } finally {
    await telemetry.shutdown();
  }
});

test("raw error export requires an explicit owned bootstrap choice", async () => {
  const exporter = new InMemorySpanExporter();
  const telemetry = await bootstrapTelemetry({
    errorDetails: "raw",
    traceExporter: exporter,
    metricExporter: new InMemoryMetricExporter(AggregationTemporality.CUMULATIVE),
  });
  try {
    captureFailure();
    await telemetry.forceFlush();
    expect(exportedDetails(exporter)).toContain("private-message");
    expect(exporter.getFinishedSpans()[0]!.status.message).toBe("private-status");
  } finally {
    await telemetry.shutdown();
  }
});

test("external owner wrapper omits without mutating other pipelines or lifecycle ownership", async () => {
  const raw = new InMemorySpanExporter();
  const safe = new InMemorySpanExporter();
  let closed = 0;
  const originalShutdown = safe.shutdown.bind(safe);
  safe.shutdown = async () => {
    closed++;
    await originalShutdown();
  };
  const provider = new NodeTracerProvider({
    spanProcessors: [
      new SimpleSpanProcessor(createSafeSpanExporter(safe)),
      new SimpleSpanProcessor(raw),
    ],
  });
  trace.setGlobalTracerProvider(provider);
  try {
    captureFailure();
    await provider.forceFlush();
    expect(JSON.stringify(safe.getFinishedSpans())).not.toContain("private-");
    expect(exportedDetails(raw)).toContain("private-message");
    const telemetry = await bootstrapTelemetry({ mode: "external" });
    await telemetry.shutdown();
    expect(closed).toBe(0);
  } finally {
    await provider.shutdown();
    trace.disable();
  }
  expect(closed).toBe(1);
});

test("throwing exporter never replaces the business result or original thrown error", async () => {
  const provider = new NodeTracerProvider({
    spanProcessors: [
      new SimpleSpanProcessor(
        createSafeSpanExporter({
          export() {
            throw new Error("private-export-failure");
          },
          async shutdown() {},
        }),
      ),
    ],
  });
  const tracer = provider.getTracer("test");
  const original = new Error("private-service-failure");
  async function service(fail: boolean) {
    const span = tracer.startSpan("safe-operation");
    try {
      if (fail) throw original;
      return "success";
    } finally {
      span.end();
    }
  }
  try {
    expect(await service(false)).toBe("success");
    await expect(service(true)).rejects.toBe(original);
    await expect(provider.forceFlush()).rejects.toBeDefined();
  } finally {
    await provider.shutdown();
  }
});

test("official beta.42 instrumentation exception and status details are omitted at export", async () => {
  for (const errorDetails of ["omit", "raw"] as const) {
    const exporter = new InMemorySpanExporter();
    const telemetry = await bootstrapTelemetry({
      errorDetails,
      traceExporter: exporter,
      metricExporter: new InMemoryMetricExporter(AggregationTemporality.CUMULATIVE),
      instrumentations: [createORPCInstrumentation()],
    });
    const original = new Error("private-orpc-message", { cause: new Error("private-orpc-cause") });
    original.stack = "private-orpc-stack private-orpc-cause";
    const procedure = os.handler(() => {
      throw original;
    });
    try {
      await expect(call(procedure, undefined)).rejects.toBe(original);
      await telemetry.forceFlush();
      const spans = exporter.getFinishedSpans();
      expect(spans.length).toBeGreaterThan(0);
      expect(spans.some((span) => span.instrumentationScope.name === "@orpc/opentelemetry")).toBe(
        true,
      );
      expect(spans.some((span) => span.status.code === SpanStatusCode.ERROR)).toBe(true);
      if (errorDetails === "omit") {
        expect(exportedDetails(exporter)).not.toContain("private-");
        expect(spans.every((span) => span.status.message === undefined)).toBe(true);
      } else {
        expect(exportedDetails(exporter)).toContain("private-orpc-message");
        expect(exportedDetails(exporter)).toContain("private-orpc-stack");
        expect(exportedDetails(exporter)).toContain("private-orpc-cause");
        expect(spans.some((span) => span.status.message === "private-orpc-message")).toBe(true);
      }
    } finally {
      await telemetry.shutdown();
    }
  }
});
