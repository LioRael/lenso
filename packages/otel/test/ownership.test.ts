import { expect, test } from "bun:test";
import { metrics, propagation } from "@opentelemetry/api";
import { W3CTraceContextPropagator } from "@opentelemetry/core";
import { InstrumentationBase } from "@opentelemetry/instrumentation";
import { InMemorySpanExporter } from "@opentelemetry/sdk-trace-base";
import { AggregationTemporality, InMemoryMetricExporter } from "@opentelemetry/sdk-metrics";
import { bootstrapTelemetry } from "../src/bun";

class Probe extends InstrumentationBase {
  enabled = 0;
  disabled = 0;
  failEnable = false;
  failDisable = false;
  constructor() {
    super("test-probe", "1", { enabled: false });
  }
  protected init() {
    return [];
  }
  override enable() {
    this.enabled++;
    if (this.failEnable) throw new Error("enable failed");
  }
  override disable() {
    this.disabled++;
    if (this.failDisable) throw new Error("disable failed");
  }
}

const metricExporter = () => new InMemoryMetricExporter(AggregationTemporality.CUMULATIVE);

test("metric export failure is visible without breaking the business operation", async () => {
  let closed = 0;
  const telemetry = await bootstrapTelemetry({
    traceExporter: new InMemorySpanExporter(),
    metricExporter: {
      export(_data, done) {
        done({ code: 1, error: new Error("private driver detail") });
      },
      async forceFlush() {},
      async shutdown() {
        closed++;
      },
    },
  });
  metrics.getMeter("test").createCounter("business.calls").add(1);
  expect(await Promise.resolve("business succeeded")).toBe("business succeeded");
  await expect(telemetry.forceFlush()).rejects.toThrow("Metric export failed");
  await expect(telemetry.shutdown()).rejects.toThrow("Telemetry shutdown failed");
  expect(closed).toBe(0);
});

test("all registrations and owned exporters close even when instrumentation disable throws", async () => {
  const first = new Probe();
  first.failDisable = true;
  const second = new Probe();
  let closed = 0;
  const telemetry = await bootstrapTelemetry({
    takeOwnership: true,
    traceExporter: {
      export(_spans, done) {
        done({ code: 0 });
      },
      async shutdown() {
        closed++;
      },
    },
    metricExporter: metricExporter(),
    instrumentations: [first, second],
  });
  await expect(telemetry.shutdown()).rejects.toThrow("Telemetry shutdown failed");
  expect([first.disabled, second.disabled, closed]).toEqual([1, 1, 1]);
});

test("partial registration rollback disables earlier and failing instrumentation", async () => {
  const first = new Probe();
  const second = new Probe();
  second.failEnable = true;
  await expect(
    bootstrapTelemetry({
      traceExporter: new InMemorySpanExporter(),
      metricExporter: metricExporter(),
      instrumentations: [first, second],
    }),
  ).rejects.toThrow("enable failed");
  expect([first.enabled, first.disabled, second.enabled, second.disabled]).toEqual([1, 1, 1, 1]);
});

test("owned exporter is disposed exactly once when final export fails without a preceding flush", async () => {
  let closed = 0;
  const telemetry = await bootstrapTelemetry({
    takeOwnership: true,
    traceExporter: {
      export(_spans, done) {
        done({ code: 1, error: new Error("export failed") });
      },
      async shutdown() {
        closed++;
      },
    },
    metricExporter: metricExporter(),
  });
  const { trace } = await import("@opentelemetry/api");
  trace.getTracer("test").startSpan("pending").end();
  await expect(telemetry.shutdown()).rejects.toThrow("Telemetry shutdown failed");
  expect(closed).toBe(1);
});

test("timed-out initialization rollback retains ownership until real cleanup", async () => {
  const release = Promise.withResolvers<void>();
  propagation.setGlobalPropagator(new W3CTraceContextPropagator());
  await expect(
    bootstrapTelemetry({
      timeoutMs: 20,
      takeOwnership: true,
      traceExporter: {
        export(_spans, done) {
          done({ code: 0 });
        },
        shutdown: () => release.promise,
      },
      metricExporter: metricExporter(),
    }),
  ).rejects.toThrow("external propagator");
  propagation.disable();
  await expect(bootstrapTelemetry()).rejects.toThrow("already initialized");
  release.resolve();
  await Bun.sleep(1);
  const next = await bootstrapTelemetry({
    traceExporter: new InMemorySpanExporter(),
    metricExporter: metricExporter(),
  });
  await next.shutdown();
});

test("external SDK mode explicitly registers only the requested instrumentation", async () => {
  const probe = new Probe();
  const telemetry = await bootstrapTelemetry({ mode: "external", instrumentations: [probe] });
  expect(probe.enabled).toBe(1);
  await telemetry.shutdown();
  await telemetry.shutdown();
  expect(probe.disabled).toBe(1);
});
