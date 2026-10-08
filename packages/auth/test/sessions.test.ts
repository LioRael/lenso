import { expect, test } from "bun:test";
import { AuthError } from "../src/errors";
import { createManagedSessions, sessionLifetime } from "../src/sessions";
import type { SessionMutation, SessionRecord, SessionStore } from "../src/session-store";
import type { AuthSource } from "../src/source";

class MemoryStore<S extends string> implements SessionStore<S> {
  readonly records = new Map<string, SessionRecord<S>>();
  constructor(private readonly now: () => number) {}
  async create(record: SessionRecord<S>) {
    if (this.records.has(record.id)) throw new Error("duplicate");
    this.records.set(record.id, record);
  }
  async read(realmId: string, id: string) {
    const record = this.records.get(id);
    return record?.realmId === realmId ? record : null;
  }
  async mutate(mutation: SessionMutation<S>) {
    const { next, expectedDigest, expectedRevision, now } = mutation;
    const current = this.records.get(next.id);
    const at = Math.max(now, this.now());
    if (
      !current ||
      current.realmId !== next.realmId ||
      current.revokedAt !== null ||
      current.revision !== expectedRevision ||
      current.tokenDigest !== expectedDigest ||
      at >=
        Math.min(
          current.expiresAt,
          next.expiresAt,
          current.lastActiveAt + Math.min(current.idleTimeoutMs, next.idleTimeoutMs),
        ) ||
      (mutation.kind === "renew" && at < current.renewedAt + next.renewAfterMs) ||
      next.revision !== current.revision + 1 ||
      next.expiresAt > current.expiresAt ||
      next.idleTimeoutMs > current.idleTimeoutMs ||
      next.renewAfterMs < current.renewAfterMs
    )
      return false;
    this.records.set(next.id, next);
    return true;
  }
  async revoke(realmId: string, id: string, at: number) {
    const current = this.records.get(id);
    if (!current || current.realmId !== realmId || current.revokedAt !== null) return false;
    this.records.set(id, { ...current, revision: current.revision + 1, revokedAt: at });
    return true;
  }
}

const lifetime = { idle: 100, absolute: 500, renewAfter: 20 };
const context = { signal: new AbortController().signal };

function setup(
  options: {
    realm?: string;
    now?: () => number;
    active?: (subject: string) => boolean;
    login?: AuthSource<{ proof: string }, string>;
  } = {},
) {
  let time = 1_000;
  const now = options.now ?? (() => time);
  const store = new MemoryStore(now);
  const active = new Set(["custom:subject"]);
  const login: AuthSource<{ proof: string }, string> = options.login ?? {
    capabilities: { authenticatedAt: true, assurance: ["password"] },
    async verify(evidence) {
      return evidence.proof === "valid"
        ? {
            status: "verified",
            subjectId: "custom:subject",
            kind: "service",
            session: { expiresAt: 9_999, authenticatedAt: 990, assurance: ["password"] },
          }
        : { status: "rejected" };
    },
  };
  const sessions = createManagedSessions({
    realmId: options.realm ?? "realm-a",
    login,
    store,
    lifetime,
    now,
    subjectActive: async (subject) =>
      options.active ? options.active(subject) : active.has(subject),
  });
  return {
    sessions,
    store,
    active,
    setTime: (value: number) => {
      time = value;
    },
  };
}

test("validates and freezes lifetime configuration", () => {
  const value = { idle: 100, absolute: 200, renewAfter: 10 };
  const result = sessionLifetime(value);
  value.idle = 10;
  expect(result.idle).toBe(100);
  expect(Object.isFrozen(result)).toBe(true);
  for (const invalid of [
    { idle: 0, absolute: 1, renewAfter: 0 },
    { idle: 10, absolute: 10, renewAfter: 10 },
    { idle: 11, absolute: 10, renewAfter: 1 },
    { idle: Number.MAX_SAFE_INTEGER + 1, absolute: 1, renewAfter: 1 },
  ])
    expect(() => sessionLifetime(invalid)).toThrow();
});

