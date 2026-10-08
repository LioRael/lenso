import { expect, test } from "bun:test";
import { os } from "@orpc/server";
import { defineApp, startApp } from "lenso";
import { createWebPlugin, type WebContext, type FetchOptions } from "../src/index";
import { createClient } from "../src/client";

function deferred<T = void>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
async function start(
  handler: (context: WebContext) => Response | Promise<Response>,
  options: FetchOptions = {},
) {
  const web = createWebPlugin({
    requires: [],
    router: () => ({}),
    fetch: () => handler,
    ...options,
  });
  const app = await startApp(defineApp({ plugins: [web] }));
  return { app, fetch: app.get(web).fetch };
}
const request = () => new Request("http://localhost/raw");

test("Response return is not body completion; raw bytes pull without prefetch", async () => {
  let pulls = 0;
  const cleaned = deferred();
  const { app, fetch } = await start((context) => {
    context.onCleanup(() => cleaned.resolve());
    return new Response(
      new ReadableStream(
        {
          pull(controller) {
            if (pulls++ === 0) controller.enqueue(new Uint8Array([0, 255, 42]));
            else controller.close();
          },
        },
        { highWaterMark: 0 },
      ),
      { status: 201, headers: { "x-raw": "yes" } },
    );
  });
  const response = await fetch(request());
  expect(pulls).toBe(0);
  expect(response.status).toBe(201);
  expect(response.headers.get("x-raw")).toBe("yes");
  let completed = false;
  void cleaned.promise.then(() => {
    completed = true;
  });
  const reader = response.body!.getReader();
  expect((await reader.read()).value).toEqual(new Uint8Array([0, 255, 42]));
  expect(pulls).toBe(1);
  expect(completed).toBe(false);
  expect((await reader.read()).done).toBe(true);
  await cleaned.promise;
  await app.stop();
});

test("cancel signals upstream immediately but cleanup waits for cancellation and owned work", async () => {
  const cancellation = deferred();
  const producer = deferred();
  const cleaned = deferred();
  let context!: WebContext;
  let cleanupCount = 0;
  const { app, fetch } = await start((ctx) => {
    context = ctx;
    ctx.waitUntil(producer.promise);
    ctx.onCleanup(() => {
      cleanupCount++;
      cleaned.resolve();
    });
    return new Response(
      new ReadableStream({ cancel: () => cancellation.promise }, { highWaterMark: 0 }),
    );
  });
  const response = await fetch(request());
  const cancelling = response.body!.cancel();
  expect(context.signal.aborted).toBe(true);
  expect(context.request.signal.aborted).toBe(true);
  expect(cleanupCount).toBe(0);
  cancellation.resolve();
  await Promise.resolve();
  expect(cleanupCount).toBe(0);
  producer.resolve();
  await cancelling;
  await cleaned.promise;
  await app.stop();
  expect(cleanupCount).toBe(1);
});

test("deadline owns late headers/body and cleanup instead of declaring upstream stopped", async () => {
  const late = deferred<Response>();
  const cleaned = deferred();
  const cancelled = deferred();
  let count = 0;
  const { app, fetch } = await start(
    async (ctx) => {
      ctx.onCleanup(() => {
        count++;
        cleaned.resolve();
      });
      return late.promise;
    },
    { timeoutMs: 10 },
  );
  expect((await fetch(request())).status).toBe(504);
  expect(count).toBe(0);
  late.resolve(
    new Response(
      new ReadableStream(
        {
          cancel() {
            cancelled.resolve();
          },
        },
        { highWaterMark: 0 },
      ),
    ),
  );
  await cancelled.promise;
  await cleaned.promise;
  await app.stop();
  expect(count).toBe(1);
});

test("oversized chunks and source errors fail the body with sanitized diagnostics", async () => {
  for (const oversized of [true, false]) {
    const phases: string[] = [];
    const cleaned = deferred();
    const { app, fetch } = await start(
      (ctx) => {
        ctx.onCleanup(() => cleaned.resolve());
        return new Response(
          new ReadableStream(
            {
              pull(controller) {
                if (oversized) controller.enqueue(new Uint8Array(9));
                else throw new Error("provider-secret");
              },
            },
            { highWaterMark: 0 },
          ),
        );
      },
      { maxChunkBytes: 8, onError: (phase) => phases.push(phase) },
    );
    const response = await fetch(request());
    await expect(response.text()).rejects.toThrow("Response body failed");
    await cleaned.promise;
    expect(phases).toContain("body");
    expect(phases.join()).not.toContain("provider-secret");
    await app.stop();
  }
});

