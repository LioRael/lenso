import { Database } from "bun:sqlite";
import { expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import { drizzle } from "drizzle-orm/bun-sqlite";
import { audience, createAuth, realm, type ActorOf } from "../src/core";
import { AuthError } from "../src/errors";
import { sqliteSessionStore } from "../src/drizzle/sqlite";
import {
  createManagedSessions,
  createSessionAdministration,
  type SessionAdministrationResource,
  type SessionRevokeAudit,
} from "../src/sessions";
import type { SessionAdminStore, SessionRecord } from "../src/session-store";

const migration = await readFile(
  new URL("../migrations/sqlite/0000_auth_sessions.sql", import.meta.url),
  "utf8",
);

async function fixture(run: (f: Awaited<ReturnType<typeof setup>>) => Promise<void>) {
  const f = await setup();
  try {
    await run(f);
  } finally {
    await f.auth.close();
    await f.otherAuth.close();
    await f.staff.close();
    f.db.close();
  }
}

async function setup() {
  const db = new Database(":memory:");
  db.exec(migration);
  const store = sqliteSessionStore(drizzle(db));
  const state = {
    authenticated: true,
    allowed: true,
    prepareFailed: false,
    completeFailed: false,
    duplicate: false,
  };
  const source = {
    async verify(evidence: string) {
      return state.authenticated && evidence === "staff-credential"
        ? { status: "verified" as const, subjectId: "staff-member" }
        : { status: "rejected" as const };
    },
  };
  const staff = createManagedSessions({
    realmId: "staff",
    login: source,
    store,
    lifetime: { idle: 60_000, absolute: 120_000, renewAfter: 100 },
    subjectActive: async () => state.authenticated,
  });
  const staffSession = await staff.issue("staff-credential");
  const auth = createAuth(realm("staff", staff.source));
  const otherAuth = createAuth(realm("staff", staff.source));
  const access = auth
    .for(audience("sessions:admin"))
    .memberships(async (_subject, _resource: SessionAdministrationResource) => ({
      permitted: state.allowed,
    }));
  const actor = await access.required(staffSession.credential);
  const otherActor = await otherAuth
    .for(audience("sessions:admin"))
    .required(staffSession.credential);
  const wrongAudienceActor = await auth
    .for(audience("sessions:other"))
    .required(staffSession.credential);
  type Principal = ActorOf<typeof access>;
  const intents: {
    input: Parameters<SessionRevokeAudit<Principal, { intentId: string }>["prepare"]>[0];
    principal: Principal;
  }[] = [];
  const outcomes: Parameters<SessionRevokeAudit<Principal, { intentId: string }>["complete"]>[1][] =
    [];
  let afterPrepare = async () => {};
  let afterPolicy = async (_resource: SessionAdministrationResource) => {};
  // Fault-injection port proves Auth's protocol, not Audit provider durability.
  const audit: SessionRevokeAudit<Principal, { intentId: string }> = {
    async prepare(input, principal) {
      if (state.prepareFailed) throw new Error("audit intent storage failed");
      if (state.duplicate) return { status: "already-recorded", intentId: input.id };
      intents.push({ input, principal });
      await afterPrepare();
      return { status: "ready", receipt: { intentId: input.id } };
    },
    async complete(_receipt, outcome) {
      if (state.completeFailed) throw new Error("audit outcome unknown");
      outcomes.push(outcome);
    },
  };
  const make = (targetRealm = "customers", persistence: SessionAdminStore = store) =>
    createSessionAdministration({
      realmId: targetRealm,
      store: persistence,
      access,
      policy: async ({ principal, resource, membership }) => {
        await afterPolicy(resource);
        return (
          principal.realmId === "staff" &&
          principal.subjectId === "staff-member" &&
          membership.permitted &&
          resource.realmId === targetRealm
        );
      },
      audit,
      auditScope: { tenantId: null, scopeId: `sessions:${targetRealm}` },
    });
  const base: SessionRecord = {
    id: "a",
    realmId: "customers",
    subjectId: "customer",
    kind: "user",
    tokenDigest: "private-digest-a",
    revision: 1,
    issuedAt: Date.now() - 1000,
    expiresAt: Date.now() + 60_000,
    idleTimeoutMs: 60_000,
    renewAfterMs: 100,
    lastActiveAt: Date.now() - 500,
    renewedAt: Date.now() - 1000,
    authenticatedAt: null,
    assurance: [],
    revokedAt: null,
  };
  for (const row of [
    base,
    { ...base, id: "b", tokenDigest: "private-digest-b" },
    { ...base, id: "c", issuedAt: base.issuedAt - 1, tokenDigest: "private-digest-c" },
    { ...base, realmId: "other-customers", tokenDigest: "private-digest-other" },
  ])
    await store.create(row);
  return {
    db,
    auth,
    otherAuth,
    actor,
    otherActor,
    wrongAudienceActor,
    staff,
    staffSession,
    store,
    access,
    audit,
    state,
    intents,
    outcomes,
    make,
    service: make(),
    setAfterPrepare(value: typeof afterPrepare) {
      afterPrepare = value;
    },
    setAfterPolicy(value: typeof afterPolicy) {
      afterPolicy = value;
    },
  };
}

test("staff Access authorizes customer targets; pages are bounded, exact-instance/realm and credential-free", () =>
  fixture(async ({ service, make, actor }) => {
    const first = await service.list({ limit: 2 }, actor);
    expect(first.sessions.map((row) => row.id)).toEqual(["b", "a"]);
    expect(first.nextCursor).not.toBeNull();
    const second = await service.list({ limit: 2, cursor: first.nextCursor! }, actor);
    expect(second.sessions.map((row) => row.id)).toEqual(["c"]);
    expect(second.nextCursor).toBeNull();
    expect(JSON.stringify(first)).not.toContain("digest");
    expect(JSON.stringify(first)).not.toContain("credential");
    expect(await service.get({ id: "missing" }, actor)).toBeNull();
    await expect(make().list({ cursor: first.nextCursor! }, actor)).rejects.toMatchObject({
      code: "invalid-input",
    });
    await expect(
      make("other-customers").list({ cursor: first.nextCursor! }, actor),
    ).rejects.toMatchObject({ code: "invalid-input" });
    await expect(service.list({ limit: 101 }, actor)).rejects.toMatchObject({
      code: "invalid-input",
    });
    await expect(service.list({ cursor: "invalid" }, actor)).rejects.toMatchObject({
      code: "invalid-input",
    });
    await expect(service.list({ realmId: "other-customers" } as {}, actor)).rejects.toMatchObject({
      code: "invalid-input",
    });
  }));

test("denied, revoked, copied and other-instance actors cannot enumerate or revoke", () =>
  fixture(
    async ({
      service,
      actor,
      otherActor,
      wrongAudienceActor,
      staff,
      staffSession,
      state,
      intents,
      store,
    }) => {
      for (const caller of [null, { ...actor }, otherActor, wrongAudienceActor]) {
        await expect(service.list({}, caller as typeof actor)).rejects.toBeInstanceOf(AuthError);
        await expect(
          service.revoke({ id: "a", expectedRevision: 1 }, caller as typeof actor),
        ).rejects.toBeInstanceOf(AuthError);
      }
      state.allowed = false;
      await expect(service.get({ id: "a" }, actor)).rejects.toMatchObject({ code: "FORBIDDEN" });
      state.allowed = true;
      await staff.revoke(staffSession.credential);
      await expect(service.list({}, actor)).rejects.toMatchObject({ code: "UNAUTHORIZED" });
      expect(intents).toHaveLength(0);
      expect((await store.read("customers", "a"))?.revokedAt).toBeNull();
    },
  ));

test("targeted revoke has independent permission and denied scope cannot probe existence", () =>
  fixture(async ({ actor, store, access, audit }) => {
    let reads = 0;
    const administration = createSessionAdministration({
      realmId: "customers",
      store: {
        ...store,
        async read(r, id) {
          reads++;
          return store.read(r, id);
        },
      },
      access,
      policy: ({ principal, resource, membership }) =>
        principal.subjectId === "staff-member" &&
        membership.permitted &&
        resource.realmId === "customers" &&
        (resource.operation === "scope"
          ? resource.action === "revoke" && resource.sessionId === "a"
          : resource.operation === "revoke" && resource.session.subjectId === "customer"),
      audit,
      auditScope: { tenantId: null, scopeId: "customers-auth:sessions" },
    });
    await expect(administration.list({}, actor)).rejects.toMatchObject({ code: "FORBIDDEN" });
    for (const id of ["b", "missing"]) {
      await expect(administration.get({ id }, actor)).rejects.toMatchObject({ code: "FORBIDDEN" });
      await expect(administration.revoke({ id, expectedRevision: 1 }, actor)).rejects.toMatchObject(
        { code: "FORBIDDEN" },
      );
    }
    expect(reads).toBe(0);
    expect(await administration.revoke({ id: "a", expectedRevision: 1 }, actor)).toMatchObject({
      revoked: true,
    });
    expect(reads).toBeGreaterThan(0);
  }));

test("detail rechecks target state after asynchronous authorization and filters extra provider properties", () =>
  fixture(async ({ service, actor, store, make, setAfterPolicy }) => {
    const wrapped = {
      ...store,
      async read(r: string, id: string) {
        const row = await store.read(r, id);
        return row ? { ...row, credential: "must-never-escape", extra: "must-never-escape" } : null;
      },
    };
    expect(JSON.stringify(await make("customers", wrapped).get({ id: "a" }, actor))).not.toContain(
      "must-never-escape",
    );
    setAfterPolicy(async (resource) => {
      if (resource.operation === "get")
        await store.revokeRevision("customers", resource.session.id, 1, Date.now());
    });
    await expect(service.get({ id: "a" }, actor)).rejects.toMatchObject({ code: "stale-revision" });
    const malformed = {
      ...store,
      async read(r: string, id: string) {
        const row = await store.read(r, id);
        return row ? { ...row, realmId: "wrong-realm" } : null;
      },
    };
    await expect(make("customers", malformed).get({ id: "b" }, actor)).rejects.toMatchObject({
      code: "SERVICE_UNAVAILABLE",
    });
  }));

test("admin revoke checks revision, increments it once, and records real caller/target and outcome", () =>
  fixture(async ({ service, actor, store, intents, outcomes }) => {
    await expect(service.revoke({ id: "a", expectedRevision: 2 }, actor)).rejects.toMatchObject({
      code: "stale-revision",
    });
    const result = await service.revoke({ id: "a", expectedRevision: 1 }, actor);
    expect(result.revoked).toBe(true);
    expect((await store.read("customers", "a"))?.revision).toBe(2);
    expect((await store.read("other-customers", "a"))?.revokedAt).toBeNull();
    expect(intents[0]?.principal).toBe(actor);
    expect(intents[0]?.input).toMatchObject({
      action: "auth.session.revoke",
      target: { type: "auth-session", id: "a" },
      scope: { tenantId: null, scopeId: "sessions:customers" },
    });
    expect(outcomes[0]).toMatchObject({ result: "success", reasonCode: "revoked" });
    await expect(service.revoke({ id: "a", expectedRevision: 1 }, actor)).rejects.toMatchObject({
      code: "stale-revision",
    });
    expect(intents).toHaveLength(1);
  }));

test("strict audit failure and duplicate intent prevent effects; outcome failure does not roll back revoke", () =>
  fixture(async ({ service, actor, store, state }) => {
    state.prepareFailed = true;
    await expect(service.revoke({ id: "a", expectedRevision: 1 }, actor)).rejects.toThrow(
      "audit intent storage failed",
    );
    expect((await store.read("customers", "a"))?.revokedAt).toBeNull();
    state.prepareFailed = false;
    state.duplicate = true;
    await expect(service.revoke({ id: "a", expectedRevision: 1 }, actor)).rejects.toMatchObject({
      code: "pending-reconciliation",
    });
    expect((await store.read("customers", "a"))?.revokedAt).toBeNull();
    state.duplicate = false;
    state.completeFailed = true;
    await expect(service.revoke({ id: "a", expectedRevision: 1 }, actor)).rejects.toThrow(
      "audit outcome unknown",
    );
    expect((await store.read("customers", "a"))?.revokedAt).not.toBeNull();
  }));

test("authorization changes and revision races after Audit.prepare prevent writes with meaningful outcomes", () =>
  fixture(async ({ service, actor, store, state, outcomes, setAfterPrepare }) => {
    setAfterPrepare(async () => {
      state.allowed = false;
    });
    await expect(service.revoke({ id: "a", expectedRevision: 1 }, actor)).rejects.toMatchObject({
      code: "FORBIDDEN",
    });
    expect((await store.read("customers", "a"))?.revokedAt).toBeNull();
    expect(outcomes[0]).toMatchObject({ result: "denied", reasonCode: "authorization-changed" });
    state.allowed = true;
    setAfterPrepare(async () => {
      await store.revokeRevision("customers", "b", 1, Date.now());
    });
    await expect(service.revoke({ id: "b", expectedRevision: 1 }, actor)).rejects.toMatchObject({
      code: "stale-revision",
    });
    expect(outcomes[1]).toMatchObject({ result: "failure", reasonCode: "stale-revision" });
  }));

test("a lost store acknowledgement records unknown, not failure or rollback", () =>
  fixture(async ({ make, actor, store, outcomes }) => {
    const uncertain = {
      ...store,
      async revokeRevision(...args: Parameters<typeof store.revokeRevision>) {
        await store.revokeRevision(...args);
        throw new Error("lost acknowledgement");
      },
    };
    await expect(
      make("customers", uncertain).revoke({ id: "a", expectedRevision: 1 }, actor),
    ).rejects.toMatchObject({ code: "outcome-unknown" });
    expect((await store.read("customers", "a"))?.revokedAt).not.toBeNull();
    expect(outcomes[0]).toMatchObject({ result: "unknown", reasonCode: "write-unconfirmed" });
  }));

test("cancellation after intent acknowledgement completes a no-effect outcome", () =>
  fixture(async ({ service, actor, store, outcomes, setAfterPrepare }) => {
    const controller = new AbortController();
    const reason = new Error("cancelled");
    setAfterPrepare(async () => {
      controller.abort(reason);
    });
    await expect(
      service.revoke({ id: "a", expectedRevision: 1 }, actor, {
        signal: controller.signal,
      }),
    ).rejects.toBe(reason);
    expect((await store.read("customers", "a"))?.revokedAt).toBeNull();
    expect(outcomes[0]).toMatchObject({ result: "failure", reasonCode: "request-cancelled" });
  }));
