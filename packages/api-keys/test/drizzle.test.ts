import { Database } from "bun:sqlite";
import { expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import { drizzle } from "drizzle-orm/bun-sqlite";
import type { ApiKeyStore, KeyRecord, KeyRotation, KeySubject } from "../src/store";
import { sqliteApiKeyStore } from "../src/drizzle/sqlite";
import { createApiKeys } from "../src";

export const subject: KeySubject = {
  namespace: "automation",
  tenantId: "tenant",
  subjectId: "alice",
};

export function record(id: string, changes: Partial<KeyRecord> = {}): KeyRecord {
  return {
    id,
    subject,
    requestId: `request-${id}`,
    digest: `opaque-${id}`,
    previousDigest: null,
    scopes: ["notes:read"],
    revision: 1,
    issuedAt: Date.now(),
    expiresAt: Date.now() + 60_000,
    revokedAt: null,
    overlapUntil: null,
    ...changes,
  };
}

export function rotation(row: KeyRecord, changes: Partial<KeyRotation> = {}): KeyRotation {
  return {
    subject: row.subject,
    id: row.id,
    expectedRevision: row.revision,
    digest: `successor-${row.id}`,
    overlapMs: 0,
    now: Date.now(),
    ...changes,
  };
}

export async function storeContract(store: ApiKeyStore, second = store) {
  const original = record("a");
  expect(await store.create(original)).toEqual({ created: true, record: original });
  expect(await store.read("a")).toEqual(original);
  expect(await store.read("absent")).toBeNull();
  expect(
    await store.create(
      record("replay", {
        requestId: original.requestId,
        expiresAt: 0,
        scopes: ["different"],
      }),
    ),
  ).toEqual({ created: false, record: original });
  const replayRace = await Promise.all([
    store.create(record("replay-a", { requestId: "same-request" })),
    second.create(record("replay-b", { requestId: "same-request" })),
  ]);
  expect(replayRace.filter((result) => result.created)).toHaveLength(1);
  expect(replayRace[0]!.record).toEqual(replayRace[1]!.record);
  for (const collision of [
    record("a", { requestId: "new-request" }),
    record("other-id", { digest: original.digest }),
    record("expired", { expiresAt: Date.now() - 1 }),
  ]) {
    await expect(store.create(collision)).rejects.toThrow("Key storage operation failed");
  }
  expect(await store.read("expired")).toBeNull();
  const variants = [
    { ...subject, namespace: "other" },
    { ...subject, tenantId: "other" },
    { ...subject, subjectId: "other" },
  ];
  for (const [index, other] of variants.entries()) {
    await store.create(record(`collision-${index}`, { subject: other }));
    expect(await store.list(other, null, 100)).toHaveLength(1);
    expect(await store.rotate(rotation(original, { subject: other }))).toBeNull();
    expect(await store.revoke(other, original.id, Date.now())).toBe(false);
  }
  // Request IDs are partitioned by namespace and tenant, not by subject.
  for (const [index, other] of variants.slice(0, 2).entries()) {
    expect(
      (
        await store.create(
          record(`partition-${index}`, {
            subject: other,
            requestId: original.requestId,
          }),
        )
      ).created,
    ).toBe(true);
  }
  expect(
    (
      await store.create(
        record("subject-replay", {
          subject: variants[2]!,
          requestId: original.requestId,
        }),
      )
    ).record,
  ).toEqual(original);
  await store.create(record("b"));
  await store.create(record("c"));
  expect((await store.list(subject, null, 2)).map((row) => row.id)).toEqual(["a", "b"]);
  expect((await store.list(subject, "b", 1)).map((row) => row.id)).toEqual(["c"]);
  for (const limit of [0, -1, 101, 1.5, Infinity]) {
    await expect(store.list(subject, null, limit)).rejects.toThrow("Key storage operation failed");
  }
  const competing = await Promise.all([
    store.rotate(rotation(original, { digest: "winner-a", overlapMs: 1_000 })),
    second.rotate(rotation(original, { digest: "winner-b", overlapMs: 1_000 })),
  ]);
  expect(competing.filter(Boolean)).toHaveLength(1);
  const winner = competing.find((row) => row !== null)!;
  expect(winner.scopes).toEqual(original.scopes);
  expect(winner.id).toBe(original.id);
  expect(winner.revision).toBe(2);
  expect(winner.previousDigest).toBe(original.digest);
  expect(winner.overlapUntil).toBeGreaterThan(Date.now());
  expect(winner.overlapUntil).toBeLessThanOrEqual(winner.expiresAt);
  expect(await store.rotate(rotation(winner, { now: winner.overlapUntil! - 1 }))).toBeNull();
  const next = await store.rotate(
    rotation(winner, { now: winner.overlapUntil!, digest: "final-a" }),
  );
  expect(next?.revision).toBe(3);
  expect(next?.previousDigest).toBeNull();
  expect(next?.overlapUntil).toBeNull();
  expect(next?.scopes).toEqual(original.scopes);
  // A zero-overlap successor never retains the previous credential.
  const expired = record("future-expired");
  await store.create(expired);
  expect(await store.rotate(rotation(expired, { now: expired.expiresAt }))).toBeNull();
  const elapsed = record("database-expired", { expiresAt: Date.now() + 500 });
  await store.create(elapsed);
  await Bun.sleep(Math.max(0, elapsed.expiresAt - Date.now()) + 10);
  expect(await store.rotate(rotation(elapsed, { now: elapsed.issuedAt }))).toBeNull();
  expect(await store.read(elapsed.id)).toEqual(elapsed);
  const capped = record("capped");
  await store.create(capped);
  expect((await store.rotate(rotation(capped, { overlapMs: 120_000 })))?.overlapUntil).toBe(
    capped.expiresAt,
  );
  expect(await store.revoke(subject, original.id, Date.now())).toBe(true);
  const revoked = await store.read(original.id);
  expect(revoked?.revokedAt).toBeNumber();
  expect(revoked?.previousDigest).toBeNull();
  expect(await second.revoke(subject, original.id, Date.now() + 1_000)).toBe(true);
  expect(await store.read(original.id)).toEqual(revoked);
  expect(await store.rotate(rotation(next!))).toBeNull();
  expect(await store.revoke(subject, "missing", Date.now())).toBe(false);
  const race = record("revoke-race");
  await store.create(race);
  await Promise.all([
    store.rotate(rotation(race, { overlapMs: 1_000 })),
    second.revoke(subject, race.id, Date.now()),
  ]);
  expect((await store.read(race.id))?.revokedAt).toBeNumber();
  expect((await store.read(race.id))?.previousDigest).toBeNull();
  expect(await store.rotate(rotation((await store.read(race.id))!))).toBeNull();
  const duplicate = record("duplicate-rotation");
  await store.create(duplicate);
  await expect(
    store.rotate(rotation(duplicate, { digest: (await store.read("b"))!.digest })),
  ).rejects.toThrow("Key storage operation failed");
  expect(await store.read(duplicate.id)).toEqual(duplicate);
  await credentialContract(store, second);
}

async function credentialContract(store: ApiKeyStore, second: ApiKeyStore) {
  const caller = {};
  let now = Date.now();
  let permission = true;
  const options = {
    config: { maxLifetimeMs: 60_000, maxOverlapMs: 5_000 },
    authorizeManagement: (value: object) => value === caller,
    grantScopes: (_caller: object, _subject: KeySubject, requested: readonly string[]) =>
      requested.filter((scope) => scope === "notes:read"),
    subjectActive: () => true,
    authorizeUse: (_key: unknown, _scope: string, resource: { tenantId: string }) =>
      permission && resource.tenantId === subject.tenantId,
    now: () => now,
  };
  const first = createApiKeys({ ...options, store });
  const other = createApiKeys({ ...options, store: second });
  const input = {
    subject,
    requestedScopes: ["notes:read", "admin"],
    requestId: "real-service-credential",
    expiresAt: now + 30_000,
  };
  try {
    const results = await Promise.all([first.issue(input, caller), other.issue(input, caller)]);
    const issued = results.find((result) => result.credential !== null)!;
    expect(results.filter((result) => result.credential !== null)).toHaveLength(1);
    expect(results.find((result) => result.replayed)?.credential).toBeNull();
    expect(issued.key.scopes).toEqual(["notes:read"]);
    const stored = await store.read(issued.key.id);
    expect(JSON.stringify(stored)).not.toContain(issued.credential!.split(".")[1]);
    expect(await first.verify(`${issued.credential!.split(".")[0]}.${"A".repeat(43)}`)).toBeNull();
    await expect(
      first.use(issued.credential, "admin", { tenantId: subject.tenantId }),
    ).rejects.toMatchObject({ code: "FORBIDDEN" });
    await first.use(issued.credential, "notes:read", { tenantId: subject.tenantId });
    permission = false;
    await expect(
      first.use(issued.credential, "notes:read", { tenantId: subject.tenantId }),
    ).rejects.toMatchObject({ code: "FORBIDDEN" });
    permission = true;
    const rotationInput = {
      subject,
      id: issued.key.id,
      expectedRevision: 0,
      overlapMs: 1_000,
    };
    const rotations = await Promise.allSettled([
      first.rotate(rotationInput, caller),
      other.rotate(rotationInput, caller),
    ]);
    expect(rotations.filter((result) => result.status === "fulfilled")).toHaveLength(1);
    const winner = rotations.find((result) => result.status === "fulfilled")!;
    if (winner.status !== "fulfilled") throw new Error("Missing credential rotation winner");
    expect(await other.verify(issued.credential)).not.toBeNull();
    expect(await first.verify(winner.value.credential)).not.toBeNull();
    now = winner.value.key.overlapUntil!;
    expect(await first.verify(issued.credential)).toBeNull();
    const successor = await other.rotate(
      {
        ...rotationInput,
        expectedRevision: 1,
        overlapMs: 0,
      },
      caller,
    );
    expect(await first.verify(winner.value.credential)).toBeNull();
    expect(await first.verify(successor.credential)).not.toBeNull();
    await first.revoke({ subject, id: issued.key.id }, caller);
    expect(await other.verify(successor.credential)).toBeNull();
    const expires = await first.issue({ ...input, requestId: "real-service-expiry" }, caller);
    now = expires.key.expiresAt;
    expect(await other.verify(expires.credential)).toBeNull();
  } finally {
    await first.close();
    await other.close();
  }
}

test("real Bun SQLite: native API key store contract", async () => {
  const client = new Database(":memory:");
  try {
    client.exec(
      await readFile(new URL("../migrations/sqlite/0000_api_keys.sql", import.meta.url), "utf8"),
    );
    await storeContract(sqliteApiKeyStore(drizzle(client)));
  } finally {
    client.close();
  }
});