test("issues opaque sessions only from verified login evidence; storage contains only a digest", async () => {
  const { sessions, store } = setup();
  await expect(sessions.issue({ proof: "no" })).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  const issued = await sessions.issue({ proof: "valid" });
  expect(issued.sessionId).toMatch(/^[0-9a-f-]{36}$/);
  expect(issued.credential).toMatch(/^[0-9a-f-]{36}\.[A-Za-z0-9_-]{43}$/);
  expect(JSON.stringify([...store.records.values()])).not.toContain(issued.credential);
  const record = store.records.get(issued.sessionId)!;
  expect(record.subjectId).toBe("custom:subject");
  expect(record.tokenDigest).toMatch(/^[0-9a-f]{64}$/);
  expect(record.kind).toBe("service");
  expect(record.authenticatedAt).toBe(990);
  expect(sessions.source.realmId).toBe("realm-a");
  expect(sessions.source.capabilities).toMatchObject({
    authoritative: true,
    sessionCreatedAt: true,
    authenticatedAt: true,
  });
});

test("source verification is read-only, absent is distinct, and invalid credentials reject", async () => {
  const { sessions, store, setTime } = setup();
  expect(await sessions.source.verify(null, context)).toEqual({ status: "absent" });
  const issued = await sessions.issue({ proof: "valid" });
  const before = store.records.get(issued.sessionId);
  expect(await sessions.source.verify(issued.credential, context)).toMatchObject({
    status: "verified",
    subjectId: "custom:subject",
    session: { authoritative: true, sessionCreatedAt: 1_000 },
  });
  expect(store.records.get(issued.sessionId)).toBe(before);
  await expect(sessions.source.verify("not-a-token", context)).rejects.toMatchObject({
    code: "UNAUTHORIZED",
  });
  const otherRealm = setup({ realm: "realm-b" });
  await expect(otherRealm.sessions.source.verify(issued.credential, context)).rejects.toMatchObject(
    { code: "UNAUTHORIZED" },
  );
  setTime(1_100);
  await expect(sessions.source.verify(issued.credential, context)).rejects.toMatchObject({
    code: "UNAUTHORIZED",
  });
});

test("inactive subjects reject every use, but valid possession still permits revocation", async () => {
  const { sessions, active } = setup();
  const issued = await sessions.issue({ proof: "valid" });
  active.delete("custom:subject");
  await expect(sessions.source.verify(issued.credential, context)).rejects.toMatchObject({
    code: "UNAUTHORIZED",
  });
  await expect(sessions.touch(issued.credential)).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  await expect(sessions.renew(issued.credential)).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  await sessions.revoke(issued.credential);
});

test("touch explicitly extends activity without widening frozen limits", async () => {
  const { sessions, store, setTime } = setup();
  const issued = await sessions.issue({ proof: "valid" });
  setTime(1_080);
  await sessions.touch(issued.credential);
  expect((await sessions.source.verify(issued.credential, context)).status).toBe("verified");
  const record = store.records.get(issued.sessionId)!;
  expect(record.lastActiveAt).toBe(1_080);
  expect(record.expiresAt).toBe(1_500);
  setTime(1_110);
  await sessions.touch(issued.credential);
  expect(store.records.get(issued.sessionId)!.lastActiveAt).toBe(1_110);
  setTime(1_500);
  await expect(sessions.source.verify(issued.credential, context)).rejects.toMatchObject({
    code: "UNAUTHORIZED",
  });
});

test("renewal is delayed, rotates credential, and a concurrent stale mutation loses", async () => {
  const { sessions, setTime } = setup();
  const issued = await sessions.issue({ proof: "valid" });
  await expect(sessions.renew(issued.credential)).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  setTime(1_020);
  const results = await Promise.allSettled([
    sessions.renew(issued.credential),
    sessions.renew(issued.credential),
  ]);
  expect(results.filter((r) => r.status === "fulfilled")).toHaveLength(1);
  expect(results.filter((r) => r.status === "rejected")).toHaveLength(1);
  const renewed = results.find((r) => r.status === "fulfilled") as PromiseFulfilledResult<{
    credential: string;
  }>;
  expect(renewed.value.credential).not.toBe(issued.credential);
  await expect(sessions.source.verify(issued.credential, context)).rejects.toMatchObject({
    code: "UNAUTHORIZED",
  });
  expect((await sessions.source.verify(renewed.value.credential, context)).status).toBe("verified");
});

