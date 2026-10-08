import { expect, test } from "bun:test";
import { audience, createAuth, defineSource, realm, type Actor } from "@lenso/auth";
import { z } from "zod";
import { createAuthorizedNotificationService, type NotificationResource } from "../src/auth";
import type { NotificationRecord, NotificationStore, Preference } from "../src/contracts";
import { createNotificationService } from "../src/service";

/** Local fake trusted source and store, not a backend integration test. */
export function authFixture() {
  let revoked = false;
  let now = 100;
  let verifications = 0;
  const source = defineSource<string>({
    async verify(evidence) {
      verifications++;
      if (revoked) return { status: "rejected" };
      return { status: "verified", subjectId: evidence, session: { expiresAt: 1000 } };
    },
  });
  const auth = createAuth(realm("local", source), { now: () => now });
  const access = auth.for(audience("notifications"));
  const records = new Map<string, NotificationRecord>();
  const preferences = new Map<string, Preference>();
  const store: NotificationStore = {
    async insertOrGet(record) {
      records.set(record.id, record);
      return record;
    },
    async findKey(tenant, scope, key) {
      return (
        [...records.values()].find(
          (r) => r.tenantId === tenant && r.scope === scope && r.idempotencyKey === key,
        ) ?? null
      );
    },
    async get(id) {
      return records.get(id) ?? null;
    },
    async list(filter) {
      return [...records.values()]
        .filter((r) => r.tenantId === filter.tenantId && r.recipientId === filter.recipientId)
        .slice(0, filter.limit);
    },
    async save(record, revision) {
      if (records.get(record.id)?.revision !== revision) return false;
      records.set(record.id, record);
      return true;
    },
    async attempts(id) {
      return [
        {
          id: "private-attempt",
          notificationId: id,
          number: 1,
          state: "failed",
          startedAt: 100,
          finishedAt: 101,
          providerMessageId: "private-provider",
          error: "rejected",
        },
      ];
    },
    async getPreference(key) {
      return preferences.get(JSON.stringify(key)) ?? null;
    },
    async setPreference(preference) {
      const { enabled, ...key } = preference;
      preferences.set(JSON.stringify(key), { ...key, enabled });
    },
    async markEnqueued(id, jobId) {
      const record = records.get(id);
      if (!record || (record.taskJobId !== null && record.taskJobId !== jobId)) return false;
      record.taskJobId = jobId;
      return true;
    },
    async recoverable() {
      return [];
    },
  };
  let sends = 0;
  const service = createNotificationService({
    store,
    templates: [
      {
        id: "test",
        version: "v1",
        category: "news",
        necessity: "required",
        channels: ["email"],
        from: "sender@example.test",
        variables: z.strictObject({}),
        subject: "Private subject",
        text: "Private message",
      },
    ],
    channels: [
      {
        id: "email",
        kind: "email",
        idempotencyWindowMs: 86_400_000,
        async send() {
          sends++;
          return { state: "failed", code: "rejected", retryable: true };
        },
      },
    ],
    clock: () => now,
  });
  const queued: string[] = [];
  const tenantFor = async (actor: Actor) => (actor.subjectId === "outsider" ? "other" : "tenant");
  const requeue = async (id: string) => {
    queued.push(id);
    return true;
  };
  const managePolicy = ({
    principal,
    resource,
  }: {
    principal: Actor;
    resource: NotificationResource;
  }) => principal.subjectId === "admin" && resource.scope === "billing";
  const authorized = createAuthorizedNotificationService({ service, access, tenantFor });
  const managed = createAuthorizedNotificationService({
    service,
    access,
    tenantFor,
    managePolicy,
    requeue,
  });
  async function create(recipientId = "alice", tenantId = "tenant") {
    return service.create({
      tenantId,
      recipientId,
      scope: "billing",
      businessId: "private-business",
      idempotencyKey: crypto.randomUUID(),
      email: "recipient@example.test",
      templateId: "test",
      templateVersion: "v1",
      variables: {},
    });
  }
  return {
    auth,
    access,
    service,
    records,
    preferences,
    authorized,
    managed,
    create,
    tenantFor,
    managePolicy,
    requeue,
    queued,
    revoke: () => {
      revoked = true;
    },
    expire: () => {
      now = 1000;
    },
    verifications: () => verifications,
    sends: () => sends,
  };
}

test("genuine actors are reverified on every read; forged, foreign audience/instance, revoked and expired fail", async () => {
  const f = authFixture();
  const foreign = authFixture();
  try {
    const row = await f.create();
    const actor = await f.access.required("alice");
    const before = f.verifications();
    await f.authorized.query({ id: row.id }, actor);
    await f.authorized.query({ id: row.id }, actor);
    expect(f.verifications()).toBe(before + 2);
    const wrong = await f.auth.for(audience("other")).required("alice");
    const alien = await foreign.access.required("alice");
    for (const forged of [{ ...actor }, wrong, alien]) {
      await expect(
        f.authorized.query({ id: row.id }, forged as typeof actor),
      ).rejects.toMatchObject({ code: "access-denied" });
    }
    f.expire();
    await expect(f.authorized.query({ id: row.id }, actor)).rejects.toMatchObject({
      code: "access-denied",
    });
  } finally {
    await f.auth.close();
    await foreign.auth.close();
  }
  const revoked = authFixture();
  try {
    const row = await revoked.create();
    const actor = await revoked.access.required("alice");
    revoked.revoke();
    await expect(revoked.authorized.query({ id: row.id }, actor)).rejects.toMatchObject({
      code: "access-denied",
    });
  } finally {
    await revoked.auth.close();
  }
});

