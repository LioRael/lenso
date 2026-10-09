import { expect, test } from "bun:test";
import { definePlugin } from "@lenso/core/plugin";
import { createWebPlugin } from "@lenso/web";
import { createBindingsPlugin, createWorkerHandler, type WorkerApp } from "../src/index";

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

test("bind receives the started app before dispatch, once per request", async () => {
  const requests: string[][] = [];
  const instanceIds = new Set<string>();
  const handler = createWorkerHandler(() => {
    const events: string[] = [];
    requests.push(events);
    const service = {
      async fetch() {
        events.push("fetch");
        return new Response("ok");
      },
    };
    const web = definePlugin({
      id: "web",
      setup() {
        return service;
      },
    });
    return {
      plugins: [web],
      web,
      bind(running) {
        expect(running.get(web)).toBe(service);
        instanceIds.add(running.instanceId);
        expect(() => running.get(definePlugin({ id: "web", setup: () => service }))).toThrow();
        events.push("bind");
      },
    };
  });
  await Promise.all(
    [0, 1].map(() => handler.fetch(new Request("https://example.com"), {}, executionContext)),
  ).then((responses) => Promise.all(responses.map((response) => response.text())));
  expect(requests).toEqual([
    ["bind", "fetch"],
    ["bind", "fetch"],
  ]);
  expect(instanceIds.size).toBe(2);
});

test("bind failure rolls back and aggregates cleanup failures", async () => {
  const bindFailure = new Error("bind failed");
  const cleanupFailure = new Error("cleanup failed");
  let dispatched = false;
  const handler = createWorkerHandler(() => {
    const web = definePlugin({
      id: "web",
      setup(context) {
        context.onCleanup(() => {
          throw cleanupFailure;
        });
        return {
          async fetch() {
            dispatched = true;
            return new Response("no");
          },
        };
      },
    });
    return {
      plugins: [web],
      web,
      bind() {
        throw bindFailure;
      },
    };
  });
  const error = await handler
    .fetch(new Request("https://example.com"), {}, executionContext)
    .catch((failure: unknown) => failure);
  expect(error).toBeInstanceOf(AggregateError);
  expect((error as AggregateError).errors[0]).toBe(bindFailure);
  expect((error as AggregateError).errors[1]).toBeInstanceOf(AggregateError);
  expect(((error as AggregateError).errors[1] as AggregateError).errors).toEqual([cleanupFailure]);
  expect(dispatched).toBe(false);
});

test.each(["before-bind", "inside-bind"] as const)(
  "abort %s prevents dispatch and drains cleanup once",
  async (phase) => {
    const abort = new AbortController();
    const reason = new Error("cancelled");
    const pending: Promise<unknown>[] = [];
    let bound = 0;
    let dispatched = 0;
    let cleaned = 0;
    const handler = createWorkerHandler(() => {
      const web = definePlugin({
        id: "web",
        setup(owner) {
          owner.onCleanup(() => {
            cleaned++;
          });
          if (phase === "before-bind") abort.abort(reason);
          return {
            async fetch() {
              dispatched++;
              return new Response("no");
            },
          };
        },
      });
      return {
        plugins: [web],
        web,
        bind() {
          bound++;
          abort.abort(reason);
        },
      };
    });
    await expect(
      handler.fetch(
        new Request("https://example.com", { signal: abort.signal }),
        {},
        {
          waitUntil: (work) => {
            pending.push(work);
          },
        },
      ),
    ).rejects.toBe(reason);
    await Promise.all(pending);
    expect(bound).toBe(phase === "before-bind" ? 0 : 1);
    expect(dispatched).toBe(0);
    expect(cleaned).toBe(1);
    expect(pending.length).toBe(phase === "before-bind" ? 0 : 1);
  },
);

test.each(["value", "promise", "rejection"] as const)(
  "invalid %s binding result fails closed and cleans the app",
  async (kind) => {
    let cleaned = 0;
    let dispatched = false;
    const handler = createWorkerHandler(() => {
      const web = definePlugin({
        id: "web",
        setup(owner) {
          owner.onCleanup(() => {
            cleaned++;
          });
          return {
            async fetch() {
              dispatched = true;
              return new Response("no");
            },
          };
        },
      });
      const bind = (() =>
        kind === "value"
          ? true
          : kind === "promise"
            ? Promise.resolve()
            : Promise.reject(
                new Error("unsupported async binding"),
              )) as unknown as WorkerApp["bind"];
      return { plugins: [web], web, bind };
    });
    await expect(
      handler.fetch(new Request("https://example.com"), {}, executionContext),
    ).rejects.toThrow("must return undefined synchronously");
    expect(cleaned).toBe(1);
    expect(dispatched).toBe(false);
  },
);

