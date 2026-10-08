import { InMemoryMetricExporter, AggregationTemporality } from "@opentelemetry/sdk-metrics";
import { bootstrapTelemetry } from "../../src/bun";

await bootstrapTelemetry({
  timeoutMs: 100,
  flushOnCliExit: true,
  metricExporter: new InMemoryMetricExporter(AggregationTemporality.CUMULATIVE),
  traceExporter: {
    export(spans, done) {
      void Bun.write(
        process.env.LENSO_TRACE_OUTPUT!,
        JSON.stringify(
          spans.map((span) => ({
            name: span.name,
            attributes: span.attributes,
            status: span.status,
          })),
        ),
      ).then(() =>
        done(
          process.env.LENSO_EXPORT_FAIL
            ? { code: 1, error: new Error("export failed") }
            : { code: 0 },
        ),
      );
    },
    async shutdown() {},
  },
});