test("ownership, tenant and missing records share denial; list and preferences bind current subject", async () => {
  const f = authFixture();
  try {
    const own = await f.create();
    const otherOwner = await f.create("bob");
    const otherTenant = await f.create("alice", "other");
    const actor = await f.access.required("alice");
    for (const id of [otherOwner.id, otherTenant.id, "missing"]) {
      await expect(f.authorized.query({ id }, actor)).rejects.toMatchObject({
        code: "access-denied",
      });
    }
    const listed = await f.authorized.list({ tenantId: "tenant", limit: 10 }, actor);
    expect(listed.map((row) => row.id)).toEqual([own.id]);
    expect(Object.keys(listed[0]!)).toEqual([
      "id",
      "state",
      "attemptCount",
      "retryable",
      "createdAt",
      "updatedAt",
    ]);
    await expect(f.authorized.list({ tenantId: "other", limit: 10 }, actor)).rejects.toMatchObject({
      code: "access-denied",
    });
    const key = { tenantId: "tenant", category: "news", channelId: "email" };
    await f.authorized.setPreference({ ...key, enabled: true }, actor);
    expect(await f.authorized.getPreference(key, actor)).toEqual({
      category: "news",
      channelId: "email",
      enabled: true,
    });
    const bob = await f.access.required("bob");
    expect((await f.authorized.getPreference(key, bob)).enabled).toBe(false);
    await expect(
      f.authorized.getPreference({ ...key, tenantId: "other" }, actor),
    ).rejects.toMatchObject({ code: "access-denied" });
    await expect(
      f.authorized.setPreference({ ...key, tenantId: "other", enabled: true }, actor),
    ).rejects.toMatchObject({ code: "access-denied" });
    expect(f.authorized).not.toHaveProperty("close");
    f.revoke();
    await expect(f.authorized.list({ tenantId: "tenant", limit: 10 }, actor)).rejects.toMatchObject(
      { code: "access-denied" },
    );
    await expect(f.authorized.getPreference(key, actor)).rejects.toMatchObject({
      code: "access-denied",
    });
    await expect(
      f.authorized.setPreference({ ...key, enabled: false }, actor),
    ).rejects.toMatchObject({ code: "access-denied" });
  } finally {
    await f.auth.close();
  }
});

test("management defaults deny; explicit scoped admin policy requeues only retryable failures and redacts attempts", async () => {
  const f = authFixture();
  try {
    const row = await f.create();
    const admin = await f.access.required("admin");
    const alice = await f.access.required("alice");
    for (const method of ["adminQuery", "attempts", "retry"] as const) {
      await expect(f.authorized[method]({ id: row.id }, admin)).rejects.toMatchObject({
        code: "access-denied",
      });
      await expect(f.managed[method]({ id: row.id }, alice)).rejects.toMatchObject({
        code: "access-denied",
      });
    }
    expect(await f.managed.attempts({ id: row.id }, admin)).toEqual([
      { number: 1, state: "failed", startedAt: 100, finishedAt: 101 },
    ]);
    const record = f.records.get(row.id)!;
    for (const state of ["accepted", "delivered", "suppressed", "pending", "sending"] as const) {
      record.state = state;
      await expect(f.managed.retry({ id: row.id }, admin)).rejects.toMatchObject({
        code: "delivery-retry",
      });
    }
    record.state = "failed";
    record.retryable = false;
    await expect(f.managed.retry({ id: row.id }, admin)).rejects.toMatchObject({
      code: "delivery-retry",
    });
    record.retryable = true;
    expect(await f.managed.retry({ id: row.id }, admin)).toEqual({ queued: true });
    record.state = "sending";
    record.leaseUntil = 101;
    await expect(f.managed.retry({ id: row.id }, admin)).rejects.toMatchObject({
      code: "delivery-retry",
    });
    record.leaseUntil = 99;
    expect(await f.managed.retry({ id: row.id }, admin)).toEqual({ queued: true });
    expect(f.queued).toEqual([row.id, row.id]);
    expect(f.sends()).toBe(0);
    const cross = await f.create("alice", "other");
    await expect(f.managed.retry({ id: cross.id }, admin)).rejects.toMatchObject({
      code: "access-denied",
    });
    await expect(f.managed.adminQuery({ id: "missing" }, admin)).rejects.toMatchObject({
      code: "access-denied",
    });
    record.scope = "unmanaged";
    await expect(f.managed.adminQuery({ id: row.id }, admin)).rejects.toMatchObject({
      code: "access-denied",
    });
  } finally {
    await f.auth.close();
  }
});