test("setup failures never invoke bind", async () => {
  let bound = false;
  const failure = new Error("setup failed");
  const plugin = definePlugin({
    id: "setup",
    setup() {
      throw failure;
    },
  });
  const web = definePlugin({
    id: "web",
    async setup() {
      return {
        async fetch() {
          return new Response();
        },
      };
    },
  });
  const handler = createWorkerHandler(() => ({
    plugins: [plugin, web],
    web,
    bind() {
      bound = true;
    },
  }));
  await expect(
    handler.fetch(new Request("https://example.com"), {}, executionContext),
  ).rejects.toBe(failure);
  expect(bound).toBe(false);
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

test("fetch and cleanup failures retain both error identities in order", async () => {
  const fetchFailure = new Error("fetch failed");
  const cleanupFailure = new Error("cleanup failed");
  const handler = createWorkerHandler(() => {
    const web = definePlugin({
      id: "web",
      setup(context) {
        context.onCleanup(() => {
          throw cleanupFailure;
        });
        return {
          async fetch(): Promise<Response> {
            throw fetchFailure;
          },
        };
      },
    });
    return { plugins: [web], web };
  });
  const failure = await handler
    .fetch(new Request("https://example.com"), {}, executionContext)
    .catch((error: unknown) => error);
  expect(failure).toBeInstanceOf(AggregateError);
  const aggregate = failure as AggregateError;
  expect(aggregate.errors[0]).toBe(fetchFailure);
  expect(aggregate.errors[1]).toBeInstanceOf(AggregateError);
  expect((aggregate.errors[1] as AggregateError).errors).toEqual([cleanupFailure]);
  expect((aggregate.errors[1] as AggregateError).errors[0]).toBe(cleanupFailure);
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

test.each(["eof", "cancel", "empty", "error"] as const)(
  "Web/Workers %s waits for tracked work and releases request before app resources",
  async (mode) => {
    const events: string[] = [];
    let settle!: () => void;
    const work = new Promise<void>((resolve) => {
      settle = resolve;
    });
    let registered!: () => void;
    const registration = new Promise<void>((resolve) => {
      registered = resolve;
    });
    const handler = createWorkerHandler(() => {
      const resource = definePlugin({
        id: "owned",
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
          context.waitUntil(work);
          context.onCleanup(() => {
            events.push("request");
          });
          registered();
          if (mode === "empty") return new Response(null, { status: 204 });
          return new Response(
            new ReadableStream<Uint8Array>(
              {
                pull(controller) {
                  if (mode === "eof") {
                    controller.enqueue(new TextEncoder().encode("ok"));
                    controller.close();
                  }
                  if (mode === "error") controller.error(new Error("source failure"));
                },
              },
              { highWaterMark: 0 },
            ),
          );
        },
      });
      return { plugins: [resource, web], web };
    });
    let completed = false;
    const outcome = (async () => {
      const response = await handler.fetch(
        new Request("https://example.com"),
        {},
        executionContext,
      );
      if (mode === "cancel") await response.body!.cancel();
      else await response.text();
    })()
      .then(
        () => undefined,
        (error) => error,
      )
      .finally(() => {
        completed = true;
      });
    await registration;
    await Bun.sleep(5);
    expect(events).toEqual([]);
    expect(completed).toBe(false);
    settle();
    const error = await outcome;
    expect(mode === "error" ? error instanceof Error : error === undefined).toBe(true);
    expect(events).toEqual(["request", "app"]);
  },
  1000,
);
test("incoming abort keeps app cleanup alive through the platform waitUntil", async () => {
  const work: Promise<unknown>[] = [];
  const events: string[] = [];
  const abort = new AbortController();
  const handler = createWorkerHandler(() => {
    const resource = definePlugin({
      id: "resource",
      setup(context) {
        context.onCleanup(async () => {
          await Promise.resolve();
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
        return new Response(
          new ReadableStream({
            cancel() {
              events.push("source");
            },
          }),
        );
      },
    });
    return { plugins: [resource, web], web };
  });
  const response = await handler.fetch(
    new Request("https://example.com", { signal: abort.signal }),
    {},
    {
      waitUntil: (promise) => {
        work.push(promise);
      },
    },
  );
  const body = response.text();
  abort.abort();
  await expect(body).rejects.toThrow();
  await Promise.all(work);
  expect(work).toHaveLength(1);
  expect(events).toEqual(["source", "request", "app"]);
});
