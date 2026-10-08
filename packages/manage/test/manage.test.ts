import { expect, test } from "bun:test";
import { defineApp, definePlugin, startApp } from "@lenso/core";
import { defineOperation, type Operation } from "@lenso/engine/operations";
import { EngineError, stableJson } from "@lenso/engine/diagnostics";
import { AuthError, audience, createAuth, defineSource, realm } from "@lenso/auth";
import { bearerEvidence } from "@lenso/auth/fetch";
import { call, ORPCError, type RouterClient } from "@orpc/server";
import { RPCHandler } from "@orpc/server/fetch";
import { createORPCClient, isDefinedError } from "@orpc/client";
import { RPCLink } from "@orpc/client/fetch";
import { z } from "zod";
import {
  bindManageOperation,
  createAgentTools,
  createManageAdapter,
  defineManage,
  describeManage,
  selectManageOperations,
  type ManageInvocationBinding,
} from "../src";
import { createManageRouter } from "../src/orpc";

const input = z.object({ tenantId: z.string() });

test("explicit selections retain exact instances and snapshot declaration lists without setup", () => {
  let setups = 0;
  const factory = (id: string) =>
    definePlugin({
      id,
      setup() {
        setups++;
        return { read: (value: z.infer<typeof input>) => value, hidden: () => true };
      },
    });
  const a = factory("a");
  const b = factory("b");
  const operation = defineOperation({ plugin: a, method: "read", input, description: "Read" });
  const other = defineOperation({ plugin: b, method: "read", input, description: "Read" });
  const list = [operation];
  const manage = defineManage({
    plugin: a,
    operations: list,
    views: [
      { key: "records", title: "Bearer fixture-secret", columns: ["tenantId"], detail: "read" },
    ],
    extensions: { "app/hints": { token: "fixture-secret", label: "Records" } },
  });
  list.length = 0;
  expect(selectManageOperations(manage, ["read"])[0]).toBe(operation);
  expect(
    selectManageOperations(defineManage({ plugin: b, operations: [other] }), ["read"])[0],
  ).toBe(other);
  expect(selectManageOperations(manage, [])).toEqual([]);
  expect(() => selectManageOperations(manage, ["read", "read"])).toThrow();
  // @ts-expect-error Runtime callers must also be refused for undeclared methods.
  expect(() => selectManageOperations(manage, ["hidden"])).toThrow();
  expect(() => defineManage({ plugin: a, operations: [operation, operation] })).toThrow();
  expect(() => defineManage({ plugin: a, operations: [other] })).toThrow();
  expect(() => defineManage({ plugin: { ...a }, operations: [operation] })).toThrow();
  expect(describeManage(defineManage({ plugin: a, operations: [] })).operations).toEqual([]);
  const descriptor = describeManage(manage);
  expect(stableJson(descriptor)).not.toContain("fixture-secret");
  expect(descriptor.extensions["app/hints"]).toEqual({ label: "Records", token: "[REDACTED]" });
  expect(JSON.parse(stableJson(descriptor))).toEqual(descriptor);
  expect(setups).toBe(0);
  for (const views of [
    [{ key: "x", action: "hidden" }],
    [{ key: "x", view: "missing" }],
    [{ key: "x" }, { key: "x" }],
    [{ key: "x", module: "https://example.com/ui.js" }],
    [{ key: "x", title: () => "X" }],
  ]) {
    expect(() =>
      defineManage({ plugin: a, operations: [operation], views: views as never }),
    ).toThrow();
  }
  expect(() =>
    defineManage({
      plugin: a,
      operations: [operation],
      extensions: { "app/module": new URL("https://example.com") },
    }),
  ).toThrow();
  expect(() =>
    defineManage({ plugin: a, operations: [operation], extensions: { plain: {} } }),
  ).toThrow();
});

