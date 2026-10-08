import { expect, test } from "bun:test";
import { ApiKeyError, apiKeyConfig, namespacedSubjectId } from "../src";
import { fixture } from "./fixture";

test("only issue winner sees secret, replay returns safe metadata, storage holds digests only", async () => {
  const { keys, rows, input } = fixture();
  const outcomes = await Promise.all([keys.issue(input, {}), keys.issue(input, {})]);
  const winner = outcomes.find((result) => !result.replayed)!;
  const replay = outcomes.find((result) => result.replayed)!;
  expect(winner.credential).toMatch(/^lk_[^.]+\.[\w-]{43}$/);
  expect(replay.credential).toBeNull();
  expect(replay.key.id).toBe(winner.key.id);
  const row = rows.get(winner.key.id)!;
  expect(JSON.stringify(row)).not.toContain(winner.credential!.split(".")[1]);
  for (const output of [
    winner.key,
    await keys.list({ subject: input.subject }, {}),
    await keys.read({ subject: input.subject, id: winner.key.id }, {}),
    await keys.verify(winner.credential),
  ]) {
    const json = JSON.stringify(output);
    expect(json).not.toContain(row.digest);
    expect(json).not.toContain("digest");
    expect(json).not.toContain("credential");
  }
  await expect(keys.issue({ ...input, requestedScopes: [] }, {})).rejects.toMatchObject({
    code: "CONFLICT",
  });
  await keys.close();
});

test("wrong secrets, malformed credentials, expiry, disable and revoke fail without positive cache", async () => {
  let active = true;
  const { keys, input, subject, advance } = fixture({ subjectActive: () => active });
  const issued = await keys.issue(input, {});
  const bad = `${issued.credential!.split(".")[0]}.${"A".repeat(43)}`;
  for (const value of [bad, "", "lk_not-a-key", null]) expect(await keys.verify(value)).toBeNull();
  expect(await keys.verify(issued.credential)).not.toBeNull();
  active = false;
  expect(await keys.verify(issued.credential)).toBeNull();
  active = true;
  await keys.revoke({ subject, id: issued.key.id }, {});
  expect(await keys.verify(issued.credential)).toBeNull();
  expect(await keys.revoke({ subject, id: issued.key.id }, {})).toBeTrue();
  const second = await keys.issue({ ...input, requestId: "another" }, {});
  advance(30_000);
  expect(await keys.verify(second.credential)).toBeNull();
  await keys.close();
});

test("rotation CAS has one winner, explicit bounded overlap and no secret replay", async () => {
  const { keys, input, subject, advance } = fixture();
  const issued = await keys.issue(input, {});
  const rotate = { subject, id: issued.key.id, expectedRevision: 0, overlapMs: 1_000 };
  const outcomes = await Promise.allSettled([keys.rotate(rotate, {}), keys.rotate(rotate, {})]);
  expect(outcomes.filter((outcome) => outcome.status === "fulfilled")).toHaveLength(1);
  const winner = outcomes.find((outcome) => outcome.status === "fulfilled")!;
  if (winner.status !== "fulfilled") throw new Error("Missing test winner");
  expect(winner.value.key.revision).toBe(1);
  expect(winner.value.key.scopes).toEqual(issued.key.scopes);
  expect(await keys.verify(issued.credential)).not.toBeNull();
  expect(await keys.verify(winner.value.credential)).not.toBeNull();
  await expect(keys.rotate({ ...rotate, expectedRevision: 1 }, {})).rejects.toMatchObject({
    code: "CONFLICT",
  });
  advance(1_000);
  expect(await keys.verify(issued.credential)).toBeNull();
  const next = await keys.rotate({ ...rotate, expectedRevision: 1, overlapMs: 0 }, {});
  expect(await keys.verify(winner.value.credential)).toBeNull();
  expect(await keys.verify(next.credential)).not.toBeNull();
  await expect(keys.rotate(rotate, {})).rejects.toMatchObject({ code: "CONFLICT" });
  await keys.revoke({ subject, id: issued.key.id }, {});
  expect(await keys.verify(next.credential)).toBeNull();
  await keys.close();
});

test("client scopes cannot self-grant; use checks current policy independently from delegation", async () => {
  const caller = {};
  let currentPermission = true;
  let delegation = false;
  const { keys, input } = fixture({
    authorizeManagement: (value) => value === caller && delegation,
    authorizeUse: (_key, _scope, resource) => currentPermission && resource.tenantId === "tenant-a",
  });
  await expect(keys.issue(input, caller)).rejects.toMatchObject({ code: "FORBIDDEN" });
  delegation = true;
  const issued = await keys.issue({ ...input, requestedScopes: ["notes:read", "admin"] }, caller);
  expect(issued.key.scopes).toEqual(["notes:read"]);
  await expect(
    keys.use(issued.credential, "admin", { tenantId: "tenant-a" }),
  ).rejects.toMatchObject({ code: "FORBIDDEN" });
  await keys.use(issued.credential, "notes:read", { tenantId: "tenant-a" });
  currentPermission = false;
  await expect(
    keys.use(issued.credential, "notes:read", { tenantId: "tenant-a" }),
  ).rejects.toMatchObject({ code: "FORBIDDEN" });
  currentPermission = true;
  await expect(
    keys.use(issued.credential, "notes:read", { tenantId: "tenant-b" }),
  ).rejects.toMatchObject({ code: "FORBIDDEN" });
  await expect(keys.issue(input, {})).rejects.toMatchObject({ code: "FORBIDDEN" });
  await keys.close();
});

