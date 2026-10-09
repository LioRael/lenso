import { expect, spyOn, test } from "bun:test";
import { definePlugin, startApp } from "@lenso/core";
import { defineOperation } from "@lenso/engine/operations";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import { WebStandardStreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/webStandardStreamableHttp.js";
import { CallToolResultSchema } from "@modelcontextprotocol/sdk/types.js";
import { z } from "zod";
import { createHttpMcp, type McpIdentity, type HttpMcpOptions } from "../src/http";
import type { McpRequestContext } from "../src/adapter";

const resource = "http://127.0.0.1:3000/mcp";
const issuer = "https://identity.example/";
function identity(tenant = "one", scopes = ["mcp:use"]): McpIdentity {
  return {
    subject: "alice",
    tenant,
    issuer,
    audience: [resource],
    scopes,
    expiresAt: Date.now() / 1000 + 600,
  };
}
function text(result: unknown): string {
  return JSON.stringify(result);
}

async function fixture(overrides: Partial<HttpMcpOptions<McpIdentity>> = {}) {
  let starts = 0;
  let stops = 0;
  let visible = true;
  let allowed = true;
  let verifies = 0;
  let aborted = false;
  let entered!: () => void;
  let release!: () => void;
  const began = new Promise<void>((resolve) => {
    entered = resolve;
  });
  const blocked = new Promise<void>((resolve) => {
    release = resolve;
  });
  const requests: McpRequestContext<McpIdentity>[] = [];
  const counts = new Map<string, number>();
  const tokens = new Map<string, McpIdentity>([
    ["fixture-one", identity()],
    ["fixture-two", identity("two")],
  ]);
  const plugin = definePlugin({
    id: "http-fixture",
    setup({ onCleanup }) {
      starts++;
      onCleanup(() => {
        stops++;
      });
      const authorize = (context: McpRequestContext<McpIdentity>) => {
        context.signal.throwIfAborted();
        if (context.identity.subject !== "alice") throw new Error("private-authorization-error");
        return context.identity.tenant;
      };
      return {
        async read(_input: Record<string, never>, context: McpRequestContext<McpIdentity>) {
          const tenant = authorize(context);
          return { tenant, count: counts.get(tenant) ?? 0 };
        },
        async write(input: { amount: number }, context: McpRequestContext<McpIdentity>) {
          const tenant = authorize(context);
          counts.set(tenant, (counts.get(tenant) ?? 0) + input.amount);
          return { tenant, count: counts.get(tenant) };
        },
        async wait(input: { cooperative: boolean }, context: McpRequestContext<McpIdentity>) {
          authorize(context);
          entered();
          if (input.cooperative) {
            await new Promise<void>((resolve) => {
              const abort = () => {
                aborted = true;
                resolve();
              };
              context.signal.addEventListener("abort", abort, { once: true });
              if (context.signal.aborted) abort();
            });
            context.signal.throwIfAborted();
          } else await blocked;
          return { finished: true };
        },
        async fail(_input: Record<string, never>, _context: McpRequestContext<McpIdentity>) {
          throw new Error("private-input stack fixture-one Bearer credential");
        },
        async escaped(_input: Record<string, never>, context: McpRequestContext<McpIdentity>) {
          authorize(context);
          return "\\".repeat(400_000);
        },
      };
    },
  });
  const operations = [
    defineOperation({
      plugin,
      method: "read",
      input: z.object({}).strict(),
      description: "Read tenant count.",
      effect: "read",
      context: true,
    }),
    defineOperation({
      plugin,
      method: "write",
      input: z.object({ amount: z.number().int().min(1).max(10) }).strict(),
      description: "Increment tenant count.",
      effect: "write",
      context: true,
    }),
    defineOperation({
      plugin,
      method: "wait",
      input: z.object({ cooperative: z.boolean() }).strict(),
      description: "Wait.",
      context: true,
      cancellation: "cooperative",
    }),
    defineOperation({
      plugin,
      method: "fail",
      input: z.object({}).strict(),
      description: "Fail safely.",
      context: true,
    }),
    defineOperation({
      plugin,
      method: "escaped",
      input: z.object({}).strict(),
      description: "Return escaped JSON.",
      context: true,
    }),
  ];
  const running = await startApp({ plugins: [plugin] });
  const originalConsole = globalThis.console;
  const adapter = await createHttpMcp({
    running,
    plugins: [plugin],
    operations,
    resource,
    authorizationServers: [issuer],
    requiredScopes: ["mcp:use"],
    allowedOrigins: [new URL(resource).origin],
    verifyToken(token) {
      verifies++;
      const verified = tokens.get(token);
      if (!verified) throw new Error("private token verification failure");
      return verified;
    },
    canList: () => visible,
    authorize: () => allowed,
    binding: (_operation, _input, request) => {
      requests.push(request);
      return { context: request };
    },
    ...overrides,
  });
  expect(globalThis.console).toBe(originalConsole);
  const clients: Client[] = [];
  const connect = async (token = "fixture-one", protocolVersion?: string) => {
    const transport = new StreamableHTTPClientTransport(new URL(resource), {
      requestInit: { headers: { authorization: `Bearer ${token}` } },
      fetch: async (input, init) => {
        const request = new Request(input, init);
        if (protocolVersion && request.method === "POST") {
          const body = await request.clone().json();
          if (body.method === "initialize") {
            body.params.protocolVersion = protocolVersion;
            return adapter.fetch(
              new Request(request, { method: "POST", body: JSON.stringify(body) }),
            );
          }
        }
        return adapter.fetch(request);
      },
    });
    const client = new Client({ name: "official-fixture", version: "1" });
    clients.push(client);
    await client.connect(transport);
    return { client, transport };
  };
  const raw = (
    body?: unknown,
    options: {
      token?: string | null;
      session?: string;
      method?: string;
      origin?: string;
      version?: string;
    } = {},
  ) => {
    const headers = new Headers({
      accept: "application/json, text/event-stream",
      "content-type": "application/json",
    });
    if (options.token !== null)
      headers.set("authorization", `Bearer ${options.token ?? "fixture-one"}`);
    if (options.session) headers.set("mcp-session-id", options.session);
    if (options.origin) headers.set("origin", options.origin);
    headers.set("mcp-protocol-version", options.version ?? "2025-11-25");
    return adapter.fetch(
      new Request(resource, {
        method: options.method ?? "POST",
        headers,
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      }),
    );
  };
  return {
    adapter,
    connect,
    raw,
    running,
    plugin,
    tokens,
    requests,
    began,
    release,
    revoke: () => {
      allowed = false;
    },
    hide: () => {
      visible = false;
    },
    starts: () => starts,
    stops: () => stops,
    verifies: () => verifies,
    aborted: () => aborted,
    async close() {
      release();
      await adapter.close();
      await Promise.all(clients.map((client) => client.close()));
      await running.stop();
    },
  };
}

test("official HTTP SDK negotiates, discovers real schema and reuses borrowed app", async () => {
  const f = await fixture();
  try {
    const { client, transport } = await f.connect();
    expect(transport.protocolVersion).toBe("2025-11-25");
    expect(transport.sessionId).toBeTruthy();
    const tools = (await client.listTools()).tools;
    expect(tools).toHaveLength(5);
    expect(tools[1]!.inputSchema.properties).toHaveProperty("amount");
    expect(tools[0]!.annotations?.readOnlyHint).toBe(true);
    for (let i = 0; i < 3; i++)
      expect(
        (await client.callTool({ name: tools[1]!.name, arguments: { amount: 2 } })).isError,
      ).not.toBe(true);
    expect(text(await client.callTool({ name: tools[0]!.name }))).toContain('\\"count\\":6');
    const invalid = await client.callTool({
      name: tools[1]!.name,
      arguments: { amount: -1, tenant: "two", actor: "admin", root: "/" },
    });
    expect(text(invalid)).toContain("invalid-input");
    expect(f.starts()).toBe(1);
    expect(f.stops()).toBe(0);
    expect(new Set(f.requests.map((request) => request.requestId)).size).toBe(f.requests.length);
    const close = f.adapter.close();
    expect(f.adapter.close()).toBe(close);
    await close;
    expect(f.stops()).toBe(0);
    const read = await f.running
      .get(f.plugin)
      .read({}, { identity: identity(), requestId: "owner", signal: new AbortController().signal });
    expect(read.count).toBe(6);
    expect((await f.raw(undefined, { method: "GET" })).status).toBe(503);
  } finally {
    await f.close();
  }
});

test("official client retains 2025-03-26 Streamable HTTP negotiation", async () => {
  const f = await fixture();
  try {
    const { client, transport } = await f.connect("fixture-one", "2025-03-26");
    expect(transport.protocolVersion).toBe("2025-03-26");
    expect((await client.listTools()).tools).toHaveLength(5);
    expect((await client.callTool({ name: "operation_0" })).isError).not.toBe(true);
    expect(
      (
        await f.raw(
          { jsonrpc: "2.0", id: 123, method: "tools/list" },
          { session: transport.sessionId, version: "invalid" },
        )
      ).status,
    ).toBe(400);
  } finally {
    await f.close();
  }
});

test("discovery is not authorization; revocation and hidden calls are rechecked", async () => {
  const f = await fixture();
  try {
    const { client } = await f.connect();
    expect((await client.listTools()).tools).toHaveLength(5);
    f.revoke();
    expect((await client.listTools()).tools).toHaveLength(5);
    expect(
      text(await client.callTool({ name: "operation_1", arguments: { amount: 1 } })),
    ).toContain("forbidden-operation");
    f.hide();
    expect((await client.listTools()).tools).toEqual([]);
    expect(text(await client.callTool({ name: "operation_0" }))).toContain("forbidden-operation");
    expect(f.requests).toHaveLength(0);
  } finally {
    await f.close();
  }
});

test("every protected request verifies token, issuer, expiry, audience and scope", async () => {
  const f = await fixture();
  try {
    const { client, transport } = await f.connect();
    await client.listTools();
    const session = transport.sessionId;
    for (const [token, claims, status] of [
      ["expired", { ...identity(), expiresAt: 1 }, 401],
      ["wrong-audience", { ...identity(), audience: ["https://other.example/mcp"] }, 401],
      ["wrong-issuer", { ...identity(), issuer: "https://other.example/" }, 401],
      ["no-scope", identity("one", []), 403],
    ] as const) {
      f.tokens.set(token, claims);
      const response = await f.raw(
        { jsonrpc: "2.0", id: 7, method: "tools/list" },
        { token, session },
      );
      expect(response.status).toBe(status);
      expect(response.headers.get("www-authenticate")).toContain("resource_metadata=");
      expect(await response.text()).not.toContain(token);
    }
    const before = f.verifies();
    f.tokens.delete("fixture-one");
    expect((await f.raw(undefined, { method: "GET", session })).status).toBe(401);
    expect((await f.raw(undefined, { method: "DELETE", session })).status).toBe(401);
    expect(f.verifies()).toBe(before + 2);
    const unauthenticated = await f.raw(undefined, { method: "GET", token: null });
    expect(unauthenticated.status).toBe(401);
    const metadataUrl = unauthenticated.headers
      .get("www-authenticate")!
      .match(/resource_metadata="([^"]+)"/)![1]!;
    const metadata = await f.adapter.fetch(new Request(metadataUrl));
    expect(await metadata.json()).toMatchObject({ resource, authorization_servers: [issuer] });
  } finally {
    await f.close();
  }
});