test("adapter gates run after one validation, filter callers and never stop or retry writes", async () => {
  let validated = 0;
  let writes = 0;
  let stopped = 0;
  let bindings = 0;
  const schema = input.transform((value) => {
    validated++;
    return value;
  });
  const plugin = definePlugin({
    id: "gated",
    setup(ctx) {
      ctx.onCleanup(() => {
        stopped++;
      });
      return {
        write: async (value: z.infer<typeof input>, context: { caller: string }) => {
          writes++;
          return { tenantId: value.tenantId, caller: context.caller };
        },
      };
    },
  });
  const operation = defineOperation({
    plugin,
    method: "write",
    input: schema,
    description: "Write",
    context: true,
    confirmation: "required",
    approval: "required",
    effect: "write",
  });
  const running = await startApp(defineApp({ plugins: [plugin] }));
  let allowed = true;
  let binding: ManageInvocationBinding<{ caller: string }> = {};
  const adapter = createManageAdapter({
    running,
    plugins: [plugin],
    operations: [operation],
    canList: () => allowed,
    binding: () => {
      bindings++;
      return binding as never;
    },
  });
  try {
    await expect(adapter.invoke("gated", "write", {})).rejects.toThrow();
    expect(bindings).toBe(0);
    for (const supplied of [
      {},
      { context: { caller: "alice" } },
      { context: { caller: "alice" }, confirm: () => true },
      { context: { caller: "alice" }, confirm: () => true, approve: () => false },
      { context: { caller: "alice" }, confirm: () => 1 as unknown as boolean, approve: () => true },
    ]) {
      binding = supplied;
      await expect(
        adapter.invoke("gated", "write", { tenantId: "north", confirmed: true }),
      ).rejects.toThrow();
    }
    expect(writes).toBe(0);
    binding = bindManageOperation(operation, {
      context: { caller: "alice" },
      confirm: () => true,
      approve: () => true,
    }).binding;
    const before = validated;
    expect(await adapter.invoke("gated", "write", { tenantId: "north" })).toEqual({
      tenantId: "north",
      caller: "alice",
    });
    expect(validated - before).toBe(1);
    allowed = false;
    expect(await adapter.catalog()).toEqual([]);
    await expect(adapter.invoke("gated", "write", { tenantId: "north" })).rejects.toThrow();
    await expect(adapter.invoke("gated", "hidden", {})).rejects.toThrow();
    expect(stopped).toBe(0);
    expect(writes).toBe(1);
    expect(() =>
      createManageAdapter({
        running,
        plugins: [{ ...plugin }],
        operations: [operation],
        binding: () => ({ context: { caller: "alice" } }),
        canList: () => true,
      }),
    ).toThrow();
  } finally {
    await running.stop();
  }
  expect(stopped).toBe(1);
});

test("finite output, redaction, opaque unknown errors and no retry share Engine policy", async () => {
  let writes = 0;
  const plugin = definePlugin({
    id: "output",
    setup: () => ({
      value: (_input: z.infer<typeof input>) => ({ token: "private", text: "x".repeat(128) }),
      fail: (_input: z.infer<typeof input>) => {
        writes++;
        throw new Error("private credential");
      },
      stream: (_input: z.infer<typeof input>) =>
        (async function* () {
          yield 1;
        })(),
    }),
  });
  const operations = ["value", "fail", "stream"].map((method) =>
    defineOperation({
      plugin,
      method: method as "value" | "fail" | "stream",
      input,
      description: method,
    }),
  );
  const running = await startApp(defineApp({ plugins: [plugin] }));
  const options = {
    running,
    plugins: [plugin],
    operations,
    binding: () => ({}),
    canList: () => true,
  };
  const adapter = createManageAdapter(options);
  try {
    expect(await adapter.invoke("output", "value", { tenantId: "north" })).toMatchObject({
      token: "[REDACTED]",
    });
    await expect(
      createManageAdapter({ ...options, maxOutputBytes: 8 }).invoke("output", "value", {
        tenantId: "north",
      }),
    ).rejects.toThrow();
    await expect(adapter.invoke("output", "stream", { tenantId: "north" })).rejects.toThrow();
    try {
      await adapter.invoke("output", "fail", { tenantId: "north" });
      throw new Error("Expected failure");
    } catch (error) {
      expect(String(error)).not.toContain("private credential");
    }
    expect(writes).toBe(1);
    const empty = createManageAdapter({ ...options, operations: [] });
    expect(await empty.catalog()).toEqual([]);
    await expect(empty.invoke("output", "value", {})).rejects.toThrow();
  } finally {
    await running.stop();
  }
});

