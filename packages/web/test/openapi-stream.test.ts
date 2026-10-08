import { expect, test } from "bun:test";
import { asyncIteratorObject, os, ORPCError, withEventMeta, type RouterClient } from "@orpc/server";
import { createORPCClient } from "@orpc/client";
import { OpenAPILink } from "@orpc/openapi/fetch";
import { openapi, type OpenAPIMeta } from "@orpc/openapi";
import { startApp } from "@lenso/core";
import { z } from "zod";
import { createOpenAPIAdapter } from "../src/openapi";
import { createWebPlugin, type WebContext } from "../src/index";

test.each(["CONFLICT", "UNKNOWN_SECRET", "PLAIN"])(
  "consumed HTTP SSE redacts late %s errors without changing status or metadata",
  async (code) => {
    let finalized = 0;
    let resolveCleanup!: () => void;
    const cleaned = new Promise<void>((resolve) => {
      resolveCleanup = resolve;
    });
    const original =
      code === "PLAIN"
        ? new Error("SECRET_MESSAGE")
        : new ORPCError(code, {
            message: "SECRET_MESSAGE",
            data: { token: "SECRET_DATA" },
            cause: new Error("SECRET_CAUSE"),
          });
    let observed: unknown;
    const router = {
      events: os
        .$context<WebContext>()
        .meta(openapi({ method: "GET", path: "/events" }))
        .output(asyncIteratorObject(z.object({ message: z.string() })))
        .handler(async function* ({ context }) {
          context.onCleanup(resolveCleanup);
          try {
            yield withEventMeta({ message: "safe" }, { id: "first", retry: 1000 });
            throw original;
          } finally {
            finalized++;
          }
        }),
    };
    const adapter = createOpenAPIAdapter<WebContext>({
      selectedRouter: router,
      prefix: "/api",
      authenticate: () => {},
      mapError: (error) => {
        observed = error;
        return undefined;
      },
    });
    const web = createWebPlugin({ requires: [], router: () => router, fetch: () => adapter.fetch });
    const app = await startApp({ plugins: [web] });
    const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: app.get(web).fetch });
    try {
      const response = await fetch(new URL("/api/events", server.url));
      expect(response.status).toBe(200);
      expect(response.headers.get("content-type")).toBe("text/event-stream");
      const body = await response.text();
      expect(body).toContain('"message":"safe"');
      expect(body).toContain("id: first");
      expect(body).toContain("retry: 1000");
      expect(body).toContain("event: error");
      expect(body).toContain(
        code === "CONFLICT" ? '"code":"CONFLICT"' : '"code":"INTERNAL_SERVER_ERROR"',
      );
      expect(body).not.toContain("SECRET");
      expect(body).not.toContain("token");
      await cleaned;
      expect(finalized).toBe(1);
      expect(observed).toBe(original);
      expect(response.status).toBe(200);
    } finally {
      await server.stop(true);
      await app.stop();
    }
  },
);

test("OpenAPI detailed SSE retains iterator return and Web cleanup on client cancellation", async () => {
  let finalized = 0;
  let resolveCleanup!: () => void;
  const cleaned = new Promise<void>((resolve) => {
    resolveCleanup = resolve;
  });
  const router = {
    events: os
      .$context<WebContext>()
      .meta(openapi({ method: "GET", path: "/events", outputStructure: "detailed" }))
      .handler(({ context }) => ({
        body: (async function* () {
          context.onCleanup(resolveCleanup);
          try {
            yield withEventMeta({ message: "safe" }, { id: "first" });
            yield { message: "second" };
          } finally {
            finalized++;
          }
        })(),
      })),
  };
  const adapter = createOpenAPIAdapter<WebContext>({
    selectedRouter: router,
    prefix: "/api",
    authenticate: () => {},
  });
  const web = createWebPlugin({ requires: [], router: () => router, fetch: () => adapter.fetch });
  const app = await startApp({ plugins: [web] });
  try {
    const client: RouterClient<typeof router> = createORPCClient(
      new OpenAPILink(router, {
        origin: "https://example.test",
        url: "/api",
        fetch: (url, init) => app.get(web).fetch(new Request(url, init)),
      }),
    );
    const output = await client.events();
    const iterator = output.body;
    expect((await iterator.next()).value).toEqual({ message: "safe" });
    await iterator.return(undefined);
    await cleaned;
    expect(finalized).toBe(1);
  } finally {
    await app.stop();
  }
});