test("same session identifier cannot cross identity/tenant bindings", async () => {
  const f = await fixture();
  try {
    const one = await f.connect();
    const two = await f.connect("fixture-two");
    await one.client.callTool({ name: "operation_1", arguments: { amount: 3 } });
    expect(text(await two.client.callTool({ name: "operation_0" }))).toContain('\\"count\\":0');
    const stolen = await f.raw(
      { jsonrpc: "2.0", id: 10, method: "tools/call", params: { name: "operation_0" } },
      { token: "fixture-two", session: one.transport.sessionId },
    );
    expect(stolen.status).toBe(404);
    expect(
      (
        await f.raw(undefined, {
          method: "GET",
          token: "fixture-two",
          session: one.transport.sessionId,
        })
      ).status,
    ).toBe(404);
    f.tokens.set("other-subject", { ...identity(), subject: "bob" });
    expect(
      (
        await f.raw(undefined, {
          method: "DELETE",
          token: "other-subject",
          session: one.transport.sessionId,
        })
      ).status,
    ).toBe(404);
    expect(text(await one.client.callTool({ name: "operation_0" }))).toContain('\\"count\\":3');
  } finally {
    await f.close();
  }
});

test("transport validates Origin/Host, rejects oversized bodies and bounds safe errors", async () => {
  const f = await fixture({ maxFrameBytes: 2048, maxInputBytes: 128, maxOutputBytes: 256 });
  try {
    expect((await f.raw(undefined, { method: "GET", origin: "https://evil.example" })).status).toBe(
      403,
    );
    expect((await f.adapter.fetch(new Request("http://evil.example/mcp"))).status).toBe(403);
    expect((await f.raw({ huge: "x".repeat(4096) })).status).toBe(413);
    const { client, transport } = await f.connect();
    const error = await client.callTool({ name: "operation_3" });
    expect(error.isError).toBe(true);
    expect(text(error)).not.toContain("private-input");
    expect(text(error)).not.toContain("fixture-one");
    expect(Buffer.byteLength((error.content as { text: string }[])[0]!.text)).toBeLessThanOrEqual(
      256,
    );
    expect(
      text(await client.callTool({ name: "operation_0", arguments: { large: "x".repeat(256) } })),
    ).toContain("input-too-large");
    expect((await f.raw(undefined, { method: "GET", session: transport.sessionId })).status).toBe(
      405,
    );
    await transport.terminateSession();
    expect(
      (
        await f.raw(
          { jsonrpc: "2.0", id: 22, method: "tools/list" },
          { session: transport.sessionId ?? "terminated" },
        )
      ).status,
    ).toBe(404);
  } finally {
    await f.close();
  }
});

