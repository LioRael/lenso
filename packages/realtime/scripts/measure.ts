import { arch, cpus, platform, release } from "node:os";
import { createRealtime } from "../src/index";
import { createMemoryProvider } from "../src/memory";
import type { Envelope } from "../src/contracts";

const resource = { scope: "measurement", type: "note", id: "one" };
const identity = (subject: string) => ({
  scope: resource.scope,
  subject,
  principal: null,
  expiresAt: Date.now() + 120000,
});
function memory() {
  Bun.gc(true);
  const { rss, heapUsed } = process.memoryUsage();
  return { rss, heapUsed };
}
function quantile(values: number[], percentile: number) {
  const sorted = [...values].sort((a, b) => a - b);
  return Number(
    sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * percentile) - 1)].toFixed(3),
  );
}
const results: unknown[] = [];
for (const count of [100, 500]) {
  const rt = await createRealtime({
    provider: createMemoryProvider(),
    config: {
      maxConnections: count,
      maxPendingOperations: count,
      maxConnectionsPerSubject: 1,
      heartbeatMs: 60000,
      maxBufferedEvents: 32,
    },
    authorize: async () => ({ validUntil: Date.now() + 30000 }),
  });
  const baseline = memory();
  let subject = 0;
  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    idleTimeout: 0,
    async fetch(request) {
      const connection = rt.connect(identity(String(++subject)), { signal: request.signal });
      try {
        await connection.subscribe(resource);
        return connection.response();
      } catch {
        connection.close();
        return new Response("Unavailable", { status: 503 });
      }
    },
  });
  const readers: ReadableStreamDefaultReader<Uint8Array>[] = [];
  try {
    const start = performance.now();
    // Keep admission below the listener backlog; all connections remain open
    // together during the measured broadcast.
    for (let batch = 0; batch < count; batch += 50) {
      await Promise.all(
        Array.from({ length: Math.min(50, count - batch) }, async () => {
          const response = await fetch(server.url, { signal: AbortSignal.timeout(20000) });
          if (response.status !== 200) throw new Error("Connection admission failed");
          readers.push(response.body!.getReader());
        }),
      );
    }
    await Promise.all(readers.map((reader) => reader.read()));
    const setupMs = performance.now() - start;
    const active = memory();
    const latency: number[] = [];
    for (let event = 0; event < 20; event++) {
      const started = performance.now();
      const received = readers.map(async (reader) => {
        const chunk = await reader.read();
        if (chunk.done || !new TextDecoder().decode(chunk.value).includes("note.updated"))
          throw new Error("Missing event");
        latency.push(performance.now() - started);
      });
      await rt.publish(resource, "note.updated", { event, text: "x".repeat(256) });
      await Promise.all(received);
    }
    results.push({
      scenario: "Fetch loopback SSE memory",
      connections: count,
      events: 20,
      deliveries: latency.length,
      payloadTextBytes: 256,
      setupMs: Number(setupMs.toFixed(2)),
      latencyMs: {
        p50: quantile(latency, 0.5),
        p95: quantile(latency, 0.95),
        max: quantile(latency, 1),
      },
      baselineBytes: baseline,
      activeBytes: active,
      deltaBytes: { rss: active.rss - baseline.rss, heapUsed: active.heapUsed - baseline.heapUsed },
    });
  } finally {
    await Promise.all(readers.map((reader) => reader.cancel()));
    await server.stop(true);
    await rt.close();
  }
}

const rt = await createRealtime({
  provider: createMemoryProvider(),
  config: { maxConnections: 500, maxBufferedEvents: 32, heartbeatMs: 60000 },
  authorize: async () => ({ validUntil: Date.now() + 30000 }),
});
try {
  const baseline = memory();
  const iterators: AsyncIterator<Envelope>[] = [];
  for (let n = 0; n < 500; n++) {
    const connection = rt.connect(identity(String(n)));
    await connection.subscribe(resource);
    iterators.push(connection[Symbol.asyncIterator]());
  }
  const active = memory();
  for (let n = 0; n < 31; n++)
    await rt.publish(resource, "note.updated", { text: "x".repeat(256) });
  const full = memory();
  const beforeOverflow = rt.stats();
  await rt.publish(resource, "note.updated", { text: "x".repeat(256) });
  let gaps = 0;
  for (const iterator of iterators) if ((await iterator.next()).value.reason === "overflow") gaps++;
  results.push({
    scenario: "Unread async iterable memory",
    connections: 500,
    bufferedEventsPerConnection: 32,
    payloadTextBytes: 256,
    baselineBytes: baseline,
    activeBytes: active,
    fullBytes: full,
    queueHeapDeltaBytes: full.heapUsed - active.heapUsed,
    beforeOverflow,
    afterOverflow: rt.stats(),
    gaps,
  });
} finally {
  await rt.close();
}

console.log(
  JSON.stringify(
    {
      environment: {
        bun: Bun.version,
        platform: platform(),
        release: release(),
        arch: arch(),
        cpu: cpus()[0]?.model,
      },
      method:
        "One warm process, explicit full GC before memory samples; publish-to-reader-completion latency over owned loopback HTTP. No TLS/proxy/Redis. Limits and results are not capacity promises.",
      results,
    },
    null,
    2,
  ),
);