test.each(["CONFLICT", "UNKNOWN_SECRET"])(
  "detailed SSE redacts late %s body errors",
  async (code) => {
    const router = {
      events: os
        .meta(openapi({ method: "GET", path: "/events", outputStructure: "detailed" }))
        .handler(() => ({
          body: (async function* () {
            yield { message: "safe" };
            throw new ORPCError(code, {
              message: "SECRET_MESSAGE",
              data: { token: "SECRET_DATA" },
              cause: new Error("SECRET_CAUSE"),
            });
          })(),
        })),
    };
    const adapter = createOpenAPIAdapter({
      selectedRouter: router,
      prefix: "/api",
      authenticate: () => {},
    });
    const response = (await adapter.handle(new Request("https://example.test/api/events"), {}))!;
    const body = await response.text();
    expect(response.status).toBe(200);
    expect(body).toContain("event: error");
    expect(body).not.toContain("SECRET");
  },
);

test("QUERY uses the same admission policy and Problem Details specification", async () => {
  const router = {
    read: os.meta(openapi({ method: "QUERY", path: "/read" })).handler(() => "safe"),
  };
  const adapter = createOpenAPIAdapter({
    prefix: "/api",
    selectedRouter: router,
    authenticate: () => {
      throw new ORPCError("UNAUTHORIZED", { message: "SECRET_SESSION" });
    },
  });
  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    fetch: async (request) =>
      (await adapter.handle(request, {})) ?? new Response(null, { status: 404 }),
  });
  try {
    const response = await fetch(new URL("/api/read", server.url), { method: "QUERY" });
    expect(response.status).toBe(401);
    const problem = await response.json();
    expect(problem.status).toBe(401);
    expect(JSON.stringify(problem)).not.toContain("SECRET");
    const spec = await adapter.generateSpec({ info: { title: "Query", version: "1" } });
    expect(spec.paths?.["/read"]?.query?.responses?.["401"]).toHaveProperty(
      "content.application/problem+json",
    );
  } finally {
    await server.stop(true);
  }
});

test("selected route metadata is validated before handler or spec construction", () => {
  const create = (metadata: Record<string, unknown>) =>
    createOpenAPIAdapter({
      prefix: "/api",
      authenticate: () => {},
      selectedRouter: {
        read: os.meta(openapi(metadata as OpenAPIMeta)).handler(() => ({
          token: "SUCCESS_DATA",
        })),
      },
    });
  for (const successStatus of [400, 199, NaN, Infinity, 200.5, Number.MAX_SAFE_INTEGER]) {
    expect(() => create({ method: "GET", path: "/read", successStatus })).toThrow("successStatus");
  }
  for (const method of ["BAD", "get", "OPTIONS", "GET?"]) {
    expect(() => create({ method, path: "/read" })).toThrow("unsupported HTTP method");
  }
  for (const path of [
    "bad",
    "//bad",
    "/bad/",
    "/bad?x",
    "/bad#x",
    "/a/../bad",
    "/a\\bad",
    "/bad name",
    "/%2e%2e",
    "/{id}/{id}",
    "/{bad",
  ]) {
    expect(() => create({ method: "GET", path })).toThrow("canonical absolute");
  }
  for (const prefix of ["bad", "/bad?x", "/bad#x", "/bad/", "/a/../bad", "/{id}"]) {
    expect(() => create({ method: "GET", path: "/read", prefix })).toThrow("canonical absolute");
  }
  for (const successStatus of [200, 201, 399]) {
    expect(() =>
      create({ method: "GET", path: "/read/{id}", prefix: "/v1", successStatus }),
    ).not.toThrow();
  }
});