async function securedService() {
  let revoked = false;
  let calls = 0;
  let transformations = 0;
  const auth = createAuth(
    realm(
      "people",
      defineSource({
        async verify(token: string | null) {
          return !revoked && ["alice", "bob"].includes(token ?? "")
            ? { status: "verified" as const, subjectId: token! }
            : { status: "rejected" as const };
        },
      }),
    ),
  );
  const access = auth
    .for(audience("records:read"))
    .memberships(async (subject, resource: { tenantId: string }) =>
      resource.tenantId === (subject.subjectId === "alice" ? "north" : "south")
        ? { role: "reader" }
        : null,
    );
  const wrong = auth.for(audience("records:other"));
  const schema = input.transform((value) => {
    transformations++;
    return value;
  });
  const plugin = definePlugin({
    id: "records",
    setup: () => ({
      read: async (
        resource: z.infer<typeof input>,
        context: { evidence: string | null; wrongAudience?: boolean },
      ) => {
        const actor = context.wrongAudience
          ? await wrong.required(context.evidence)
          : await access.required(context.evidence);
        await access.enforce(
          actor as Awaited<ReturnType<typeof access.required>>,
          resource,
          ({ membership }) => membership.role === "reader",
        );
        await Promise.resolve();
        calls++;
        return { tenantId: resource.tenantId, subjectId: actor.subjectId };
      },
    }),
  });
  const operation = defineOperation({
    plugin,
    method: "read",
    input: schema,
    description: "Read records",
    context: true,
    mapError: (error) =>
      error instanceof AuthError
        ? { code: error.code, phase: "invoke", message: error.message }
        : undefined,
  });
  const running = await startApp(defineApp({ plugins: [plugin] }));
  return {
    running,
    plugin,
    operation,
    auth,
    revoke: () => {
      revoked = true;
    },
    counts: () => ({ calls, transformations }),
  };
}

test("agent and oRPC invoke the same service with isolated fresh evidence and real Auth tenant policies", async () => {
  const fixture = await securedService();
  const { running, plugin, operation } = fixture;
  let allowed = true;
  const adapter = createManageAdapter({
    running,
    plugins: [plugin],
    operations: [operation],
    canList: () => allowed,
    binding: () => ({ context: { evidence: "alice" } }),
  });
  const router = createManageRouter({
    running,
    plugins: [plugin],
    operations: [operation],
    evidence: bearerEvidence,
    binding: (_operation, _input, evidence) => ({ context: { evidence: evidence.evidence } }),
    canList: (_operation, evidence) =>
      allowed && ["alice", "bob"].includes(evidence.evidence ?? ""),
  });
  const context = (token: string) => ({
    request: new Request("https://example.com/rpc", {
      headers: { authorization: `Bearer ${token}` },
    }),
  });
  const invoke = (token: string, tenantId: string) =>
    call(
      router.invoke,
      { pluginId: "records", method: "read", input: { tenantId } },
      { context: context(token) },
    );
  try {
    const tools = await createAgentTools(adapter);
    expect(tools.map((tool) => tool.name)).toEqual(["operation_0"]);
    expect(await tools[0]!.invoke({ tenantId: "north" })).toEqual({
      tenantId: "north",
      subjectId: "alice",
    });
    const before = fixture.counts().transformations;
    expect(await Promise.all([invoke("alice", "north"), invoke("bob", "south")])).toEqual([
      { tenantId: "north", subjectId: "alice" },
      { tenantId: "south", subjectId: "bob" },
    ]);
    expect(fixture.counts().transformations - before).toBe(2);
    await expect(invoke("alice", "south")).rejects.toMatchObject({ code: "FORBIDDEN" });
    await expect(tools[0]!.invoke({ tenantId: "south" })).rejects.toThrow();
    const wrongAudience = createManageAdapter({
      running,
      plugins: [plugin],
      operations: [operation],
      canList: () => true,
      binding: () => ({ context: { evidence: "alice", wrongAudience: true } }),
    });
    await expect(wrongAudience.invoke("records", "read", { tenantId: "north" })).rejects.toThrow();
    expect(await call(router.catalog, undefined, { context: context("unknown") })).toEqual([]);
    allowed = false;
    await expect(tools[0]!.invoke({ tenantId: "north" })).rejects.toThrow();
    expect(await call(router.catalog, undefined, { context: context("alice") })).toEqual([]);
    allowed = true;
    fixture.revoke();
    await expect(invoke("alice", "north")).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    await expect(tools[0]!.invoke({ tenantId: "north" })).rejects.toThrow();
    expect(fixture.counts().calls).toBe(3);
  } finally {
    await running.stop();
    await fixture.auth.close();
  }
});

