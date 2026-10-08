import {
  context,
  metrics,
  propagation,
  trace,
  type ContextManager,
  type TextMapPropagator,
} from "@opentelemetry/api";
import { AsyncLocalStorageContextManager } from "@opentelemetry/context-async-hooks";
import { W3CTraceContextPropagator } from "@opentelemetry/core";
import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-http";
import { OTLPMetricExporter } from "@opentelemetry/exporter-metrics-otlp-http";
import { registerInstrumentations, type Instrumentation } from "@opentelemetry/instrumentation";
import { resourceFromAttributes } from "@opentelemetry/resources";
import {
  MeterProvider,
  PeriodicExportingMetricReader,
  type PushMetricExporter,
} from "@opentelemetry/sdk-metrics";
import { BatchSpanProcessor, type Sampler, type SpanExporter } from "@opentelemetry/sdk-trace-base";
import { NodeTracerProvider } from "@opentelemetry/sdk-trace-node";

export interface TelemetryOptions {
  readonly mode?: "owned" | "external";
  readonly serviceName?: string;
  readonly sampler?: Sampler;
  readonly contextManager?: ContextManager;
  /** Supplied exporters/context managers are borrowed unless explicitly transferred. */
  readonly takeOwnership?: boolean;
  readonly propagator?: TextMapPropagator;
  readonly traceExporter?: SpanExporter;
  readonly metricExporter?: PushMetricExporter;
  readonly instrumentations?: Instrumentation[];
  readonly timeoutMs?: number;
  readonly flushOnCliExit?: boolean;
}
export interface Telemetry {
  forceFlush(): Promise<void>;
  shutdown(): Promise<void>;
}
const key = Symbol.for("lenso.telemetry.owner.v1");
const host = globalThis as typeof globalThis & { [key]?: boolean };

function borrow<T extends object>(resource: T, cleanup: "shutdown" | "disable"): T {
  return new Proxy(resource, {
    get(target, property) {
      if (property === cleanup) {
        return cleanup === "shutdown" ? async () => {} : () => resource;
      }
      const value = Reflect.get(target, property, target);
      return typeof value === "function" ? value.bind(target) : value;
    },
  });
}

function ownExporter<T extends { shutdown(): Promise<void> }>(exporter: T): T {
  let shutdown: Promise<void> | undefined;
  return new Proxy(exporter, {
    get(target, property) {
      if (property === "shutdown") {
        return () => (shutdown ??= Promise.resolve().then(() => target.shutdown()));
      }
      const value = Reflect.get(target, property, target);
      return typeof value === "function" ? value.bind(target) : value;
    },
  });
}

function register(
  instrumentations: Instrumentation[],
  providers: Parameters<typeof registerInstrumentations>[0],
): () => unknown[] {
  const cleanups: (() => void)[] = [];
  const cleanup = () => {
    const failures: unknown[] = [];
    for (const dispose of cleanups.splice(0).reverse()) {
      try {
        dispose();
      } catch (error) {
        failures.push(error);
      }
    }
    return failures;
  };
  try {
    for (const instrumentation of instrumentations) {
      try {
        cleanups.push(
          registerInstrumentations({ ...providers, instrumentations: [instrumentation] }),
        );
      } catch (error) {
        try {
          instrumentation.disable();
        } catch {}
        throw error;
      }
    }
  } catch (error) {
    cleanup();
    throw error;
  }
  return cleanup;
}

async function bounded(run: () => Promise<unknown>, timeoutMs: number): Promise<void> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    await Promise.race([
      Promise.resolve().then(run),
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error("Telemetry deadline exceeded")), timeoutMs);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