test("SDK cancellation reaches cooperative service; disconnect is not cancellation", async () => {
  const f = await fixture({ requestTimeoutMs: 80, maxHttpRequests: 2 });
  try {
    const { client } = await f.connect();
    const controller = new AbortController();
    const pending = client
      .request(
        { method: "tools/call", params: { name: "operation_2", arguments: { cooperative: true } } },
        CallToolResultSchema,
        { signal: controller.signal },
      )
      .catch(() => undefined);
    await f.began;
    controller.abort();
    await pending;
    for (let i = 0; !f.aborted() && i < 100; i++) await Bun.sleep(2);
    expect(f.aborted()).toBe(true);
    // A canceled response must release the HTTP waiter without closing the
    // session or waiting for its 1080ms outer deadline.
    await Bun.sleep(5);
    expect((await client.callTool({ name: "operation_0" })).isError).not.toBe(true);
    await f.adapter.close();
    expect(f.stops()).toBe(0);
  } finally {
    await f.close();
  }
  const g = await fixture({ requestTimeoutMs: 2000 });
  try {
    const { transport } = await g.connect();
    const disconnect = new AbortController();
    const pending = g.adapter.fetch(
      new Request(resource, {
        method: "POST",
        signal: disconnect.signal,
        headers: {
          authorization: "Bearer fixture-one",
          accept: "application/json, text/event-stream",
          "content-type": "application/json",
          "mcp-session-id": transport.sessionId!,
          "mcp-protocol-version": "2025-11-25",
        },
        body: JSON.stringify({
          jsonrpc: "2.0",
          id: 20,
          method: "tools/call",
          params: { name: "operation_2", arguments: { cooperative: false } },
        }),
      }),
    );
    await g.began;
    disconnect.abort();
    g.release();
    const response = await pending;
    expect(response.status).toBe(200);
    expect(text(await response.json())).toContain("finished");
    expect(g.aborted()).toBe(false);
  } finally {
    await g.close();
  }
});