test("cancellation remains cancellation and source/store failures are sanitized", async () => {
  const abort = new AbortController();
  const reason = new Error("caller cancelled");
  const login: AuthSource<{ proof: string }, string> = {
    async verify(_evidence, { signal }) {
      signal.throwIfAborted();
      abort.abort(reason);
      return { status: "verified", subjectId: "custom:subject" };
    },
  };
  const cancelled = setup({ login });
  await expect(cancelled.sessions.issue({ proof: "valid" }, { signal: abort.signal })).rejects.toBe(
    reason,
  );
  const brokenStore = new MemoryStore(() => 1_000);
  brokenStore.read = async () => {
    throw new Error("private backend details");
  };
  const broken = createManagedSessions({
    realmId: "failure",
    login: {
      async verify() {
        return { status: "verified", subjectId: "custom:subject" };
      },
    },
    store: brokenStore,
    lifetime,
    subjectActive: async () => true,
  });
  await expect(
    broken.source.verify(
      "00000000-0000-0000-0000-000000000000.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      context,
    ),
  ).rejects.toEqual(
    expect.objectContaining({ code: "SERVICE_UNAVAILABLE", message: "Authentication unavailable" }),
  );
  expect(new AuthError("UNAUTHORIZED").message).toBe("Authentication required");
});

test("expired login proof, future timestamps and undeclared assurance never mint a session", async () => {
  for (const session of [
    { expiresAt: 999 },
    { expiresAt: 9999, authenticatedAt: 2000 },
    { expiresAt: 9999, sessionCreatedAt: 2000 },
    { expiresAt: 9999, assurance: ["mfa"] },
  ]) {
    const { sessions, store } = setup({
      login: {
        capabilities: { authenticatedAt: true, assurance: ["password"] },
        async verify() {
          return { status: "verified", subjectId: "custom:subject", session };
        },
      },
    });
    await expect(sessions.issue({ proof: "valid" })).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    expect(store.records.size).toBe(0);
  }
  let now = 1000;
  const store = new MemoryStore(() => now);
  const sessions = createManagedSessions({
    realmId: "people",
    login: {
      async verify() {
        return { status: "verified", subjectId: "alice", session: { expiresAt: 1005 } };
      },
    },
    lifetime,
    store,
    now: () => now,
    subjectActive: async () => {
      now = 1010;
      return true;
    },
  });
  await expect(sessions.issue(undefined)).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  expect(store.records.size).toBe(0);
});

test("impossible persisted timelines reject even with the correct token digest", async () => {
  for (const patch of [
    { lastActiveAt: 1400 },
    { issuedAt: 1400, renewedAt: 1400, lastActiveAt: 1400 },
    { authenticatedAt: 1001 },
    { renewedAt: 999 },
  ]) {
    const { sessions, store } = setup();
    const issued = await sessions.issue({ proof: "valid" });
    const record = store.records.get(issued.sessionId)!;
    store.records.set(issued.sessionId, { ...record, ...patch });
    await expect(sessions.source.verify(issued.credential, context)).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
  }
});

