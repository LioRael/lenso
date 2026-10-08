import { expect, test } from "bun:test";
import { os, ORPCError, type RouterClient } from "@orpc/server";
import { createORPCClient } from "@orpc/client";
import { openapi } from "@orpc/openapi";
import { OpenAPILink } from "@orpc/openapi/fetch";
import { z } from "zod";
import { startApp } from "@lenso/core";
import { createOpenAPIAdapter } from "../src/openapi";
import { createProblemDetails, problemType } from "../src/problem-details";
import { createProblemDetailsDecoder } from "../src/openapi-client";
import { createWebPlugin, type WebContext } from "../src/index";

const procedure = os.$context<WebContext>();
const echo = procedure
  .meta(openapi({ method: "POST", path: "/echo" }))
  .input(z.object({ name: z.string().default("secret-default") }))
  .output(z.object({ name: z.string() }))
  .handler(({ input }) => input);
const fail = procedure
  .meta(openapi({ method: "POST", path: "/fail" }))
  .input(z.object({ code: z.string() }))
  .handler(({ input }) => {
    if (input.code === "PLAIN") throw new Error("secret-provider-token");
    if (input.code === "FAKE") throw { code: "CONFLICT", message: "secret-object" };
    throw new ORPCError(input.code, {
      message: "secret-message",
      data: { token: "secret-data" },
      cause: new Error("secret-cause"),
    });
  });
const selectedRouter = { echo, fail };

test("OpenAPI real HTTP and OpenAPILink use selected procedures and safe Problem Details", async () => {
  const occurrences: string[] = [];
  const adapter = createOpenAPIAdapter<WebContext>({
    prefix: "/api",
    selectedRouter,
    onProblem: (problem) => {
      occurrences.push(problem.instance);
      throw new Error("ignored-logger-failure");
    },
    authenticate: (request) => {
      if (request.headers.get("authorization") !== "Bearer test")
        throw new ORPCError("UNAUTHORIZED", {
          message: "secret-session",
        });
    },
  });
  const web = createWebPlugin({
    requires: [],
    router: () => ({
      ...selectedRouter,
      private: procedure
        .meta(openapi({ method: "POST", path: "/private" }))
        .handler(() => "private"),
    }),
    fetch: () => adapter.fetch,
  });
  const app = await startApp({ plugins: [web] });
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: app.get(web).fetch });
  const request = (path: string, body: unknown, authenticated = true) =>
    fetch(new URL(path, server.url), {
      method: "POST",
      headers: {
        "content-type": "application/json",
        ...(authenticated ? { authorization: "Bearer test" } : {}),
      },
      body: JSON.stringify(body),
    });
  try {
    const link = new OpenAPILink(selectedRouter, {
      origin: server.url.origin,
      url: "/api",
      headers: { authorization: "Bearer test" },
      customErrorResponseBodyDecoder: createProblemDetailsDecoder(),
    });
    const client: RouterClient<typeof selectedRouter> = createORPCClient(link);
    expect(await client.echo({ name: "Ada" })).toEqual({ name: "Ada" });
    await expect(client.fail({ code: "CONFLICT" })).rejects.toMatchObject({
      code: "CONFLICT",
      message: "Conflict",
    });
    await expect(client.fail({ code: "UNKNOWN_SECRET" })).rejects.toMatchObject({
      code: "INTERNAL_SERVER_ERROR",
      message: "Internal Server Error",
    });
    const unauthenticated: RouterClient<typeof selectedRouter> = createORPCClient(
      new OpenAPILink(selectedRouter, {
        origin: server.url.origin,
        url: "/api",
        customErrorResponseBodyDecoder: createProblemDetailsDecoder(),
      }),
    );
    await expect(unauthenticated.echo({ name: "Ada" })).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    for (const [path, body, status, authenticated] of [
      ["/api/echo", { name: 42 }, 400, true],
      ["/api/echo", { name: "Ada" }, 401, false],
      ["/api/missing", {}, 404, true],
      ["/api/private", {}, 404, true],
      ["/api/fail", { code: "FORBIDDEN" }, 403, true],
      ["/api/fail", { code: "NOT_FOUND" }, 404, true],
      ["/api/fail", { code: "CONFLICT" }, 409, true],
      ["/api/fail", { code: "UNKNOWN_SECRET" }, 500, true],
      ["/api/fail", { code: "PLAIN" }, 500, true],
      ["/api/fail", { code: "FAKE" }, 500, true],
    ] as const) {
      const response = await request(path, body, authenticated);
      expect(response.status).toBe(status);
      expect(response.headers.get("content-type")).toBe("application/problem+json");
      const text = await response.text();
      expect(text).not.toContain("secret");
      expect(text).not.toContain("UNKNOWN");
      const problem = JSON.parse(text);
      expect(problem.status).toBe(status);
      expect(problem.type).toBe(problemType(problem.code));
      expect(problem.instance).toMatch(/^urn:uuid:/);
      expect(occurrences).toContain(problem.instance);
      expect(Object.keys(problem).sort()).toEqual([
        "code",
        "detail",
        "instance",
        "status",
        "title",
        "type",
      ]);
    }
    const spec = await adapter.generateSpec({ info: { title: "Selected API", version: "1" } });
    expect(Object.keys(spec.paths ?? {}).sort()).toEqual(["/echo", "/fail"]);
    expect(JSON.stringify(spec)).not.toContain("secret-default");
    expect(spec.paths?.["/echo"]?.post?.responses?.["409"]).toMatchObject({
      content: { "application/problem+json": { schema: { type: "object" } } },
    });
    expect((await request("/api/spec.json", {})).status).toBe(404);
  } finally {
    await server.stop(true);
    await app.stop();
  }
});