test("agent refuses runtime-only/nonobject schemas and protocol errors remain opaque", async () => {
  const plugin = definePlugin({ id: "plain", setup: () => ({ read: (_input: string) => "ok" }) });
  const operation = defineOperation({
    plugin,
    method: "read",
    input: z.string(),
    description: "Read",
  });
  const running = await startApp(defineApp({ plugins: [plugin] }));
  try {
    const options = {
      running,
      plugins: [plugin],
      operations: [operation],
      binding: () => ({}),
      canList: () => true,
    };
    await expect(createAgentTools(createManageAdapter(options))).rejects.toThrow();
    const runtimeOnly: Operation = {
      ...operation,
      input: { "~standard": { version: 1, vendor: "fixture", validate: () => ({ value: "ok" }) } },
    };
    await expect(
      createAgentTools(createManageAdapter({ ...options, operations: [runtimeOnly] })),
    ).rejects.toThrow();
    const router = createManageRouter({
      ...options,
      evidence: () => {
        throw new Error("Bearer credential-private");
      },
    });
    try {
      await call(router.catalog, undefined, {
        context: { request: new Request("https://example.com") },
      });
      throw new Error("Expected failure");
    } catch (error) {
      expect(error).toMatchObject({ code: "MANAGE_FAILED", data: { schemaVersion: 1 } });
      expect(JSON.stringify(error)).not.toContain("credential-private");
    }
    await expect(
      call(
        router.invoke,
        { pluginId: "plain", method: "read", input: "x", actor: "forged" } as never,
        { context: { request: new Request("https://example.com") } },
      ),
    ).rejects.toMatchObject({ code: "BAD_REQUEST" });
  } finally {
    await running.stop();
  }
});

test("opaque catalog keys cannot redirect through redacted display identifiers", async () => {
  const old = process.env.MANAGE_TEST_SECRET;
  process.env.MANAGE_TEST_SECRET = "original-target";
  const factory = (id: string, result: number) =>
    definePlugin({ id, setup: () => ({ read: (_input: unknown) => result }) });
  const first = factory("original-target", 1);
  const collision = factory("[REDACTED]", 2);
  const operations = [first, collision].map((plugin) =>
    defineOperation({ plugin, method: "read", input: z.object({}), description: "Read" }),
  );
  const running = await startApp({ plugins: [first, collision] });
  try {
    const adapter = createManageAdapter({
      running,
      plugins: [first, collision],
      operations,
      binding: () => ({}),
      canList: () => true,
    });
    const catalog = await adapter.catalog();
    expect(catalog.map((entry) => entry.pluginId)).toEqual(["[REDACTED]", "[REDACTED]"]);
    expect(catalog.map((entry) => entry.key)).toEqual(["operation_0", "operation_1"]);
    const tools = await createAgentTools(adapter);
    expect(await tools[0]!.invoke({})).toBe(1);
    expect(await tools[1]!.invoke({})).toBe(2);
    const router = createManageRouter({
      running,
      plugins: [first, collision],
      operations,
      evidence: () => ({ evidence: null }),
      binding: () => ({}),
      canList: () => true,
    });
    expect(
      await call(
        router.invoke,
        { key: catalog[0]!.key, input: {} },
        {
          context: { request: new Request("https://example.com") },
        },
      ),
    ).toBe(1);
    await expect(adapter.invokeEntry("operation_00", {})).rejects.toThrow();
  } finally {
    await running.stop();
    if (old === undefined) delete process.env.MANAGE_TEST_SECRET;
    else process.env.MANAGE_TEST_SECRET = old;
  }
});

