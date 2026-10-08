import { expect, test } from "bun:test";
import { os } from "@orpc/server";
import { defineApp, definePlugin, startApp } from "lenso";
import { z } from "zod";
import { createWebPlugin, type WebContext } from "@lenso/web";
import { createClient } from "@lenso/web/client";
import {
  authorize,
  createAuthPlugin,
  createSessionProvider,
  optionalAuth,
  requiredAuth,
  type AuthProvider,
  type Identity,
} from "../src/index";

// Test fixtures only. These headers are never a production provider.
const fixtures: Record<string, Identity> = {
  alice: { subject: "alice", tenantId: "north" },
  bob: { subject: "bob", tenantId: "north" },
  eve: { subject: "eve", tenantId: "south" },
};
const fixture: AuthProvider = {
  async authenticate({ request }) {
    const token = request.headers.get("x-test-identity");
    if (!token) return { status: "anonymous" };
    const identity = fixtures[token];
    return identity ? { status: "authenticated", identity } : { status: "invalid" };
  },
};

test("real HTTP auth middleware injects typed identity; services enforce tenant and object ownership", async () => {
  const auth = createAuthPlugin({ provider: fixture });
  const notes = definePlugin({
    id: "private-notes",
    setup: () => ({
      async read(identity: Identity | null, id: string) {
        // Owned facts come from the object, never a tenantId supplied by a client.
        const note = { id: "one", tenantId: "north", ownerId: "alice", text: "private" };
        await authorize(
          identity,
          (caller) =>
            caller.tenantId === note.tenantId && caller.subject === note.ownerId && id === note.id,
        );
        return { text: note.text };
      },
    }),
  });
  const makeRouter = (
    service: Awaited<ReturnType<typeof notes.setup>>,
    authentication: Parameters<typeof requiredAuth>[0],
  ) => ({
    who: os
      .$context<WebContext>()
      .use(optionalAuth(authentication))
      .handler(({ context }) => context.identity?.subject ?? "anonymous"),
    read: os
      .$context<WebContext>()
      .use(requiredAuth(authentication))
      .input(z.object({ id: z.string() }))
      .handler(({ context, input }) => {
        const identity: Identity = context.identity;
        return service.read(identity, input.id);
      }),
  });
  const web = createWebPlugin({
    requires: [auth, notes],
    router: (context) => makeRouter(context.get(notes), context.get(auth)),
  });
  const app = await startApp(defineApp({ plugins: [web, auth, notes] }));
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: app.get(web).fetch });
  try {
    const client = (token?: string) =>
      createClient<ReturnType<typeof makeRouter>>(new URL("/rpc", server.url), {
        headers: token ? { "x-test-identity": token } : {},
      });
    expect(await client().who()).toBe("anonymous");
    await expect(client().read({ id: "one" })).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    await expect(client("invalid").who()).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    expect(await client("alice").read({ id: "one" })).toEqual({ text: "private" });
    await expect(client("bob").read({ id: "one" })).rejects.toMatchObject({ code: "FORBIDDEN" });
    await expect(client("eve").read({ id: "one" })).rejects.toMatchObject({ code: "FORBIDDEN" });
    await expect(client("alice").read({ id: "another-object" })).rejects.toMatchObject({
      code: "FORBIDDEN",
    });
    // Direct service invocation cannot bypass its authorization check.
    await expect(app.get(notes).read(null, "one")).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    await expect(app.get(notes).read(fixtures.bob!, "one")).rejects.toMatchObject({
      code: "FORBIDDEN",
    });
  } finally {
    await server.stop(true);
    await app.stop();
  }
});

test("session provider uses supported getSession headers API and projects away secrets", async () => {
  let calls = 0;
  const provider = createSessionProvider({
    async getSession({ headers }) {
      calls++;
      return headers.has("cookie")
        ? { user: { id: "alice" }, session: { token: "fixture-secret" } }
        : null;
    },
    identity: (session) => ({ subject: session.user.id }),
  });
  const auth = createAuthPlugin({ provider });
  const app = await startApp(defineApp({ plugins: [auth] }));
  try {
    const context = {
      request: new Request("http://localhost", { headers: { cookie: "fixture-session" } }),
    };
    const identity = await app.get(auth).authenticate(context);
    expect(identity).toEqual({ subject: "alice" });
    expect(Object.isFrozen(identity)).toBe(true);
    expect(await app.get(auth).authenticate(context)).toBe(identity);
    expect(calls).toBe(1);
    expect(
      await app.get(auth).authenticate({ request: new Request("http://localhost") }),
    ).toBeNull();
  } finally {
    await app.stop();
  }
});

test("malformed identity and provider errors fail closed without exposing credentials", async () => {
  const cases: { provider: AuthProvider; code: string }[] = [
    {
      provider: {
        authenticate: async () => ({ status: "authenticated", identity: { subject: "" } }),
      },
      code: "UNAUTHORIZED",
    },
    {
      provider: {
        authenticate: async () => {
          throw new Error("fixture-secret");
        },
      },
      code: "SERVICE_UNAVAILABLE",
    },
  ];
  for (const { provider, code } of cases) {
    const auth = createAuthPlugin({ provider });
    const app = await startApp(defineApp({ plugins: [auth] }));
    try {
      const failure = app.get(auth).authenticate({ request: new Request("http://localhost") });
      await expect(failure).rejects.toMatchObject({ code });
      await expect(failure).rejects.not.toThrow("fixture-secret");
    } finally {
      await app.stop();
    }
  }
});

test("service policy failures are sanitized and cancellation is passed to the provider", async () => {
  await expect(
    authorize(fixtures.alice!, () => {
      throw new Error("fixture-secret");
    }),
  ).rejects.toMatchObject({ code: "SERVICE_UNAVAILABLE", message: "Authentication unavailable" });
  const abort = new AbortController();
  const auth = createAuthPlugin({
    provider: {
      async authenticate({ signal }) {
        expect(signal).toBe(abort.signal);
        abort.abort();
        return { status: "authenticated", identity: fixtures.alice! };
      },
    },
  });
  const app = await startApp(defineApp({ plugins: [auth] }));
  try {
    await expect(
      app
        .get(auth)
        .authenticate({ request: new Request("http://localhost"), signal: abort.signal }),
    ).rejects.toThrow("abort");
  } finally {
    await app.stop();
  }
});
