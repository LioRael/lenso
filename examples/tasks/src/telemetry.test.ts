import { expect, test } from "bun:test";
import { bootstrapTelemetry } from "@lenso/otel/bun";
import { createLogger } from "@lenso/log";
import { InMemorySpanExporter } from "@opentelemetry/sdk-trace-base";
import { AggregationTemporality, InMemoryMetricExporter } from "@opentelemetry/sdk-metrics";

test.skipIf(!process.env.TASK_TEST_DATABASE_URL)(
  "existing report task propagates through PostgreSQL to fresh attempts in another Bun process",
  async () => {
    const spans = new InMemorySpanExporter();
    const measurements = new InMemoryMetricExporter(AggregationTemporality.CUMULATIVE);
    const telemetry = await bootstrapTelemetry({
      traceExporter: spans,
      metricExporter: measurements,
    });
    const logs: string[] = [];
    const logger = createLogger({ level: "debug", stream: { write: (line) => logs.push(line) } });
    const { openResources } = await import("./resources");
    const config = {
      connectionString: process.env.TASK_TEST_DATABASE_URL!,
      queueName: "authorization-test",
    };
    const producer = await openResources(config, { instanceId: "report-producer", logger });
    try {
      const reportId = `telemetry-${crypto.randomUUID()}`;
      const jobId = await producer.queue.enqueue(producer.task, {
        reportId,
        rows: [8, 9],
        failUntilAttempt: 1,
        durationMs: 0,
      });
      const child = Bun.spawn(
        [process.execPath, new URL("./test-fixtures/trace-worker.ts", import.meta.url).pathname],
        {
          env: { ...process.env, LENSO_TEST_JOB_ID: jobId },
          stdout: "pipe",
          stderr: "pipe",
        },
      );
      const [exit, stdout, stderr] = await Promise.all([
        child.exited,
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
      ]);
      expect(exit).toBe(0);
      expect(await producer.service.get(reportId)).toEqual({ sum: 17, count: 2 });
      const result = JSON.parse(stdout) as {
        attempts: { spanId: string; parentId?: string; links: string[] }[];
        labels: Record<string, unknown>[];
      };
      await telemetry.forceFlush();
      const finished = spans.getFinishedSpans();
      const enqueue = finished.find((span) => span.name === "lenso.task.enqueue")!;
      const attempts = result.attempts;
      expect(attempts).toHaveLength(2);
      expect(attempts.every((span) => span.parentId === undefined)).toBe(true);
      expect(attempts.every((span) => span.links[0] === enqueue.spanContext().spanId)).toBe(true);
      expect(new Set(attempts.map((span) => span.spanId)).size).toBe(2);
      const records = [...logs, ...stderr.trim().split("\n")]
        .map((line) => JSON.parse(line))
        .filter((line) => line.jobId === jobId);
      expect(records.some((line) => line.instanceId === "report-producer")).toBe(true);
      expect(
        records.some((line) => line.instanceId === "report-worker" && line.attempt === 2),
      ).toBe(true);
      expect(records.every((line) => line.traceId && line.spanId)).toBe(true);
      expect(JSON.stringify(records)).not.toContain(reportId);
      expect(
        result.labels.every((labels) => Object.keys(labels).every((key) => key === "outcome")),
      ).toBe(true);
      for (const resource of measurements.getMetrics())
        for (const scope of resource.scopeMetrics)
          for (const metric of scope.metrics)
            for (const point of metric.dataPoints)
              expect(Object.keys(point.attributes).every((key) => key === "outcome")).toBe(true);
    } finally {
      await producer.close();
      await telemetry.shutdown();
    }
  },
  20_000,
);
