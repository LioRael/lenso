import { expect, test } from "bun:test";
import { bootstrapTelemetry } from "@lenso/otel/bun";
import { createORPCInstrumentation } from "@lenso/otel/orpc";
import { createLogger } from "@lenso/log";
import { createClient } from "@lenso/web/client";
import { InMemorySpanExporter } from "@opentelemetry/sdk-trace-base";
import { AggregationTemporality, InMemoryMetricExporter } from "@opentelemetry/sdk-metrics";
import type { createRouter } from "./router";
import { createExampleServer } from "./server";

test("existing greeting CLI operation exports standard OTLP only to the local receiver", async () => {
  const received: Array<{ path: string; body: any }> = [];
  const receiver = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    async fetch(request) {
      received.push({ path: new URL(request.url).pathname, body: await request.json() });
      return Response.json({});
    },
  });
  try {
    const child = Bun.spawn(
      [
        process.execPath,
        "--preload",
        new URL("./telemetry.ts", import.meta.url).pathname,
        new URL("../../../packages/cli/dist/bin.js", import.meta.url).pathname,
        "call",
        "greeting",
        "greet",
        "--root",
        new URL("..", import.meta.url).pathname,
        "--stdin",
        "--json",
      ],
      {
        env: {
          ...process.env,
          OTEL_EXPORTER_OTLP_TRACES_ENDPOINT: new URL("/v1/traces", receiver.url).href,
          OTEL_EXPORTER_OTLP_METRICS_ENDPOINT: new URL("/v1/metrics", receiver.url).href,
          OTEL_EXPORTER_OTLP_HEADERS: "",
          OTEL_EXPORTER_OTLP_TRACES_HEADERS: "",
          OTEL_EXPORTER_OTLP_METRICS_HEADERS: "",
        },
        stdin: new Blob(['{"name":"Ada"}']),
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
    expect(JSON.parse(stdout).data).toEqual({ message: "Hello, Ada!", count: 1 });
    expect(stdout.trim().split("\n")).toHaveLength(1);
    expect(stderr).not.toContain("Telemetry flush failed");
    const spans = received
      .filter((entry) => entry.path === "/v1/traces")
      .flatMap((entry) =>
        entry.body.resourceSpans.flatMap((resource: any) =>
          resource.scopeSpans.flatMap((scope: any) => scope.spans),
        ),
      );
    expect(spans.map((span) => span.name)).toContain("lenso.cli.command");
    expect(spans.map((span) => span.name)).toContain("lenso.operation");
    expect(received.some((entry) => entry.path === "/v1/metrics")).toBe(true);
    expect(JSON.stringify(received)).not.toContain('"Ada"');
  } finally {
    await receiver.stop(true);
  }
});

test("existing greeting Web shares the SDK and isolates concurrent app/request trace and log fields", async () => {
  const exporter = new InMemorySpanExporter();
  const telemetry = await bootstrapTelemetry({
    traceExporter: exporter,
    metricExporter: new InMemoryMetricExporter(AggregationTemporality.CUMULATIVE),
    instrumentations: [createORPCInstrumentation({ propagationEnabled: false })],
  });
  const logs: string[] = [];
  const logger = createLogger({ level: "debug", stream: { write: (line) => logs.push(line) } });
  const first = await createExampleServer(0, {
    instanceId: "first-app",
    logger,
    requestLifetime: true,
  });
  const second = await createExampleServer(0, {
    instanceId: "second-app",
    logger,
    requestLifetime: true,
  });
  try {
    const traceIds = ["1".repeat(32), "2".repeat(32)];
    const clients = [first, second].map((server, index) =>
      createClient<ReturnType<typeof createRouter>>(new URL("/rpc", server.url), {
        headers: { traceparent: `00-${traceIds[index]}-${"3".repeat(16)}-01` },
      }),
    );
    const results = await Promise.all(clients.map((client) => client.greet({ name: "Ada" })));
    expect(results).toEqual([
      { message: "Hello, Ada!", count: 1 },
      { message: "Hello, Ada!", count: 1 },
    ]);
    await Promise.all([first.app.stop(), second.app.stop()]);
    await telemetry.forceFlush();
    const spans = exporter.getFinishedSpans();
    const procedures = spans.filter((span) => span.name === "call_procedure");
    expect(procedures).toHaveLength(2);
    expect(procedures.map((span) => span.attributes["lenso.instance.id"]).sort()).toEqual([
      "first-app",
      "second-app",
    ]);
    expect(new Set(procedures.map((span) => span.spanContext().traceId))).toEqual(
      new Set(traceIds),
    );
    const requestLogs = logs
      .map((line) => JSON.parse(line))
      .filter((line) => line.operation === "web.request");
    expect(requestLogs).toHaveLength(2);
    expect(
      requestLogs.every(
        (line) => line.traceId === traceIds[line.instanceId === "first-app" ? 0 : 1],
      ),
    ).toBe(true);
    const serverParents = new Set(procedures.map((span) => span.parentSpanContext?.spanId));
    expect(
      spans.filter(
        (span) => span.name === "orpc.greet" && serverParents.has(span.spanContext().spanId),
      ),
    ).toHaveLength(2);
  } finally {
    await Promise.all([first.app.stop(), second.app.stop()]);
    await telemetry.shutdown();
  }
});
