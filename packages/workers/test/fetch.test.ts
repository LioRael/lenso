import { expect, test } from "bun:test";
import { definePlugin } from "lenso/plugin";
import { createWebPlugin } from "@lenso/web";
import { createBindingsPlugin, createWorkerHandler } from "../src/index";

const executionContext = { waitUntil: (_promise: Promise<unknown>) => {} };

test("bindings are injected per request and cleanup waits for response EOF", async () => {
  let stopped = 0;
  const handler = createWorkerHandler<{ label: string }>((env) => {
    const bindings = createBindingsPlugin({ id: "bindings", bindings: env });
    const web = definePlugin({
      id: "web",
      requires: [bindings],
      setup(context) {
        const { label } = context.get(bindings);
        context.onCleanup(() => {
          stopped++;
        });
        return {
          async fetch() {
            return new Response(label);
          },
        };
      },
    });
    return { plugins: [bindings, web], web };
  });
  const a = await handler.fetch(
    new Request("https://example.com"),
    { label: "A" },
    executionContext,
  );
  const b = await handler.fetch(
    new Request("https://example.com"),
    { label: "B" },
    executionContext,
  );
  expect(stopped).toBe(0);
  expect(await a.text()).toBe("A");
  expect(stopped).toBe(1);
  expect(await b.text()).toBe("B");
  expect(stopped).toBe(2);
});

test("cancelling a response cancels its source and stops the app once", async () => {
  let cancelled = 0;
  let stopped = 0;
  const handler = createWorkerHandler(() => {
    const web = definePlugin({
      id: "web",
      setup(context) {
        context.onCleanup(() => {
          stopped++;
        });
        return {
          async fetch() {
            return new Response(
              new ReadableStream({
                cancel() {
                  cancelled++;
                },
              }),
            );
          },
        };
      },
    });
    return { plugins: [web], web };
  });
  const response = await handler.fetch(new Request("https://example.com"), {}, executionContext);
  await response.body?.cancel("done");
  expect(cancelled).toBe(1);
  expect(stopped).toBe(1);
});

test("fetch failures preserve the original error and release acquired resources", async () => {
  let stopped = 0;
  const failure = new Error("fetch failed");
  const handler = createWorkerHandler(() => {
    const web = definePlugin({
      id: "web",
      setup(context) {
        context.onCleanup(() => {
          stopped++;
        });
        return {
          async fetch(): Promise<Response> {
            throw failure;
          },
        };
      },
    });
    return { plugins: [web], web };
  });
  await expect(
    handler.fetch(new Request("https://example.com"), {}, executionContext),
  ).rejects.toBe(failure);
  expect(stopped).toBe(1);
});

test("a bodyless response releases its app before returning", async () => {
  let stopped = false;
  const handler = createWorkerHandler(() => {
    const web = definePlugin({
      id: "web",
      setup(context) {
        context.onCleanup(() => {
          stopped = true;
        });
        return {
          async fetch() {
            return new Response(null, { status: 204 });
          },
        };
      },
    });
    return { plugins: [web], web };
  });
  const response = await handler.fetch(new Request("https://example.com"), {}, executionContext);
  expect(response.status).toBe(204);
  expect(stopped).toBe(true);
});

test("Web request cleanup finishes before the Workers app releases its dependency", async () => {
  const events: string[] = [];
  const handler = createWorkerHandler(() => {
    const resource = definePlugin({
      id: "resource",
      setup(context) {
        context.onCleanup(() => {
          events.push("app");
        });
        return {};
      },
    });
    const web = createWebPlugin({
      requires: [resource],
      router: () => ({}),
      fetch: () => (context) => {
        context.onCleanup(() => {
          events.push("request");
        });
        return new Response("stream");
      },
    });
    return { plugins: [resource, web], web };
  });
  const response = await handler.fetch(new Request("https://example.com"), {}, executionContext);
  expect(await response.text()).toBe("stream");
  expect(events).toEqual(["request", "app"]);
});