test("selection defaults none, prefix admission and explicit route metadata are mandatory", async () => {
  const empty = createOpenAPIAdapter({ prefix: "/api", authenticate: () => {} });
  expect(
    (await empty.generateSpec({ info: { title: "Empty", version: "1" } })).paths ?? {},
  ).toEqual({});
  expect(await empty.handle(new Request("https://example.test/rpc/echo"), {})).toBeUndefined();
  expect((await empty.handle(new Request("https://example.test/api/echo"), {}))?.status).toBe(404);
  expect(() =>
    createOpenAPIAdapter({
      prefix: "/api",
      authenticate: () => {},
      selectedRouter: { private: os.handler(() => "private") },
    }),
  ).toThrow("explicit method and path");
  expect(() => createOpenAPIAdapter({ prefix: "/", authenticate: () => {} })).toThrow("prefix");
  expect(() => createOpenAPIAdapter({ prefix: "/api", authenticate: undefined! })).toThrow(
    "authenticate",
  );
});

test("schema conversion fails clearly for runtime-validation-only schemas", async () => {
  const schema = {
    "~standard": {
      version: 1 as const,
      vendor: "runtime-only",
      validate: (value: unknown) => ({ value }),
    },
  };
  const adapter = createOpenAPIAdapter({
    prefix: "/api",
    authenticate: () => {},
    selectedRouter: {
      read: os
        .meta(openapi({ method: "POST", path: "/read" }))
        .input(schema)
        .handler(({ input }) => input),
    },
  });
  await expect(
    adapter.generateSpec({ info: { title: "Unsupported", version: "1" } }),
  ).rejects.toThrow();
});

test("custom domain mapping has matching HTTP, client and spec statuses", async () => {
  const codes = { DUPLICATE: { status: 599, title: "Duplicate", detail: "Choose another name." } };
  const router = {
    create: os.meta(openapi({ method: "POST", path: "/create" })).handler(() => {
      throw new RangeError("secret-domain");
    }),
  };
  const adapter = createOpenAPIAdapter({
    prefix: "/api",
    selectedRouter: router,
    authenticate: () => {},
    codes,
    mapError: (error) => (error instanceof RangeError ? "DUPLICATE" : undefined),
  });
  const client: RouterClient<typeof router> = createORPCClient(
    new OpenAPILink(router, {
      origin: "https://example.test",
      url: "/api",
      customErrorResponseBodyDecoder: createProblemDetailsDecoder({ codes }),
      fetch: async (url, init) => {
        const response = (await adapter.handle(new Request(url, init), {}))!;
        expect(response.status).toBe(599);
        return response;
      },
    }),
  );
  await expect(client.create()).rejects.toMatchObject({ code: "DUPLICATE", message: "Duplicate" });
  const spec = await adapter.generateSpec({ info: { title: "Domain", version: "1" } });
  expect(spec.paths?.["/create"]?.post?.responses?.["599"]).toHaveProperty(
    "content.application/problem+json",
  );
});

