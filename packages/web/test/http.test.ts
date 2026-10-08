import { expect, test } from "bun:test";
import { os, ORPCError } from "@orpc/server";
import { safe } from "@orpc/client";
import { defineApp, definePlugin, startApp } from "@lenso/core";
import { z } from "zod";
import { createWebPlugin, type WebContext } from "../src/index";
import { createClient } from "../src/client";

test("real HTTP typed client calls an initialized async dependency", async () => {
  const service = definePlugin({
    id: "business",
    setup: () => ({ greet: async ({ name }: { name: string }) => `Hello, ${name}!` }),
  });
  const router = (greet: (input: { name: string }) => Promise<string>) => ({
    greet: os
      .$context<WebContext>()
      .input(z.object({ name: z.string() }))
      .handler(({ input }) => greet(input)),
  });
  const web = createWebPlugin({
    requires: [service],
    router: (context) => router(context.get(service).greet),
  });
  const app = await startApp(defineApp({ plugins: [web, service] }));
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: app.get(web).fetch });
  try {
    const client = createClient<ReturnType<typeof router>>(new URL("/rpc", server.url));
    const output: string = await client.greet({ name: "Ada" });
    expect(output).toBe("Hello, Ada!");
    expect((await fetch(new URL("/missing", server.url))).status).toBe(404);
    // oxlint-disable-next-line no-constant-condition
    if (false) {
      // This assertion is checked by typecheck; the invalid call never runs.
      // @ts-expect-error The actual inferred router accepts a string name only.
      await client.greet({ name: 42 });
    }
  } finally {
    await server.stop(true);
    await app.stop();
  }
});

test("optional oRPC auth middleware composes without core auth conventions", async () => {
  const procedure = os.$context<WebContext>().use(async ({ context, next }) => {
    if (context.request.headers.get("x-development-token") !== "local-test") {
      throw new ORPCError("UNAUTHORIZED");
    }
    return next();
  });
  const router = { protected: procedure.handler(() => ({ allowed: true })) };
  const web = createWebPlugin({ requires: [], router: () => router });
  const app = await startApp(defineApp({ plugins: [web] }));
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: app.get(web).fetch });
  try {
    const url = new URL("/rpc", server.url);
    await expect(createClient<typeof router>(url).protected()).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    expect(
      await createClient<typeof router>(url, {
        headers: { "x-development-token": "local-test" },
      }).protected(),
    ).toEqual({ allowed: true });
  } finally {
    await server.stop(true);
    await app.stop();
  }
});

test("v2 custom fetch preserves nested inference, POST routing and handler error status policy", async () => {
  let calls = 0;
  const router = {
    nested: {
      echo: os.input(z.object({ name: z.string() })).handler(({ input }) => {
        calls++;
        return { name: input.name, at: new Date(0) };
      }),
      limited: os
        .errors({ RATE_LIMITED: { data: z.object({ retryAfter: z.number() }) } })
        .handler(({ errors }) => {
          throw errors.RATE_LIMITED({ data: { retryAfter: 60 } });
        }),
      unavailable: os.handler(() => {
        throw new ORPCError("SERVICE_UNAVAILABLE");
      }),
      failure: os.handler(() => {
        throw new Error("private-provider-detail");
      }),
    },
  };
  const web = createWebPlugin({
    requires: [],
    prefix: "/api/rpc",
    errorStatusMap: { RATE_LIMITED: 429 },
    router: () => router,
  });
  const app = await startApp({ plugins: [web] });
  const statuses: number[] = [];
  const client = createClient<typeof router>("https://example.test/api/rpc?entry=test", {
    fetch: async (url, init) => {
      expect(typeof url).toBe("string");
      expect(new URL(String(url)).searchParams.get("entry")).toBe("test");
      expect(init?.method).toBe("POST");
      const response = await app.get(web).fetch(new Request(url, init));
      statuses.push(response.status);
      return response;
    },
  });
  try {
    const output: { name: string; at: Date } = await client.nested.echo({ name: "Ada" });
    expect(output).toEqual({ name: "Ada", at: new Date(0) });
    const [error, , definedError, isSuccess] = await safe(client.nested.limited());
    expect(isSuccess).toBe(false);
    expect(error).toMatchObject({ code: "RATE_LIMITED" });
    const retryAfter: number | undefined = definedError?.data.retryAfter;
    expect(retryAfter).toBe(60);
    expect(error).not.toHaveProperty("status");
    await expect(client.nested.unavailable()).rejects.toMatchObject({
      code: "SERVICE_UNAVAILABLE",
    });
    await expect(client.nested.failure()).rejects.toMatchObject({
      code: "INTERNAL_SERVER_ERROR",
      message: "Internal Server Error",
    });
    expect(statuses).toEqual([200, 429, 503, 500]);
    const get = await app.get(web).fetch(new Request("https://example.test/api/rpc/nested/echo"));
    expect(get.status).toBe(404);
    await get.text();
    expect(calls).toBe(1);
  } finally {
    await app.stop();
  }
});

test("v2 does not silently deduplicate router and procedure middleware", async () => {
  let calls = 0;
  const base = os.$context<WebContext>().use(async ({ next }) => {
    calls++;
    return next();
  });
  const router = base.router({ read: base.handler(() => "ok") });
  const web = createWebPlugin({ requires: [], router: () => router });
  const app = await startApp({ plugins: [web] });
  try {
    const client = createClient<typeof router>("https://example.test/rpc", {
      fetch: (url, init) => app.get(web).fetch(new Request(url, init)),
    });
    expect(await client.read()).toBe("ok");
    expect(calls).toBe(2);
  } finally {
    await app.stop();
  }
});