test("borrowed exact instances and catalog/error byte budgets fail closed", async () => {
  const plugin = definePlugin({ id: "installed", setup: () => ({ read: (_input: unknown) => 1 }) });
  const other = { ...plugin };
  const operation = defineOperation({
    plugin,
    method: "read",
    input: z.object({}),
    description: "Read",
  });
  const impostor = { ...operation, plugin: other };
  const running = await startApp({ plugins: [plugin] });
  try {
    expect(() =>
      createManageAdapter({
        running,
        plugins: [other],
        operations: [impostor],
        binding: () => ({}),
        canList: () => true,
      }),
    ).toThrow("exact running plugin instance");
    const options = {
      running,
      plugins: [plugin],
      operations: [operation],
      binding: () => ({}),
      canList: () => true,
    };
    await expect(
      createManageAdapter({ ...options, maxOutputBytes: 16 }).catalog(),
    ).rejects.toMatchObject({
      diagnostic: { code: "output-too-large", phase: "output" },
    });
    const router = createManageRouter({
      ...options,
      maxOutputBytes: 16,
      evidence: () => ({ evidence: null }),
    });
    try {
      await call(
        router.invoke,
        { pluginId: "x".repeat(100_000), method: "read", input: {} },
        {
          context: { request: new Request("https://example.com") },
        },
      );
      throw new Error("Expected bounded error");
    } catch (error) {
      expect(error).toMatchObject({
        code: "NOT_FOUND",
        data: { diagnostic: { code: "NOT_FOUND" } },
      });
      expect(stableJson((error as { data: unknown }).data).length).toBeLessThan(4096);
    }
    const plainStream = {
      [Symbol.asyncIterator]: async function* () {
        yield 1;
      },
    };
    expect(() => stableJson({ stream: plainStream })).toThrow("streams");
  } finally {
    await running.stop();
  }
});

test("explicit Fetch mounting uses v2 client requests and current Auth catalog evidence", async () => {
  const fixture = await securedService();
  const { running, plugin, operation, auth } = fixture;
  const catalogAccess = auth.for(audience("records:catalog"));
  const router = createManageRouter({
    running,
    plugins: [plugin],
    operations: [operation],
    evidence: bearerEvidence,
    binding: (_operation, _input, evidence) => ({ context: { evidence: evidence.evidence } }),
    async canList(_operation, evidence) {
      const actor = await catalogAccess.required(evidence.evidence, { signal: evidence.signal });
      await catalogAccess.enforce(
        actor,
        { entry: "records" },
        ({ principal }) => principal.kind === "user",
      );
      return true;
    },
  });
  const handler = new RPCHandler(router);
  const client = (credential: string) =>
    createORPCClient<RouterClient<typeof router>>(
      new RPCLink({
        origin: "https://manage.test",
        url: "/manage",
        headers: { authorization: `Bearer ${credential}` },
        async fetch(url, init) {
          const request = new Request(url, init);
          const { matched, response } = await handler.handle(request, {
            prefix: "/manage",
            context: { request },
          });
          return matched ? response : new Response("Not found", { status: 404 });
        },
      }),
    );
  try {
    const alice = client("alice");
    const bob = client("bob");
    const catalog = await alice.catalog();
    const key = catalog[0]!.key;
    expect(
      await Promise.all([
        alice.invoke({ key, input: { tenantId: "north" } }),
        bob.invoke({ key, input: { tenantId: "south" } }),
      ]),
    ).toEqual([
      { tenantId: "north", subjectId: "alice" },
      { tenantId: "south", subjectId: "bob" },
    ]);
    await expect(bob.invoke({ key, input: { tenantId: "north" } })).rejects.toMatchObject({
      code: "FORBIDDEN",
    });
    fixture.revoke();
    await expect(alice.catalog()).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    await expect(alice.invoke({ key, input: { tenantId: "north" } })).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    expect(running.status()).toEqual([{ id: "records", state: "ready" }]);
  } finally {
    await running.stop();
    await auth.close();
  }
});

