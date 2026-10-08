import { expect, test } from "bun:test";
import { trace } from "@opentelemetry/api";
import { InMemorySpanExporter } from "@opentelemetry/sdk-trace-base";
import { AggregationTemporality, InMemoryMetricExporter } from "@opentelemetry/sdk-metrics";
import { bootstrapTelemetry } from "@lenso/otel/bun";
import { startApp } from "lenso";
import { createWebPlugin } from "../src/index";

test("Web lifetime survives headers and cancellation until body work and cleanup actually settle", async () => {
  const exporter = new InMemorySpanExporter();
  const telemetry = await bootstrapTelemetry({
    traceExporter: exporter,
    metricExporter: new InMemoryMetricExporter(AggregationTemporality.CUMULATIVE),
  });
  const release = Promise.withResolvers<void>();
  let pullSpan: string | undefined;
  let cleanupSpan: string | undefined;
  const web = createWebPlugin({
    requires: [],
    router: () => ({}),
    telemetry: { requestLifetime: true },
    fetch: () => (request) => {
      request.waitUntil(release.promise);
      request.onCleanup(() => {
        cleanupSpan = trace.getActiveSpan()?.spanContext().spanId;
      });
      return new Response(
        new ReadableStream(
          {
            pull(controller) {
              pullSpan = trace.getActiveSpan()?.spanContext().spanId;
              controller.enqueue(new TextEncoder().encode("first"));
            },
          },
          { highWaterMark: 0 },
        ),
      );
    },
  });
  const app = await startApp({ plugins: [web], instanceId: "stream-instance" });
  try {
    const response = await app.get(web).fetch(
      new Request("https://example.test/raw?token=do-not-export", {
        headers: { authorization: "Bearer do-not-export" },
      }),
    );
    expect(response.status).toBe(200);
    const reader = response.body!.getReader();
    expect(new TextDecoder().decode((await reader.read()).value)).toBe("first");
    const cancelling = reader.cancel();
    await Bun.sleep(1);
    await telemetry.forceFlush();
    expect(exporter.getFinishedSpans().some((span) => span.name === "web.lifetime")).toBe(false);
    expect(cleanupSpan).toBeUndefined();
    release.resolve();
    await cancelling;
    await app.stop();
    await telemetry.forceFlush();
    const span = exporter
      .getFinishedSpans()
      .find((candidate) => candidate.name === "web.lifetime")!;
    expect(span.attributes["lenso.instance.id"]).toBe("stream-instance");
    expect(pullSpan).toBe(span.spanContext().spanId);
    expect(cleanupSpan).toBe(span.spanContext().spanId);
    expect(span.events.map((event) => event.name)).toEqual([
      "response_ready",
      "aborted",
      "cleanup_complete",
    ]);
    expect(JSON.stringify(span.attributes)).not.toContain("do-not-export");
  } finally {
    release.resolve();
    await app.stop();
    await telemetry.shutdown();
  }
});
