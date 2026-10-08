import { expect, test } from "bun:test";
import { os, ORPCError } from "@orpc/server";
import { createWebPlugin, type WebContext } from "@lenso/web";
import { createClient } from "@lenso/web/client";
import { definePlugin } from "@lenso/core";
import { createWorkerHandler } from "../src/index";

test("Workers v2 RPC success/error and iterator cancellation retain request ownership", async () => {
  const events: string[] = [];
  const ended = Promise.withResolvers<void>();
  const router = {
    read: os.handler(() => ({ value: 42 })),
    denied: os.handler(() => {
      throw new ORPCError("FORBIDDEN");
    }),
    stream: os.$context<WebContext>().handler(async function* ({ context, signal }) {
      context.onCleanup(() => {
        events.push("request");
      });
      try {
        yield { value: 1 };
        await new Promise<void>((resolve) => {
          if (signal!.aborted) resolve();
          else signal!.addEventListener("abort", () => resolve(), { once: true });
        });
      } finally {
        events.push("producer");
        ended.resolve();
      }
    }),
  };
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
    const web = createWebPlugin({ requires: [resource], router: () => router });
    return { plugins: [resource, web], web };
  });
  const platformWork: Promise<unknown>[] = [];
  const statuses: number[] = [];
  const client = createClient<typeof router>("https://worker.test/rpc", {
    fetch: async (url, init) => {
      const response = await handler.fetch(
        new Request(url, init),
        {},
        {
          waitUntil: (work) => {
            platformWork.push(work);
          },
        },
      );
      statuses.push(response.status);
      return response;
    },
  });
  const result: { value: number } = await client.read();
  expect(result.value).toBe(42);
  await expect(client.denied()).rejects.toMatchObject({ code: "FORBIDDEN" });
  expect(statuses).toEqual([200, 403]);
  expect(events).toEqual(["app", "app"]);
  events.length = 0;
  const abort = new AbortController();
  const iterator = await client.stream(undefined, { signal: abort.signal });
  const first = await iterator.next();
  if (first.done) throw new Error("Expected a stream event");
  const value: number = first.value.value;
  expect(value).toBe(1);
  expect(events).toEqual([]);
  abort.abort();
  await ended.promise;
  await Promise.all(platformWork);
  expect(events).toEqual(["producer", "request", "app"]);
});