test.each([0, ""])(
  "valid cancellation ID %j drains its response and preserves the session",
  async (id) => {
    const f = await fixture({ requestTimeoutMs: 5000, maxHttpRequests: 2 });
    try {
      const { transport } = await f.connect();
      const session = transport.sessionId;
      const pending = f.raw(
        {
          jsonrpc: "2.0",
          id,
          method: "tools/call",
          params: { name: "operation_2", arguments: { cooperative: true } },
        },
        { session },
      );
      await f.began;
      expect(
        (
          await f.raw(
            { jsonrpc: "2.0", method: "notifications/cancelled", params: { requestId: id } },
            { session },
          )
        ).status,
      ).toBe(202);
      const result = await pending;
      expect(result.status).toBe(200);
      expect(text(await result.json())).toContain("request-cancelled");
      expect(f.aborted()).toBe(true);
      expect(
        (await f.raw({ jsonrpc: "2.0", id: 55, method: "tools/list" }, { session })).status,
      ).toBe(200);
    } finally {
      await f.close();
    }
  },
);

test.each([0, ""])(
  "cancellation before SDK handler dispatch refuses execution for ID %j",
  async (id) => {
    let reached!: () => void;
    let deliver!: () => void;
    const admitted = new Promise<void>((resolve) => {
      reached = resolve;
    });
    const original = WebStandardStreamableHTTPServerTransport.prototype.handleRequest;
    const wrapped = new WeakSet<WebStandardStreamableHTTPServerTransport>();
    const spy = spyOn(
      WebStandardStreamableHTTPServerTransport.prototype,
      "handleRequest",
    ).mockImplementation(function (
      this: WebStandardStreamableHTTPServerTransport,
      request,
      options,
    ) {
      if (!wrapped.has(this)) {
        wrapped.add(this);
        const incoming = this.onmessage;
        this.onmessage = (message, extra) => {
          if ("method" in message && message.method === "tools/call") {
            deliver = () => {
              incoming?.(message, extra);
            };
            reached();
          } else incoming?.(message, extra);
        };
      }
      return original.call(this, request, options);
    });
    const f = await fixture();
    try {
      const { transport } = await f.connect();
      const session = transport.sessionId;
      const pending = f.raw(
        {
          jsonrpc: "2.0",
          id,
          method: "tools/call",
          params: { name: "operation_1", arguments: { amount: 3 } },
        },
        { session },
      );
      await admitted;
      await f.raw(
        { jsonrpc: "2.0", method: "notifications/cancelled", params: { requestId: id } },
        { session },
      );
      deliver();
      expect(text(await (await pending).json())).toContain("request-cancelled");
      expect(f.requests).toHaveLength(0);
    } finally {
      await f.close();
      spy.mockRestore();
    }
  },
);