test("body deadline and app stop abort idle streams and finalize once", async () => {
  for (const deadline of [true, false]) {
    const cleaned = deferred();
    let signal!: AbortSignal;
    const { app, fetch } = await start(
      (ctx) => {
        signal = ctx.signal;
        ctx.onCleanup(() => cleaned.resolve());
        return new Response(new ReadableStream({}, { highWaterMark: 0 }));
      },
      deadline ? { timeoutMs: 10 } : {},
    );
    const response = await fetch(request());
    const body = response.text();
    if (!deadline) await app.stop();
    await expect(body).rejects.toThrow("Response cancelled");
    await cleaned.promise;
    expect(signal.aborted).toBe(true);
    await app.stop();
    expect((await fetch(request())).status).toBe(503);
  }
});

test("header errors and late rejection are observed; every cleanup still runs", async () => {
  for (const late of [false, true]) {
    const rejected = Promise.withResolvers<Response>();
    const cleaned = deferred();
    const order: number[] = [];
    const phases: string[] = [];
    const { app, fetch } = await start(
      (ctx) => {
        ctx.onCleanup(() => {
          order.push(1);
          cleaned.resolve();
        });
        ctx.onCleanup(() => {
          order.push(2);
          throw new Error("secret-cleanup-error");
        });
        if (!late) throw new Error("secret-provider-error");
        return rejected.promise;
      },
      { timeoutMs: late ? 10 : undefined, onError: (phase) => phases.push(phase) },
    );
    const response = await fetch(request());
    expect(response.status).toBe(late ? 504 : 500);
    expect(await response.text()).not.toContain("secret");
    if (late) rejected.reject(new Error("secret-late-error"));
    await cleaned.promise;
    expect(order).toEqual([2, 1]);
    expect(phases).toContain("cleanup");
    if (!late) expect(phases).toContain("handler");
    await app.stop();
  }
});

test("EOF keeps disconnect signal live until non-cancellable work actually settles", async () => {
  const producer = deferred();
  const reading = deferred();
  const cleaned = deferred();
  const abort = new AbortController();
  let signal!: AbortSignal;
  let cleanupCount = 0;
  const { app, fetch } = await start((ctx) => {
    signal = ctx.signal;
    ctx.waitUntil(producer.promise);
    ctx.onCleanup(() => {
      cleanupCount++;
      cleaned.resolve();
    });
    return new Response(
      new ReadableStream(
        {
          pull(controller) {
            reading.resolve();
            controller.close();
          },
        },
        { highWaterMark: 0 },
      ),
    );
  });
  const response = await fetch(new Request("http://localhost/raw", { signal: abort.signal }));
  const body = response.text();
  await reading.promise;
  abort.abort();
  expect(signal.aborted).toBe(true);
  expect(cleanupCount).toBe(0);
  producer.resolve();
  await body;
  await cleaned.promise;
  await app.stop();
});

test("real HTTP client disconnect reaches raw upstream Fetch signal", async () => {
  const disconnected = deferred();
  const cleaned = deferred();
  const upstream = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    fetch(req) {
      req.signal.addEventListener("abort", () => disconnected.resolve(), { once: true });
      return new Response(
        new ReadableStream({
          start(controller) {
            controller.enqueue(new TextEncoder().encode("data: first\n\n"));
          },
        }),
        { headers: { "content-type": "text/event-stream" } },
      );
    },
  });
  const { app, fetch: proxyFetch } = await start(async (ctx) => {
    ctx.onCleanup(() => cleaned.resolve());
    return fetch(upstream.url, { signal: ctx.signal });
  });
  const proxy = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: proxyFetch });
  const abort = new AbortController();
  try {
    const response = await fetch(proxy.url, { signal: abort.signal });
    const reader = response.body!.getReader();
    expect(new TextDecoder().decode((await reader.read()).value)).toBe("data: first\n\n");
    abort.abort();
    await disconnected.promise;
    await cleaned.promise;
  } finally {
    await proxy.stop(true);
    await upstream.stop(true);
    await app.stop();
  }
});

test("standard oRPC event iterator retains typed events and finalizes on client abort", async () => {
  const producerEnded = deferred();
  const cleaned = deferred();
  const router = {
    events: os.$context<WebContext>().handler(async function* ({ context, signal }) {
      context.onCleanup(() => cleaned.resolve());
      try {
        yield { message: "first" };
        await new Promise<void>((resolve) => {
          if (signal!.aborted) resolve();
          else signal!.addEventListener("abort", () => resolve(), { once: true });
        });
      } finally {
        producerEnded.resolve();
      }
    }),
  };
  const web = createWebPlugin({ requires: [], router: () => router });
  const app = await startApp(defineApp({ plugins: [web] }));
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: app.get(web).fetch });
  const abort = new AbortController();
  try {
    const iterator = await createClient<typeof router>(new URL("/rpc", server.url)).events(
      undefined,
      { signal: abort.signal },
    );
    const event = await iterator.next();
    const message: string = event.value!.message;
    expect(message).toBe("first");
    abort.abort();
    await producerEnded.promise;
    await cleaned.promise;
  } finally {
    await server.stop(true);
    await app.stop();
  }
});
