import { expect, test } from "bun:test";
import { os, call } from "@orpc/server";
import { defineApp, definePlugin, startApp, type PluginContext } from "lenso";
import { z } from "zod";
import { createWebPlugin, type WebContext } from "@lenso/web";
import { createClient } from "@lenso/web/client";
import {
  allPolicies,
  audience,
  authoritativeSession,
  authenticatedWithin,
  AuthConfigurationError,
  AuthError,
  createAuth,
  defineSource,
  realm,
  requireAssurance,
  sessionCreatedWithin,
  type ActorOf,
  type AuthenticationResult,
  type PolicyContext,
  type SessionEvidence,
  type SessionRequirements,
} from "../src/index";
import { createAuthPlugin } from "../src/plugin";
import {
  bearerEvidence,
  headersEvidence,
  authErrorResponse,
  requireSameOrigin,
} from "../src/fetch";
import { optionalAuth, requiredAuth } from "../src/orpc";
import { sessionSource } from "../src/session-source";

// These credentials are fixtures, not a production authentication protocol.
const fixture = defineSource({
  async verify(token: string | null): Promise<AuthenticationResult> {
    if (token === null) return { status: "absent" };
    if (["alice", "bob", "eve"].includes(token)) {
      return { status: "verified", subjectId: token };
    }
    return { status: "rejected" };
  },
});

test("only absent credentials permit anonymous access; malformed proofs fail closed", async () => {
  const auth = createAuth(realm("people", fixture));
  const access = auth.for(audience("notes:read"));
  expect(await access.optional(null)).toBeNull();
  await expect(access.required(null)).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  await expect(access.optional("invalid")).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  for (const invalid of [
    { status: "verified", subjectId: "" },
    { status: "verified", subjectId: "alice", kind: "admin" },
    { status: "unresolved" },
    { status: "verified", subjectId: "alice", session: { expiresAt: NaN } },
  ]) {
    const malformed = createAuth(
      realm(
        "people",
        defineSource({
          verify: async () => invalid as AuthenticationResult,
        }),
      ),
    );
    await expect(malformed.for(audience("notes:read")).optional(undefined)).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    await malformed.close();
  }
  await auth.close();
});