test("SDK transport retains no completed streams/correlations across repeated HTTP calls", async () => {
  const transports: WebStandardStreamableHTTPServerTransport[] = [];
  const original = WebStandardStreamableHTTPServerTransport.prototype.handleRequest;
  const spy = spyOn(
    WebStandardStreamableHTTPServerTransport.prototype,
    "handleRequest",
  ).mockImplementation(function (this: WebStandardStreamableHTTPServerTransport, request, options) {
    if (!transports.includes(this)) transports.push(this);
    return original.call(this, request, options);
  });
  const f = await fixture();
  try {
    const { client } = await f.connect();
    for (let i = 0; i < 40; i++) await client.callTool({ name: "operation_0" });
    // Version-pinned regression for the SDK 1.32.1 JSON-mode resolver leak.
    for (const field of ["_streamMapping", "_requestToStreamMapping", "_requestResponseMap"]) {
      const map = Reflect.get(transports[0]!, field) as Map<unknown, unknown>;
      expect(map.size).toBe(0);
    }
    const escaped = await client.callTool({ name: "operation_4" });
    expect(escaped.isError).not.toBe(true);
    expect(JSON.parse((escaped.content as { text: string }[])[0]!.text)).toHaveLength(400_000);
  } finally {
    await f.close();
    spy.mockRestore();
  }
});