test("safe policy recognizes real ORPCError only, configured domain codes, and bounded fixed fields", () => {
  const policy = createProblemDetails({
    codes: { DUPLICATE: { status: 409, title: "Duplicate", detail: "Choose another name." } },
    mapError: (error) => (error instanceof RangeError ? "DUPLICATE" : undefined),
  });
  expect(policy.fromError(new RangeError("secret")).code).toBe("DUPLICATE");
  expect(policy.fromError({ code: "CONFLICT" }).status).toBe(500);
  expect(
    policy.fromCode({ toString: () => "CONFLICT", private: "secret" } as unknown as string).status,
  ).toBe(500);
  expect(policy.fromError(new ORPCError("UNKNOWN")).status).toBe(500);
  const decoder = createProblemDetailsDecoder();
  expect(
    decoder(
      { ...policy.fromCode("CONFLICT"), detail: "secret" },
      {
        status: 409,
        headers: { "content-type": "application/problem+json" },
      },
    ).message,
  ).toBe("Conflict");
  expect(
    decoder(policy.fromCode("CONFLICT"), {
      status: 500,
      headers: { "content-type": "application/problem+json" },
    }).code,
  ).toBe("INTERNAL_SERVER_ERROR");
});

test("RPC and problem status configuration reject invalid runtime numbers", () => {
  for (const status of [399, 600, 400.5, NaN, Infinity, Number.MAX_SAFE_INTEGER]) {
    expect(() =>
      createWebPlugin({ requires: [], router: () => ({}), errorStatusMap: { CUSTOM: status } }),
    ).toThrow("safe integers");
    expect(() =>
      createProblemDetails({
        codes: {
          CUSTOM: { status, title: "Custom", detail: "Refused." },
        },
      }),
    ).toThrow("safe integers");
  }
  for (const status of [400, 599]) {
    expect(() =>
      createWebPlugin({ requires: [], router: () => ({}), errorStatusMap: { CUSTOM: status } }),
    ).not.toThrow();
  }
});

test("pure Fetch boundaries reuse the same bounded safe Problem response helper", async () => {
  const response = createProblemDetails().response(new Error("PRIVATE-SQL-path"));
  expect(response.status).toBe(500);
  expect(response.headers.get("content-type")).toBe("application/problem+json");
  const body = await response.text();
  expect(body).not.toContain("PRIVATE-SQL-path");
  expect(JSON.parse(body)).toMatchObject({
    type: problemType("INTERNAL_SERVER_ERROR"),
    code: "INTERNAL_SERVER_ERROR",
    status: 500,
  });
});

test("OpenAPI snapshots nested selection containers without changing procedure identity", async () => {
  const original = os
    .meta(openapi({ method: "POST", path: "/original" }))
    .handler(() => "original");
  const replacement = os
    .meta(openapi({ method: "POST", path: "/replacement" }))
    .handler(() => "replacement");
  const selected = { nested: { run: original } };
  const adapter = createOpenAPIAdapter({
    selectedRouter: selected,
    prefix: "/api",
    authenticate: () => {},
  });
  selected.nested.run = replacement;
  const request = new Request("https://example.test/api/original", { method: "POST" });
  expect(await (await adapter.handle(request, {}))!.json()).toBe("original");
  const spec = await adapter.generateSpec({ info: { title: "Snapshot", version: "1" } });
  expect(Object.keys(spec.paths!)).toEqual(["/original"]);
  expect(selected.nested.run).toBe(replacement);
});
