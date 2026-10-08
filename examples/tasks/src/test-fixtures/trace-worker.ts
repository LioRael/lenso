import { bootstrapTelemetry } from "@lenso/otel/bun";
import { createLogger } from "@lenso/log";
import { InMemorySpanExporter } from "@opentelemetry/sdk-trace-base";
import { AggregationTemporality, InMemoryMetricExporter } from "@opentelemetry/sdk-metrics";

if (import.meta.main) {
  const traces = new InMemorySpanExporter();
  const metrics = new InMemoryMetricExporter(AggregationTemporality.CUMULATIVE);
  const telemetry = await bootstrapTelemetry({
    traceExporter: traces,
    metricExporter: metrics,
    takeOwnership: true,
  });
  try {
    const { openResources } = await import("../resources");
    const resources = await openResources(
      {
        connectionString: process.env.TASK_TEST_DATABASE_URL!,
        queueName: "authorization-test",
      },
      { instanceId: "report-worker", logger: createLogger({ level: "debug" }) },
    );
    try {
      const worker = await resources.queue.startWorker();
      try {
        const deadline = Date.now() + 15_000;
        while ((await resources.queue.get(process.env.LENSO_TEST_JOB_ID!))?.state !== "succeeded") {
          if (Date.now() > deadline) throw new Error("Test task did not complete");
          await Bun.sleep(30);
        }
      } finally {
        await worker.stop();
      }
      await telemetry.forceFlush();
      process.stdout.write(
        JSON.stringify({
          attempts: traces
            .getFinishedSpans()
            .filter((span) => span.name === "lenso.task.attempt")
            .map((span) => ({
              spanId: span.spanContext().spanId,
              parentId: span.parentSpanContext?.spanId,
              links: span.links.map((link) => link.context.spanId),
            })),
          labels: metrics
            .getMetrics()
            .flatMap((resource) =>
              resource.scopeMetrics.flatMap((scope) =>
                scope.metrics.flatMap((metric) =>
                  metric.dataPoints.map((point) => point.attributes),
                ),
              ),
            ),
        }) + "\n",
      );
    } finally {
      await resources.close();
    }
  } finally {
    await telemetry.shutdown();
  }
}