/** Call from an entry or preload, never from config/plugin imports. */
export async function bootstrapTelemetry(options: TelemetryOptions = {}): Promise<Telemetry> {
  if (options.mode === "external") {
    const unregister = register(options.instrumentations ?? [], {
      tracerProvider: trace.getTracerProvider(),
      meterProvider: metrics.getMeterProvider(),
    });
    let stopped = false;
    return {
      forceFlush: async () => {},
      async shutdown() {
        if (!stopped) {
          stopped = true;
          const failures = unregister();
          if (failures.length)
            throw new AggregateError(failures, "Instrumentation shutdown failed");
        }
      },
    };
  }
  if (host[key]) throw new Error("Telemetry is already initialized");
  const timeoutMs = options.timeoutMs ?? 5000;
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 60_000)
    throw new Error("Invalid telemetry timeout");
  const resource = resourceFromAttributes({ "service.name": options.serviceName ?? "lenso" });
  const traceExporter = options.traceExporter
    ? options.takeOwnership
      ? ownExporter(options.traceExporter)
      : borrow(options.traceExporter, "shutdown")
    : ownExporter(new OTLPTraceExporter({ timeoutMillis: timeoutMs, concurrencyLimit: 1 }));
  const metricExporter = options.metricExporter
    ? options.takeOwnership
      ? ownExporter(options.metricExporter)
      : borrow(options.metricExporter, "shutdown")
    : ownExporter(new OTLPMetricExporter({ timeoutMillis: timeoutMs, concurrencyLimit: 1 }));
  let metricExportFailures = 0;
  const observedMetricExporter = new Proxy(metricExporter, {
    get(target, property) {
      if (property === "export") {
        return (...[data, done]: Parameters<PushMetricExporter["export"]>) =>
          target.export(data, (result) => {
            if (result.code !== 0) metricExportFailures++;
            done(result);
          });
      }
      const value = Reflect.get(target, property, target);
      return typeof value === "function" ? value.bind(target) : value;
    },
  });
  const shutdownProviders = async () => {
    const before = metricExportFailures;
    const outcomes = await Promise.allSettled([
      tracerProvider.shutdown(),
      meterProvider.shutdown(),
    ]);
    // SDK processors can skip exporter disposal when their final export fails.
    outcomes.push(
      ...(await Promise.allSettled([traceExporter.shutdown(), metricExporter.shutdown()])),
    );
    const failures = outcomes.flatMap((outcome) =>
      outcome.status === "rejected" ? [outcome.reason] : [],
    );
    if (metricExportFailures !== before) failures.push(new Error("Metric export failed"));
    return failures;
  };
  const tracerProvider = new NodeTracerProvider({
    resource,
    sampler: options.sampler,
    spanProcessors: [
      new BatchSpanProcessor(traceExporter, {
        maxQueueSize: 2048,
        maxExportBatchSize: 512,
        exportTimeoutMillis: timeoutMs,
      }),
    ],
  });
  const meterProvider = new MeterProvider({
    resource,
    readers: [
      new PeriodicExportingMetricReader({
        exporter: observedMetricExporter,
        exportIntervalMillis: 60_000,
        exportTimeoutMillis: timeoutMs,
      }),
    ],
  });
  const manager = options.contextManager
    ? options.takeOwnership
      ? options.contextManager
      : borrow(options.contextManager, "disable")
    : new AsyncLocalStorageContextManager();
  host[key] = true;
  let registeredTrace = false;
  let registeredMetrics = false;
  let registeredContext = false;
  let registeredPropagation = false;
  let unregister: (() => unknown[]) | undefined;
  try {
    registeredTrace = trace.setGlobalTracerProvider(tracerProvider);
    if (!registeredTrace)
      throw new Error("An external tracer provider is registered; use external mode");
    registeredMetrics = metrics.setGlobalMeterProvider(meterProvider);
    if (!registeredMetrics)
      throw new Error("An external meter provider is registered; use external mode");
    registeredContext = context.setGlobalContextManager(manager);
    if (!registeredContext)
      throw new Error("An external context manager is registered; use external mode");
    manager.enable();
    registeredPropagation = propagation.setGlobalPropagator(
      options.propagator ?? new W3CTraceContextPropagator(),
    );
    if (!registeredPropagation)
      throw new Error("An external propagator is registered; use external mode");
    unregister = register(options.instrumentations ?? [], {
      tracerProvider,
      meterProvider,
    });
  } catch (error) {
    if (registeredTrace) trace.disable();
    if (registeredMetrics) metrics.disable();
    if (registeredContext) context.disable();
    if (registeredPropagation) propagation.disable();
    const cleanup = shutdownProviders().finally(() => {
      delete host[key];
    });
    await bounded(() => cleanup, timeoutMs).catch(() => {});
    throw error;
  }
  let shutdown: Promise<void> | undefined;
  const telemetry: Telemetry = {
    forceFlush: () =>
      bounded(async () => {
        const before = metricExportFailures;
        await Promise.all([tracerProvider.forceFlush(), meterProvider.forceFlush()]);
        if (metricExportFailures !== before) throw new Error("Metric export failed");
      }, timeoutMs),
    shutdown() {
      return (shutdown ??= bounded(async () => {
        try {
          const failures = unregister?.() ?? [];
          failures.push(...(await shutdownProviders()));
          if (failures.length) throw new AggregateError(failures, "Telemetry shutdown failed");
        } finally {
          trace.disable();
          metrics.disable();
          context.disable();
          propagation.disable();
          delete host[key];
        }
      }, timeoutMs));
    },
  };
  if (options.flushOnCliExit) {
    const finishKey = Symbol.for("lenso.telemetry.cli.finish.v1");
    (globalThis as unknown as Record<symbol, unknown>)[finishKey] = async () => {
      try {
        await telemetry.forceFlush();
      } finally {
        try {
          await telemetry.shutdown();
        } finally {
          delete (globalThis as unknown as Record<symbol, unknown>)[finishKey];
        }
      }
    };
  }
  return telemetry;
}