test("actors project no secrets, and plain objects, clones and other instances cannot authorize", async () => {
  const auth = createAuth(realm("people", fixture));
  const access = auth.for(audience("notes:read"));
  const actor = await access.required("alice");
  expect(actor).toMatchObject({
    realmId: "people",
    subjectId: "alice",
    audience: "notes:read",
    kind: "user",
  });
  expect(Object.keys(actor)).toEqual(["realmId", "subjectId", "audience", "kind"]);
  expect(Object.isFrozen(actor)).toBe(true);
  for (const forged of [{ ...actor }, JSON.parse(JSON.stringify(actor))]) {
    await expect(access.enforce(forged, {}, () => true)).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
  }
  const other = createAuth(realm("people", fixture));
  await expect(
    other.for(audience("notes:read")).enforce(actor, {}, () => true),
  ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  const differentAudience = auth.for(audience("notes:write"));
  await expect(
    differentAudience.enforce(
      actor as unknown as ActorOf<typeof differentAudience>,
      {},
      () => true,
    ),
  ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  await expect(access.enforce(null, {}, () => true)).rejects.toMatchObject({
    code: "UNAUTHORIZED",
  });
  await Promise.all([auth.close(), other.close()]);
});

test("services revalidate credentials and membership against the actual object on each operation", async () => {
  let enabled = true;
  let member = true;
  const auth = createAuth(
    realm(
      "people",
      defineSource({
        async verify(token: string) {
          return enabled && token === "alice"
            ? { status: "verified", subjectId: "alice" }
            : { status: "rejected" };
        },
      }),
    ),
  );
  const access = auth
    .for(audience("notes:read"))
    .memberships(async (subject, note: { tenantId: string }) => {
      expect(subject.realmId).toBe("people");
      return member && note.tenantId === "north" ? { role: "owner" as const } : null;
    });
  const actor = await access.required("alice");
  await access.enforce(
    actor,
    { tenantId: "north" },
    ({ membership }) => membership.role === "owner",
  );
  await expect(access.enforce(actor, { tenantId: "south" }, () => true)).rejects.toMatchObject({
    code: "FORBIDDEN",
  });
  member = false;
  await expect(access.enforce(actor, { tenantId: "north" }, () => true)).rejects.toMatchObject({
    code: "FORBIDDEN",
  });
  member = true;
  enabled = false;
  await expect(access.enforce(actor, { tenantId: "north" }, () => true)).rejects.toMatchObject({
    code: "UNAUTHORIZED",
  });
  await auth.close();
});

test("per-entry requirements intersect, reverify authoritatively and never fake reauthentication", async () => {
  let now = 1000;
  let session: SessionEvidence = {
    expiresAt: 5000,
    sessionCreatedAt: 900,
    authenticatedAt: 800,
    assurance: ["mfa"],
    authoritative: false,
  };
  const authoritativeReads: boolean[] = [];
  const auth = createAuth(
    realm(
      "people",
      defineSource({
        capabilities: {
          authoritative: true,
          sessionCreatedAt: true,
          authenticatedAt: true,
          assurance: ["mfa"],
        },
        async verify(_evidence: string, context) {
          authoritativeReads.push(context.authoritative ?? false);
          return {
            status: "verified",
            subjectId: "alice",
            session: { ...session, authoritative: context.authoritative === true },
          };
        },
      }),
    ),
    { now: () => now },
  );
  const base = auth.for(audience("notes:read"));
  const actor = await base.required("session");
  const strong = base.requireSession(
    authoritativeSession(),
    sessionCreatedWithin(200),
    authenticatedWithin(300),
    requireAssurance("mfa"),
  );
  await strong.enforce(actor, {}, () => true);
  expect(authoritativeReads).toEqual([false, true]);
  now = 1100;
  await expect(strong.enforce(actor, {}, () => true)).rejects.toMatchObject({
    code: "REAUTHENTICATION_REQUIRED",
  });
  await expect(
    strong.requireSession(sessionCreatedWithin(1000)).required("session"),
  ).rejects.toMatchObject({ code: "REAUTHENTICATION_REQUIRED" });
  session = { expiresAt: 5000, sessionCreatedAt: 1050, assurance: ["mfa"] };
  await expect(strong.required("session")).rejects.toMatchObject({
    code: "REAUTHENTICATION_REQUIRED",
  });
  expect(() =>
    createAuth(realm("plain", fixture))
      .for(audience("notes:read"))
      .requireSession(authenticatedWithin(300)),
  ).toThrow(AuthConfigurationError);
  await auth.close();
});

test("source errors, membership failures and policies expose only safe errors", async () => {
  const broken = createAuth(
    realm(
      "people",
      defineSource({
        async verify() {
          throw new Error("fixture-secret");
        },
      }),
    ),
  ).for(audience("notes:read"));
  await expect(broken.required(undefined)).rejects.toMatchObject({
    code: "SERVICE_UNAVAILABLE",
    message: "Authentication unavailable",
  });
  const auth = createAuth(realm("people", fixture));
  const access = auth.for(audience("notes:read"));
  const actor = await access.required("alice");
  await expect(
    access.enforce(actor, {}, () => {
      throw new Error("fixture-secret");
    }),
  ).rejects.toMatchObject({ code: "SERVICE_UNAVAILABLE", message: "Authentication unavailable" });
  await expect(
    access.enforce(actor, {}, () => {
      throw new AuthError("FORBIDDEN");
    }),
  ).rejects.toMatchObject({ code: "FORBIDDEN" });
  await expect(access.enforce(actor, {}, () => false)).rejects.toMatchObject({ code: "FORBIDDEN" });
  await expect(
    access
      .memberships(async () => {
        throw new Error("fixture-secret");
      })
      .enforce(actor, {}, () => true),
  ).rejects.toMatchObject({ code: "SERVICE_UNAVAILABLE" });
  const rules = allPolicies<PolicyContext<typeof actor, {}, undefined>>(
    () => true,
    async () => false,
  );
  await expect(access.enforce(actor, {}, rules)).rejects.toMatchObject({ code: "FORBIDDEN" });
  expect(() => allPolicies()).toThrow(AuthConfigurationError);
  await auth.close();
});

test("cancellation, in-flight draining and stopped service references are enforced", async () => {
  let started!: () => void;
  const waiting = new Promise<void>((resolve) => {
    started = resolve;
  });
  const auth = createAuth(
    realm(
      "people",
      defineSource({
        async verify(_token: string, { signal }) {
          started();
          await new Promise<void>((resolve) =>
            signal.addEventListener("abort", () => resolve(), { once: true }),
          );
          signal.throwIfAborted();
          return { status: "verified", subjectId: "alice" };
        },
      }),
    ),
  );
  const access = auth.for(audience("notes:read"));
  const pending = access.required("session");
  const rejected = pending.catch((error: unknown) => error);
  await waiting;
  const firstStop = auth.close();
  expect(auth.close()).toBe(firstStop);
  await firstStop;
  expect(await rejected).toMatchObject({ code: "SERVICE_UNAVAILABLE" });
  await expect(access.required("session")).rejects.toMatchObject({ code: "SERVICE_UNAVAILABLE" });

  const active = createAuth(realm("people", fixture));
  const read = active.for(audience("notes:read"));
  const abort = new AbortController();
  const actor = await read.required("alice", { signal: abort.signal });
  abort.abort(new Error("caller-cancelled"));
  await expect(read.enforce(actor, {}, () => true)).rejects.toThrow("caller-cancelled");
  await active.close();
});

test("getSession bridge preserves uncertain null and strips provider secrets", async () => {
  const provider = sessionSource({
    async getSession({ headers }) {
      return headers.has("cookie") ? { account: { id: "alice" }, token: "fixture-secret" } : null;
    },
    subjectId: (value) => value.account.id,
  });
  const auth = createAuth(realm("people", provider));
  const read = auth.for(audience("notes:read"));
  await expect(read.optional(new Headers())).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  const actor = await read.required(new Headers({ cookie: "fixture-session" }));
  expect(JSON.stringify(actor)).not.toContain("fixture-secret");
  const explicit = createAuth(
    realm(
      "people",
      sessionSource({
        getSession: async () => null,
        subjectId: (_value: { account: { id: string } }) => _value.account.id,
        hasCredential: (headers) => headers.has("cookie"),
      }),
    ),
  );
  expect(await explicit.for(audience("notes:read")).optional(new Headers())).toBeNull();
  await expect(
    explicit.for(audience("notes:read")).optional(new Headers({ cookie: "invalid" })),
  ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  await Promise.all([auth.close(), explicit.close()]);
});

test("real HTTP and direct services share membership and object authorization", async () => {
  const auth = createAuthPlugin({
    id: "auth",
    setup: () => createAuth(realm("people", fixture)),
  });
  const readAudience = audience("notes:read");
  const notes = definePlugin({
    id: "private-notes",
    requires: [auth],
    setup(ctx) {
      const access = ctx
        .get(auth)
        .for(readAudience)
        .memberships(async (subject, note: { tenantId: string }) => {
          const tenant = subject.subjectId === "eve" ? "south" : "north";
          return tenant === note.tenantId ? { role: "member" as const } : null;
        });
      return {
        async read(actor: ActorOf<typeof access> | null, id: string) {
          const note = { id: "one", tenantId: "north", ownerId: "alice", text: "private" };
          await access.enforce(
            actor,
            note,
            ({ principal, resource }) =>
              resource.id === id && principal.subjectId === resource.ownerId,
          );
          return { text: note.text };
        },
      };
    },
  });
  const makeRouter = (ctx: PluginContext) => {
    const access = ctx.get(auth).for(readAudience);
    return {
      who: os
        .$context<WebContext>()
        .use(optionalAuth(access, bearerEvidence))
        .handler(({ context }) => context.actor?.subjectId ?? "anonymous"),
      read: os
        .$context<WebContext>()
        .use(requiredAuth(access, bearerEvidence))
        .input(z.object({ id: z.string() }))
        .handler(({ context, input }) => ctx.get(notes).read(context.actor, input.id)),
    };
  };
  const web = createWebPlugin({
    requires: [auth, notes],
    router: makeRouter,
  });
  const app = await startApp(defineApp({ plugins: [web, auth, notes] }));
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: app.get(web).fetch });
  try {
    const client = (token?: string) =>
      createClient<ReturnType<typeof makeRouter>>(new URL("/rpc", server.url), {
        headers: token ? { authorization: `Bearer ${token}` } : {},
      });
    expect(await client().who()).toBe("anonymous");
    await expect(client().read({ id: "one" })).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    await expect(client("invalid").who()).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    expect(await client("alice").read({ id: "one" })).toEqual({ text: "private" });
    for (const token of ["bob", "eve"]) {
      await expect(client(token).read({ id: "one" })).rejects.toMatchObject({ code: "FORBIDDEN" });
    }
    const access = app.get(auth).for(readAudience);
    const actor = await access.required("alice");
    expect(await app.get(notes).read(actor, "one")).toEqual({ text: "private" });
    await expect(
      app.get(notes).read(JSON.parse(JSON.stringify(actor)), "one"),
    ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  } finally {
    await server.stop(true);
    await app.stop();
  }
});

test("Fetch helpers are explicit, preserve headers and reject origin/credential ambiguity", async () => {
  const request = new Request("https://app.example/notes", {
    method: "POST",
    headers: { origin: "https://app.example", authorization: "Bearer token" },
  });
  expect(bearerEvidence({ request }).evidence).toBe("token");
  expect(headersEvidence({ request }).evidence).not.toBe(request.headers);
  requireSameOrigin(request, "https://app.example");
  expect(() => requireSameOrigin(request, "https://other.example")).toThrow(AuthError);
  expect(() => requireSameOrigin(new Request(request.url), "https://app.example")).toThrow(
    AuthError,
  );
  expect(() =>
    bearerEvidence({
      request: new Request(request.url, {
        headers: { authorization: "Basic credentials", cookie: "session" },
      }),
    }),
  ).toThrow(AuthError);
  expect(authErrorResponse(new AuthError("FORBIDDEN")).status).toBe(403);
  const response = authErrorResponse(new Error("fixture-secret"));
  expect(response.status).toBe(503);
  expect(await response.text()).not.toContain("fixture-secret");
});

test("oRPC preserves downstream errors and safely maps AuthError in both modes", async () => {
  const auth = createAuth(realm("people", fixture));
  const access = auth.for(audience("notes:read"));
  for (const middleware of [
    optionalAuth(access, bearerEvidence),
    requiredAuth(access, bearerEvidence),
  ]) {
    const request = new Request("https://app.example", {
      headers: { authorization: "Bearer alice" },
    });
    const denied = os
      .$context<{ request: Request; signal: AbortSignal }>()
      .use(middleware)
      .handler(() => {
        throw new AuthError("FORBIDDEN");
      });
    await expect(
      call(denied, undefined, { context: { request, signal: request.signal } }),
    ).rejects.toMatchObject({ code: "FORBIDDEN" });
    const businessFailure = new Error("business-failure");
    const broken = os
      .$context<{ request: Request; signal: AbortSignal }>()
      .use(middleware)
      .handler(() => {
        throw businessFailure;
      });
    await expect(
      call(broken, undefined, { context: { request, signal: request.signal } }),
    ).rejects.toBe(businessFailure);
  }
  await auth.close();
});

test("expiration is checked after asynchronous business policy and safe errors cannot leak mutated messages", async () => {
  let now = 1000;
  const auth = createAuth(
    realm(
      "people",
      defineSource({
        async verify() {
          return { status: "verified", subjectId: "alice", session: { expiresAt: 1100 } };
        },
      }),
    ),
    { now: () => now },
  );
  const access = auth.for(audience("notes:read"));
  const actor = await access.required(undefined);
  await expect(
    access.enforce(actor, {}, async () => {
      now = 1100;
      return true;
    }),
  ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  const unsafe = Object.assign(new AuthError("FORBIDDEN"), { message: "fixture-secret" });
  expect(await authErrorResponse(unsafe).json()).toEqual({
    code: "FORBIDDEN",
    message: "Access denied",
  });
  await auth.close();
});

test("source-owned mutable DTOs cannot extend the evidence validated for an operation", async () => {
  let now = 1000;
  const shared = {
    status: "verified" as const,
    subjectId: "alice",
    session: { expiresAt: 1100 },
  };
  const auth = createAuth(
    realm(
      "people",
      defineSource({
        async verify() {
          return shared;
        },
      }),
    ),
    { now: () => now },
  );
  const access = auth.for(audience("notes:read"));
  const actor = await access.required(undefined);
  await expect(
    access.enforce(actor, {}, async () => {
      now = 1100;
      shared.session.expiresAt = 5000;
      shared.subjectId = "bob";
      return true;
    }),
  ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  expect(actor.subjectId).toBe("alice");
  expect(Object.isFrozen(shared)).toBe(false);
  await auth.close();
});

test("reentrant close caches its promise before abort callbacks and drains registered source work", async () => {
  let release!: () => void;
  const deferred = new Promise<void>((resolve) => {
    release = resolve;
  });
  let started!: () => void;
  const ready = new Promise<void>((resolve) => {
    started = resolve;
  });
  let sourceClose!: Promise<void>;
  let listenerClose!: Promise<void>;
  const auth = createAuth(
    realm(
      "people",
      defineSource({
        async verify(_evidence: undefined, { signal }) {
          signal.addEventListener(
            "abort",
            () => {
              listenerClose = auth.close();
            },
            { once: true },
          );
          sourceClose = auth.close();
          started();
          await deferred;
          return { status: "verified", subjectId: "alice" };
        },
      }),
    ),
  );
  const pending = auth
    .for(audience("notes:read"))
    .required(undefined)
    .catch((error: unknown) => error);
  await ready;
  let closed = false;
  void sourceClose.then(() => {
    closed = true;
  });
  await Promise.resolve();
  expect(auth.close()).toBe(sourceClose);
  expect(listenerClose).toBe(sourceClose);
  expect(closed).toBe(false);
  release();
  await sourceClose;
  expect(await pending).toMatchObject({ code: "SERVICE_UNAVAILABLE" });
});

test("malformed runtime requirements fail before verification and coercible error codes are sanitized", async () => {
  const auth = createAuth(realm("people", fixture));
  const access = auth.for(audience("notes:read"));
  for (const invalid of [{ authoritative: "true" }, { assurance: null }, { assurance: "mfa" }]) {
    expect(() => access.requireSession(invalid as unknown as SessionRequirements)).toThrow(
      AuthConfigurationError,
    );
  }
  const poisoned = Object.assign(new AuthError("FORBIDDEN"), {
    code: { secret: "fixture-secret", toString: () => "FORBIDDEN" },
  });
  expect(await authErrorResponse(poisoned).json()).toEqual({
    code: "SERVICE_UNAVAILABLE",
    message: "Authentication unavailable",
  });
  const broken = createAuth(
    realm(
      "broken",
      defineSource({
        async verify(_token: string | null) {
          throw poisoned;
        },
      }),
    ),
  );
  const brokenAccess = broken.for(audience("notes:read"));
  await expect(brokenAccess.required("token")).rejects.toMatchObject({
    code: "SERVICE_UNAVAILABLE",
    message: "Authentication unavailable",
  });
  const procedure = os
    .$context<{ request: Request }>()
    .use(requiredAuth(brokenAccess, bearerEvidence))
    .handler(() => "unreachable");
  await expect(
    call(procedure, undefined, {
      context: {
        request: new Request("https://app.example", { headers: { authorization: "Bearer token" } }),
      },
    }),
  ).rejects.toMatchObject({ code: "SERVICE_UNAVAILABLE" });
  await Promise.all([auth.close(), broken.close()]);
});

test("Fetch evidence combines request cancellation and independent context cancellation", async () => {
  const auth = createAuth(realm("people", fixture));
  const access = auth.for(audience("notes:read"));
  for (const target of ["request", "context"]) {
    const requestAbort = new AbortController();
    const contextAbort = new AbortController();
    const request = new Request("https://app.example", {
      headers: { authorization: "Bearer alice" },
      signal: requestAbort.signal,
    });
    const input = bearerEvidence({ request, signal: contextAbort.signal });
    const actor = await access.required(input.evidence, { signal: input.signal });
    const reason = new Error(`${target} cancelled`);
    (target === "request" ? requestAbort : contextAbort).abort(reason);
    await expect(access.enforce(actor, {}, () => true)).rejects.toBe(reason);
    expect(headersEvidence({ request, signal: contextAbort.signal }).signal?.aborted).toBe(true);
  }
  await auth.close();
});