test("services still deny a valid independently authenticated caller", async () => {
  const f = await fixture();
  try {
    f.tokens.set("bob-fixture", { ...identity(), subject: "bob" });
    const { client } = await f.connect("bob-fixture");
    expect((await client.listTools()).tools).toHaveLength(5);
    const result = await client.callTool({ name: "operation_1", arguments: { amount: 1 } });
    expect(result.isError).toBe(true);
    expect(text(result)).not.toContain("private-authorization-error");
    const alice = await f.connect();
    expect(text(await alice.client.callTool({ name: "operation_0" }))).toContain('\\"count\\":0');
  } finally {
    await f.close();
  }
});

test("timeouts retain execution admission, rate limits isolate tenants, sessions are bounded", async () => {
  const f = await fixture({ maxConcurrentCalls: 1, requestTimeoutMs: 30 });
  try {
    const { client } = await f.connect();
    const pending = client.callTool({ name: "operation_2", arguments: { cooperative: false } });
    await f.began;
    expect(text(await client.callTool({ name: "operation_0" }))).toContain("adapter-busy");
    expect(text(await pending)).toContain("request-timeout");
    expect(text(await client.callTool({ name: "operation_0" }))).toContain("adapter-busy");
    f.release();
  } finally {
    await f.close();
  }
  const g = await fixture({
    maxSessions: 1,
    rateLimit: { requests: 2, windowMs: 60_000, maxIdentities: 2 },
  });
  try {
    // Initialization and its initialized notification consume this identity's two requests.
    const { client } = await g.connect();
    await expect(client.listTools()).rejects.toThrow();
    const response = await g.raw(
      {
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: {
          protocolVersion: "2025-11-25",
          capabilities: {},
          clientInfo: { name: "two", version: "1" },
        },
      },
      { token: "fixture-two" },
    );
    expect(response.status).toBe(503);
  } finally {
    await g.close();
  }
});

test("per-identity rate buckets and streamed frame budget cannot be bypassed", async () => {
  const f = await fixture({
    rateLimit: { requests: 2, windowMs: 60_000, maxIdentities: 2 },
    maxFrameBytes: 512,
  });
  try {
    expect((await f.raw(undefined, { method: "GET" })).status).toBe(405);
    expect((await f.raw(undefined, { method: "GET" })).status).toBe(405);
    expect((await f.raw(undefined, { method: "GET" })).status).toBe(429);
    expect((await f.raw(undefined, { method: "GET", token: "fixture-two" })).status).toBe(405);
    const stream = new ReadableStream<Uint8Array>({
      start(controller) {
        for (let i = 0; i < 4; i++) controller.enqueue(new Uint8Array(256).fill(120));
        controller.close();
      },
    });
    const response = await f.adapter.fetch(
      new Request(resource, {
        method: "POST",
        body: stream,
        headers: {
          authorization: "Bearer fixture-two",
          "content-type": "application/json",
          accept: "application/json, text/event-stream",
        },
      }),
    );
    expect(response.status).toBe(413);
    f.tokens.set("third-fixture", identity("three"));
    expect((await f.raw(undefined, { method: "GET", token: "third-fixture" })).status).toBe(429);
  } finally {
    await f.close();
  }
});