test("standard RPC clients receive fixed typed HTTP errors without requested identifiers", async () => {
  class DomainError extends Error {
    constructor(readonly kind: string) {
      super("secret internal");
    }
  }
  let failure: unknown;
  let allowed = true;
  const plugin = definePlugin({
    id: "classification",
    setup: () => ({
      run(_input: { name: string }) {
        throw failure;
      },
    }),
  });
  const operation = defineOperation({
    plugin,
    method: "run",
    input: z.object({ name: z.string() }),
    description: "Run",
    mapError: (error) =>
      error instanceof DomainError
        ? { code: error.kind, phase: "invoke", message: "Safe domain message." }
        : undefined,
  });
  const running = await startApp({ plugins: [plugin] });
  const router = createManageRouter({
    running,
    plugins: [plugin],
    operations: [operation],
    evidence: () => ({ evidence: null }),
    canList: () => allowed,
    binding: () => ({}),
  });
  const handler = new RPCHandler(router);
  let httpStatus = 0;
  let wire = "";
  const client = createORPCClient<RouterClient<typeof router>>(
    new RPCLink({
      origin: "https://manage.test",
      url: "/rpc",
      async fetch(url, init) {
        const request = new Request(url, init);
        const { response } = await handler.handle(request, {
          prefix: "/rpc",
          context: { request },
        });
        httpStatus = response!.status;
        wire = await response!.clone().text();
        return response!;
      },
    }),
  );
  const invoke = async (
    pluginId = plugin.id,
    method = "run",
    rawInput: unknown = { name: "ok" },
  ) => {
    const error = await client
      .invoke({ pluginId, method, input: rawInput })
      .catch((cause: unknown) => cause);
    if (!(error instanceof ORPCError)) throw new Error("Expected an oRPC failure.");
    return error;
  };
  try {
    const hidden = await invoke("requested-secret-id");
    expect(hidden).toMatchObject({ code: "NOT_FOUND" });
    expect(isDefinedError(hidden)).toBe(true);
    expect(httpStatus).toBe(404);
    expect(wire).not.toContain("requested-secret-id");
    expect((await invoke(plugin.id, "requested-secret-method")).data).toEqual(hidden.data);
    allowed = false;
    expect((await invoke()).data).toEqual(hidden.data);
    allowed = true;
    expect(await invoke(plugin.id, "run", {})).toMatchObject({ code: "BAD_REQUEST" });
    expect(httpStatus).toBe(400);
    const envelopeFailure = await client
      .invoke({ key: "x", actor: "forged" } as never)
      .catch((e) => e);
    expect(envelopeFailure).toMatchObject({ code: "BAD_REQUEST" });
    expect(httpStatus).toBe(400);
    for (const [kind, code, status] of [
      ["UNAUTHORIZED", "UNAUTHORIZED", 401],
      ["REAUTHENTICATION_REQUIRED", "UNAUTHORIZED", 401],
      ["FORBIDDEN", "FORBIDDEN", 403],
      ["not-found", "NOT_FOUND", 404],
      ["conflict", "CONFLICT", 409],
      ["deduplication-conflict", "CONFLICT", 409],
      ["invalid-key", "BAD_REQUEST", 400],
      ["too-large", "PAYLOAD_TOO_LARGE", 413],
      ["provider", "BAD_GATEWAY", 502],
      ["provider-unavailable", "SERVICE_UNAVAILABLE", 503],
      ["SERVICE_UNAVAILABLE", "SERVICE_UNAVAILABLE", 503],
      ["closed", "SERVICE_UNAVAILABLE", 503],
      ["unsupported", "NOT_IMPLEMENTED", 501],
      ["aborted", "CLIENT_CLOSED_REQUEST", 499],
      ["confirmation-required", "FORBIDDEN", 403],
      ["approval-required", "FORBIDDEN", 403],
    ] as const) {
      failure = new DomainError(kind);
      const error = await invoke();
      expect(error).toMatchObject({ code, data: { diagnostic: { code } } });
      expect(httpStatus).toBe(status);
      expect(isDefinedError(error)).toBe(true);
      expect(wire).not.toContain("secret internal");
      expect(wire).not.toContain("classification");
    }
    failure = new AuthError("SERVICE_UNAVAILABLE");
    expect(await invoke()).toMatchObject({ code: "SERVICE_UNAVAILABLE" });
    expect(httpStatus).toBe(503);
    for (const unknown of [
      { code: "FORBIDDEN", message: "secret fake" },
      new Error("secret internal"),
      new AggregateError([new DomainError("not-found"), new Error("cleanup")]),
      new EngineError(
        { code: "invocation-and-cleanup-failed", phase: "invoke", message: "Safe" },
        { cause: new DomainError("not-found") },
      ),
    ]) {
      failure = unknown;
      expect(await invoke()).toMatchObject({ code: "MANAGE_FAILED" });
      expect(httpStatus).toBe(500);
      expect(wire).not.toContain("secret");
    }
  } finally {
    await running.stop();
  }
});
