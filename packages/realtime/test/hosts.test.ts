import { expect, test } from "bun:test";
import { defineApp, startApp } from "@lenso/core";
import { createWebPlugin, type WebContext } from "@lenso/web";
import { createWorkerHandler } from "@lenso/workers";
import { os } from "@orpc/server";
import { createRealtimePlugin } from "../src/plugin";
import { createMemoryProvider } from "../src/memory";
import { createRealtime } from "../src/index";
import { createNotesRealtime, type NotesPort } from "../examples/notes";
import type { Identity, Realtime } from "../src/contracts";

const actor: Identity = {
  scope: "single-app",
  subject: "reader",
  principal: {},
  expiresAt: Date.now() + 60000,
};
const resource = { scope: "single-app", type: "note", id: "n1" };
function realtimePlugin() {
  return createRealtimePlugin({
    id: "realtime",
    config: {},
    provider: () => createMemoryProvider(),
    authorize: async () => ({ validUntil: Date.now() + 10000 }),
  });
}

async function readUntil(reader: ReadableStreamDefaultReader<Uint8Array>, text: string) {
  let received = "";
  for (let n = 0; n < 10 && !received.includes(text); n++) {
    const chunk = await reader.read();
    if (chunk.done) break;
    received += new TextDecoder().decode(chunk.value);
  }
  return received;
}

test("raw Web stream works through real Bun Fetch; cancellation and app stop release the connection", async () => {
  const realtime = realtimePlugin();
  const web = createWebPlugin({
    id: "realtime-web",
    requires: [realtime],
    router: () => ({}),
    fetch: (context) => async (requestContext) => {
      if (new URL(requestContext.request.url).pathname !== "/notes/n1/events") return undefined;
      // Real applications obtain this identity from their verified Auth entry.
      const connection = context.get(realtime).connect(actor, { signal: requestContext.signal });
      requestContext.onCleanup(() => connection.close());
      await connection.subscribe(resource, {
        cursor: requestContext.request.headers.get("last-event-id") ?? undefined,
      });
      return connection.response();
    },
  });
  const app = await startApp(defineApp({ plugins: [realtime, web] }));
  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    fetch: app.get(web).fetch,
    idleTimeout: 0,
  });
  try {
    const response = await fetch(new URL("/notes/n1/events", server.url));
    expect(response.headers.get("content-type")).toContain("text/event-stream");
    const reader = response.body!.getReader();
    expect(new TextDecoder().decode((await reader.read()).value)).toContain("event: ready");
    await app.get(realtime).publish(resource, "note.updated", { revision: 2 });
    expect(new TextDecoder().decode((await reader.read()).value)).toContain("note.updated");
    await reader.cancel();
    for (let n = 0; n < 100 && app.get(realtime).stats().connections; n++) await Bun.sleep(2);
    expect(app.get(realtime).stats().connections).toBe(0);
  } finally {
    await server.stop(true);
    await app.stop();
  }
});

test("oRPC beta.42 consumes the same async iterable without provider/router changes", async () => {
  const rt = await createRealtime({
    provider: createMemoryProvider(),
    authorize: async () => ({ validUntil: Date.now() + 10000 }),
  });
  const router = {
    events: os.$context<WebContext>().handler(async function* ({ context, signal }) {
      const connection = rt.connect(actor, { signal: signal ?? context.signal });
      context.onCleanup(() => connection.close());
      try {
        await connection.subscribe(resource);
        for await (const event of connection) yield event;
      } finally {
        connection.close();
      }
    }),
  };
  const web = createWebPlugin({ requires: [], router: () => router });
  const app = await startApp(defineApp({ plugins: [web] }));
  try {
    const response = await app.get(web).fetch(
      new Request("http://localhost/rpc/events", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ json: null }),
      }),
    );
    expect(response.status).toBe(200);
    const reader = response.body!.getReader();
    expect(await readUntil(reader, "ready")).toContain("ready");
    await rt.publish(resource, "note.updated", {});
    expect(await readUntil(reader, "note.updated")).toContain("note.updated");
    await reader.cancel();
    expect(rt.stats().subscriptions).toBe(0);
  } finally {
    await app.stop();
    await rt.close();
  }
});

test("portable entry builds for browser and does not import bun/Redis", async () => {
  const result = await Bun.build({
    entrypoints: [
      new URL("../src/index.ts", import.meta.url).pathname,
      new URL("../src/memory.ts", import.meta.url).pathname,
    ],
    target: "browser",
  });
  expect(result.success).toBe(true);
  for (const output of result.outputs) {
    const text = await output.text();
    expect(text).not.toContain('from "bun"');
    expect(text).not.toContain("RedisClient");
  }
});

test("Workers adapter retains a memory stream until cancellation, no upgrade invented", async () => {
  let running!: Realtime;
  let cleaned = false;
  const handler = createWorkerHandler(() => {
    const realtime = realtimePlugin();
    const web = createWebPlugin({
      requires: [realtime],
      router: () => ({}),
      fetch: (context) => async (requestContext) => {
        running = context.get(realtime);
        requestContext.onCleanup(() => {
          cleaned = true;
        });
        const connection = running.connect(actor, { signal: requestContext.signal });
        requestContext.onCleanup(() => connection.close());
        await connection.subscribe(resource);
        return connection.response();
      },
    });
    return { plugins: [realtime, web], web };
  });
  const work: Promise<unknown>[] = [];
  const response = await handler.fetch(
    new Request("https://app.test/events"),
    {},
    {
      waitUntil: (p) => {
        work.push(p);
      },
    },
  );
  expect(cleaned).toBe(false);
  const reader = response.body!.getReader();
  expect(new TextDecoder().decode((await reader.read()).value)).toContain("ready");
  await reader.cancel();
  await Promise.all(work);
  expect(running.stats()).toMatchObject({ connections: 0, topics: 0 });
  expect(cleaned).toBe(true);
});

test("Notes ordinary async adapter authorizes, snapshots and broadcasts a committed update", async () => {
  const allowed = {};
  let revision = 1;
  const notes: NotesPort<unknown> = {
    async read(identity, id, signal) {
      signal.throwIfAborted();
      if (identity.principal !== allowed) throw new Error("Denied private note");
      return id === "n1" ? { id, title: "Notes", body: "Text", revision } : null;
    },
    async update(identity, id, input) {
      if (identity.principal !== allowed) throw new Error("Denied private note");
      return { id, ...input, revision: ++revision };
    },
  };
  const bridge = await createNotesRealtime(notes, createMemoryProvider());
  try {
    const owner = { ...actor, principal: allowed };
    await expect(bridge.open(actor, "n1")).rejects.toMatchObject({ code: "denied" });
    const a = await bridge.open(owner, "n1"),
      b = await bridge.open(owner, "n1");
    expect(a.snapshot).toMatchObject({ stable: true, value: { revision: 1 } });
    const ai = a.connection[Symbol.asyncIterator](),
      bi = b.connection[Symbol.asyncIterator]();
    await ai.next();
    await bi.next();
    await bridge.update(owner, "n1", { title: "Changed", body: "Changed" });
    expect((await ai.next()).value.data).toEqual({ revision: 2 });
    expect((await bi.next()).value.data).toEqual({ revision: 2 });
  } finally {
    await bridge.close();
  }
});