test("revocation during use policy is observed by fresh verification", async () => {
  const value = fixture({
    authorizeUse: async (key) => {
      await value.keys.revoke({ subject: key.subject, id: key.id }, {});
      return true;
    },
  });
  const issued = await value.keys.issue(value.input, {});
  await expect(
    value.keys.use(issued.credential, "notes:read", { tenantId: "tenant-a" }),
  ).rejects.toMatchObject({ code: "FORBIDDEN" });
  await value.keys.close();
});

test("namespace and tenant collisions do not join subjects or disclose another partition", async () => {
  const { keys, input, subject } = fixture();
  const issued = await keys.issue(input, {});
  for (const other of [
    { ...subject, namespace: "agents" },
    { ...subject, tenantId: "tenant-b" },
    { ...subject, subjectId: "different" },
  ]) {
    expect(namespacedSubjectId(other)).not.toBe(namespacedSubjectId(subject));
    expect(await keys.list({ subject: other }, {})).toEqual([]);
    expect(await keys.read({ subject: other, id: issued.key.id }, {})).toBeNull();
    expect(await keys.revoke({ subject: other, id: issued.key.id }, {})).toBeFalse();
    await expect(
      keys.rotate({ subject: other, id: issued.key.id, expectedRevision: 0, overlapMs: 0 }, {}),
    ).rejects.toMatchObject({ code: "CONFLICT" });
  }
  await keys.close();
});

test("provider errors are opaque and close drains without closing borrowed store", async () => {
  const { keys, input } = fixture({
    authorizeManagement: () => {
      throw new Error("fixture-secret digest SQL parameters");
    },
  });
  try {
    await keys.issue(input, {});
    throw new Error("Unexpected success");
  } catch (error) {
    expect(error).toBeInstanceOf(ApiKeyError);
    expect(String(error)).not.toContain("fixture-secret");
    expect(error).toMatchObject({ code: "UNAVAILABLE" });
  }
  const first = keys.close();
  expect(keys.close()).toBe(first);
  await first;
  await expect(keys.verify(null)).rejects.toMatchObject({ code: "UNAVAILABLE" });
});

test("configuration and overlap ceilings fail closed", () => {
  expect(() => apiKeyConfig({ maxLifetimeMs: 0, maxOverlapMs: 0 })).toThrow(ApiKeyError);
  const { keys, subject } = fixture();
  expect(() =>
    keys.rotate({ subject, id: "test", expectedRevision: 0, overlapMs: 5_001 }, {}),
  ).toThrow(ApiKeyError);
  expect(() => keys.list({ subject, limit: 101 }, {})).toThrow(ApiKeyError);
});

test("revocation during a slow subject status reader is observed before verification returns", async () => {
  let revokeDuringVerify = false;
  const value = fixture({
    async subjectActive(subject) {
      if (revokeDuringVerify) {
        const id = [...value.rows.keys()][0];
        await value.store.revoke(subject, id, Date.now());
      }
      return true;
    },
  });
  const issued = await value.keys.issue(value.input, {});
  revokeDuringVerify = true;
  expect(await value.keys.verify(issued.credential)).toBeNull();
  await value.keys.close();
});

test("a bad trusted grant callback cannot grant unrequested scopes", async () => {
  const { keys, input } = fixture({ grantScopes: () => ["admin"] });
  await expect(keys.issue(input, {})).rejects.toMatchObject({ code: "FORBIDDEN" });
  await keys.close();
});

test("close waits for accepted work but refuses new work", async () => {
  let started!: () => void;
  let release!: () => void;
  const entered = new Promise<void>((resolve) => {
    started = resolve;
  });
  const pending = new Promise<void>((resolve) => {
    release = resolve;
  });
  const value = fixture({
    async subjectActive() {
      started();
      await pending;
      return true;
    },
  });
  const issuing = value.keys.issue(value.input, {});
  await entered;
  let closed = false;
  const stopping = value.keys.close().then(() => {
    closed = true;
  });
  await expect(value.keys.verify(null)).rejects.toMatchObject({ code: "UNAVAILABLE" });
  expect(closed).toBeFalse();
  release();
  const issued = await issuing;
  expect(issued.credential).not.toBeNull();
  await stopping;
  expect(closed).toBeTrue();
});