test("touch persists tighter ceilings; later configuration cannot expand those frozen bounds", async () => {
  let now = 1000;
  const original = setup({ now: () => now });
  const issued = await original.sessions.issue({ proof: "valid" });
  const make = (nextLifetime: typeof lifetime) =>
    createManagedSessions({
      realmId: "realm-a",
      login: {
        capabilities: { authenticatedAt: true, assurance: ["password"] },
        async verify() {
          return { status: "verified", subjectId: "custom:subject" };
        },
      },
      store: original.store,
      lifetime: nextLifetime,
      subjectActive: async () => true,
      now: () => now,
    });
  const tight = make({ idle: 60, absolute: 300, renewAfter: 30 });
  now = 1040;
  await tight.touch(issued.credential);
  expect(original.store.records.get(issued.sessionId)).toMatchObject({
    expiresAt: 1300,
    idleTimeoutMs: 60,
    renewAfterMs: 30,
  });
  const wide = make({ idle: 200, absolute: 1000, renewAfter: 10 });
  now = 1060;
  const renewed = await wide.renew(issued.credential);
  expect(original.store.records.get(issued.sessionId)).toMatchObject({
    expiresAt: 1300,
    idleTimeoutMs: 60,
    renewAfterMs: 30,
  });
  now = 1120;
  await expect(wide.source.verify(renewed.credential, context)).rejects.toMatchObject({
    code: "UNAUTHORIZED",
  });
});

test("a delayed atomic mutation cannot revive a session past its tightened idle deadline", async () => {
  let now = 1000;
  const original = setup({ now: () => now });
  const issued = await original.sessions.issue({ proof: "valid" });
  const commit = original.store.mutate.bind(original.store);
  original.store.mutate = async (mutation) => {
    now = 1045;
    return commit(mutation);
  };
  const tight = createManagedSessions({
    realmId: "realm-a",
    login: {
      async verify() {
        return { status: "verified", subjectId: "custom:subject" };
      },
    },
    store: original.store,
    lifetime: { idle: 40, absolute: 50, renewAfter: 20 },
    subjectActive: async () => true,
    now: () => now,
  });
  now = 1020;
  await expect(tight.renew(issued.credential)).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  expect(original.store.records.get(issued.sessionId)!.revision).toBe(1);
});

test("revocation preserves post-await cancellation and sanitizes all store errors", async () => {
  for (const changed of [true, false]) {
    const { sessions, store } = setup();
    const issued = await sessions.issue({ proof: "valid" });
    const abort = new AbortController();
    const reason = new Error("caller cancelled");
    store.revoke = async () => {
      abort.abort(reason);
      return changed;
    };
    await expect(sessions.revoke(issued.credential, { signal: abort.signal })).rejects.toBe(reason);
  }
  const { sessions, store } = setup();
  const issued = await sessions.issue({ proof: "valid" });
  store.read = async () => {
    throw Object.assign(new AuthError("SERVICE_UNAVAILABLE"), {
      message: "private backend details",
    });
  };
  await expect(sessions.source.verify(issued.credential, context)).rejects.toMatchObject({
    code: "SERVICE_UNAVAILABLE",
    message: "Authentication unavailable",
  });
});

test("session owner drains login work, stops retained references and snapshots login facts", async () => {
  let started!: () => void;
  const ready = new Promise<void>((resolve) => {
    started = resolve;
  });
  const managed = setup({
    login: {
      async verify(_evidence, { signal }) {
        started();
        await new Promise<void>((resolve) =>
          signal.addEventListener("abort", () => resolve(), { once: true }),
        );
        signal.throwIfAborted();
        return { status: "verified", subjectId: "custom:subject" };
      },
    },
  });
  const pending = managed.sessions.issue({ proof: "valid" }).catch((error: unknown) => error);
  await ready;
  const closed = managed.sessions.close();
  expect(managed.sessions.close()).toBe(closed);
  await closed;
  expect(await pending).toMatchObject({ code: "SERVICE_UNAVAILABLE" });
  await expect(managed.sessions.issue({ proof: "valid" })).rejects.toMatchObject({
    code: "SERVICE_UNAVAILABLE",
  });

  let now = 1000;
  const shared = {
    status: "verified" as const,
    subjectId: "custom:subject",
    session: { expiresAt: 1005 },
  };
  const store = new MemoryStore(() => now);
  const owner = createManagedSessions({
    realmId: "people",
    lifetime,
    store,
    now: () => now,
    login: {
      async verify() {
        return shared;
      },
    },
    subjectActive: async () => {
      now = 1010;
      shared.session.expiresAt = 5000;
      shared.subjectId = "other";
      return true;
    },
  });
  await expect(owner.issue(undefined)).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  expect(store.records.size).toBe(0);
  await owner.close();
});
