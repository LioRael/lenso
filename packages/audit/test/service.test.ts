import { expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { audience, createAuth, realm } from "@lenso/auth";
import { drizzle } from "drizzle-orm/bun-sqlite";
import { createAuthAuditAuthority } from "../src/auth";
import {
  AuditError,
  AuditOutcomeUnknownError,
  createAuditService,
  type AuditDiagnostic,
  type AuditInput,
  type AuditRepository,
  type AuditScope,
} from "../src/index";
import { createSqliteAuditRepository } from "../src/sqlite";
import { createAuditReporter } from "../src/diagnostics";

const scope: AuditScope = { tenantId: "tenant-a", scopeId: "owner:alice" };
const input = (overrides: Partial<AuditInput> = {}): AuditInput => ({
  id: crypto.randomUUID(),
  occurredAt: 100,
  scope,
  action: "note.remove",
  target: { type: "note", id: "note-1" },
  result: "success",
  reasonCode: "removed",
  correlationId: "request-1",
  summary: { removed: true },
  ...overrides,
});
const summaryPolicy = {
  "note.remove": {
    removed: { type: "boolean" as const },
    count: { type: "integer" as const, min: 0, max: 10 },
    state: { type: "enum" as const, values: ["ready", "missing"] },
  },
};

async function fixture() {
  const client = new Database(":memory:");
  try {
    client.exec(
      await Bun.file(new URL("../migrations/sqlite/0000_audit.sql", import.meta.url)).text(),
    );
  } catch (error) {
    client.close();
    throw error;
  }
  let active = true;
  const authentication = createAuth(
    realm("audit-test", {
      async verify(evidence: string | null) {
        if (!active) return { status: "rejected" as const };
        if (evidence === null) return { status: "absent" as const };
        return evidence === "alice" || evidence === "bob"
          ? { status: "verified" as const, subjectId: evidence, kind: "user" as const }
          : { status: "rejected" as const };
      },
    }),
  );
  const access = authentication.for(audience("audit.operations"));
  const authority = createAuthAuditAuthority(
    access,
    ({ principal, scope: requested }) =>
      requested.tenantId === "tenant-a" && requested.scopeId === `owner:${principal.subjectId}`,
  );
  const repository = createSqliteAuditRepository(drizzle(client));
  const diagnostics: AuditDiagnostic[] = [];
  const options = {
    repository,
    authority,
    summaryPolicy,
    report: (value: AuditDiagnostic) => {
      diagnostics.push(value);
    },
    clock: () => 200,
  };
  return {
    client,
    access,
    repository,
    diagnostics,
    options,
    service: createAuditService(options),
    revoke() {
      active = false;
    },
    async close() {
      await authentication.close();
      client.close();
    },
  };
}

test("trusted actors, exact audience, revocation and scope checks precede all storage access", async () => {
  const f = await fixture();
  try {
    const alice = await f.access.required("alice");
    const bob = await f.access.required("bob");
    const event = input();
    expect((await f.service.append(event, alice)).event.actor).toEqual({
      kind: "user",
      realmId: "audit-test",
      subjectId: "alice",
    });
    const forged = { ...alice } as typeof alice;
    for (const principal of [null, forged, bob]) {
      await expect(f.service.query({ scope }, principal)).rejects.toMatchObject({
        code: "unauthorized",
      });
      await expect(f.service.get({ scope, id: event.id }, principal)).rejects.toMatchObject({
        code: "unauthorized",
      });
      await expect(
        f.service.get({ scope, id: crypto.randomUUID() }, principal),
      ).rejects.toMatchObject({ code: "unauthorized" });
    }
    await expect(
      f.service.query({ scope: { ...scope, tenantId: "tenant-b" } }, alice),
    ).rejects.toMatchObject({ code: "unauthorized" });
    const otherAccess = createAuth(
      realm("audit-test", {
        async verify() {
          return { status: "verified" as const, subjectId: "alice" };
        },
      }),
    );
    try {
      const wrong = await otherAccess.for(audience("wrong")).required(null);
      await expect(
        f.service.append(input(), wrong as unknown as typeof alice),
      ).rejects.toMatchObject({ code: "unauthorized" });
    } finally {
      await otherAccess.close();
    }
    f.revoke();
    await expect(f.service.query({ scope }, alice)).rejects.toMatchObject({ code: "unauthorized" });
    expect((await f.repository.list({ scope, limit: 10 })).length).toBe(1);
  } finally {
    await f.close();
  }
});

test("rejects client identity, secrets, arbitrary summaries and unbounded data without persisting", async () => {
  const f = await fixture();
  try {
    const alice = await f.access.required("alice");
    for (const bad of [
      { ...input(), actor: { kind: "system", systemId: "fake" } },
      { ...input(), recordedAt: 1 },
      input({ summary: { token: "secret", removed: true } }),
      input({ summary: { state: "Bearer-secret" } }),
      input({ summary: { count: 11 } }),
      input({ summary: { removed: "yes" } }),
      input({ target: { type: "note", id: "a".repeat(257) } }),
      input({ reasonCode: "a".repeat(65) }),
      input({ occurredAt: Number.NaN }),
      input({ scope: { scopeId: "public" } as AuditScope }),
      input({ correlationId: "https://user:password@example.test" }),
    ]) {
      await expect(f.service.append(bad, alice)).rejects.toMatchObject({ code: "invalid-input" });
    }
    expect(await f.repository.list({ scope, limit: 10 })).toEqual([]);
    expect(() =>
      createAuditService({
        ...f.options,
        summaryPolicy: {
          "note.remove": { credentialDigest: { type: "enum", values: ["hidden"] } },
        },
      }),
    ).toThrow(AuditError);
    await expect(f.service.query({ scope, limit: 501 }, alice)).rejects.toMatchObject({
      code: "invalid-input",
    });
    await expect(f.service.query({ limit: 1 } as never, alice)).rejects.toMatchObject({
      code: "invalid-input",
    });
  } finally {
    await f.close();
  }
});

test("append-only corrections, duplicate conflicts and bounded pages", async () => {
  const f = await fixture();
  try {
    const alice = await f.access.required("alice");
    const original = input();
    expect((await f.service.append(original, alice)).status).toBe("inserted");
    expect((await f.service.append(original, alice)).status).toBe("duplicate");
    await expect(
      f.service.append({ ...original, reasonCode: "changed" }, alice),
    ).rejects.toMatchObject({ code: "duplicate-conflict" });
    const correction = input({
      result: "failure",
      reasonCode: "corrected",
      relation: { kind: "correction", eventId: original.id },
    });
    await f.service.append(correction, alice);
    expect((await f.service.get({ scope, id: original.id }, alice))?.result).toBe("success");
    await expect(
      f.service.append(
        input({
          relation: { kind: "correction", eventId: crypto.randomUUID() },
        }),
        alice,
      ),
    ).rejects.toMatchObject({ code: "relation-missing" });
    const first = await f.service.query({ scope, limit: 1 }, alice);
    expect(first.events).toHaveLength(1);
    expect(first.nextCursor).not.toBeNull();
    expect(Object.hasOwn(first, "count")).toBe(false);
    const second = await f.service.query({ scope, limit: 1, cursor: first.nextCursor! }, alice);
    expect(second.events).toHaveLength(1);
    expect(second.events[0].id).not.toBe(first.events[0].id);
    expect(second.nextCursor).toBeNull();
    expect("delete" in f.service || "update" in f.service).toBe(false);
  } finally {
    await f.close();
  }
});

test("strict requires a durable acknowledgement, never admits duplicate intent, reports post-effect unknown", async () => {
  const f = await fixture();
  try {
    const alice = await f.access.required("alice");
    await expect(f.service.prepare(input({ result: "intent" }), alice)).rejects.toMatchObject({
      code: "strict-unavailable",
    });
    let failing = false;
    // Fault injection tests protocol ordering only, not crash durability.
    const repository: AuditRepository = {
      ...f.repository,
      durableIntents: true,
      async insert(event) {
        if (failing) throw new Error("SQL credential=never-leak");
        return f.repository.insert(event);
      },
    };
    const service = createAuditService({ ...f.options, repository });
    const intent = input({ result: "intent", reasonCode: "requested", summary: {} });
    failing = true;
    await expect(service.prepare(intent, alice)).rejects.toMatchObject({ code: "storage-failed" });
    expect(await f.repository.get(scope, intent.id)).toBeNull();
    failing = false;
    const prepared = await service.prepare(intent, alice);
    expect(prepared.status).toBe("ready");
    expect(await service.prepare(intent, alice)).toEqual({
      status: "already-recorded",
      intentId: intent.id,
    });
    if (prepared.status !== "ready") throw new Error("Expected ready receipt");
    await expect(
      service.complete(
        { ...prepared.receipt },
        {
          id: crypto.randomUUID(),
          occurredAt: 300,
          result: "success",
          reasonCode: "done",
        },
      ),
    ).rejects.toMatchObject({ code: "invalid-receipt" });
    let externalEffects = 0;
    // The effect is deliberately outside the Audit database.
    externalEffects++;
    failing = true;
    const outcome = {
      id: crypto.randomUUID(),
      occurredAt: 300,
      result: "success" as const,
      reasonCode: "done",
      summary: { removed: true },
    };
    await expect(service.complete(prepared.receipt, outcome)).rejects.toBeInstanceOf(
      AuditOutcomeUnknownError,
    );
    expect(externalEffects).toBe(1);
    expect((await f.repository.get(scope, intent.id))?.result).toBe("intent");
    expect(f.diagnostics).toEqual([
      { mode: "strict", stage: "intent", code: "storage-failed" },
      { mode: "strict", stage: "outcome", code: "storage-failed" },
    ]);
    failing = false;
    const completed = await service.complete(prepared.receipt, outcome);
    expect(completed.relation).toEqual({ kind: "outcome", eventId: intent.id });
    expect(completed.actor).toEqual({ kind: "user", realmId: "audit-test", subjectId: "alice" });
    expect(await service.complete(prepared.receipt, outcome)).toEqual(completed);
  } finally {
    await f.close();
  }
});

test("best-effort storage failure is visible, safe, and never downgrades authorization/input failures", async () => {
  const f = await fixture();
  try {
    const alice = await f.access.required("alice");
    const logs: unknown[] = [];
    const repository: AuditRepository = {
      ...f.repository,
      async insert() {
        throw new Error("password=not-for-diagnostics");
      },
    };
    const reporter = createAuditReporter({
      logger: {
        warn(fields, message) {
          logs.push({ fields, message });
        },
      },
    });
    const service = createAuditService({ ...f.options, repository, report: reporter });
    expect(await service.appendBestEffort(input(), alice)).toEqual({
      status: "unconfirmed",
      code: "storage-failed",
    });
    expect(logs).toEqual([
      {
        fields: { mode: "best-effort", stage: "append", code: "storage-failed" },
        message: "Audit persistence failed",
      },
    ]);
    await expect(
      service.appendBestEffort(input(), { ...alice } as typeof alice),
    ).rejects.toMatchObject({ code: "unauthorized" });
    await expect(
      service.appendBestEffort(input({ summary: { password: "hidden" } }), alice),
    ).rejects.toMatchObject({ code: "invalid-input" });
    const silent = createAuditService({ ...f.options, report: undefined });
    await expect(silent.appendBestEffort(input(), alice)).rejects.toMatchObject({
      code: "diagnostics-required",
    });
    const brokenReporter = createAuditService({
      ...f.options,
      repository,
      async report() {
        throw new Error("logger token=never-leak");
      },
    });
    try {
      await brokenReporter.appendBestEffort(input(), alice);
      throw new Error("Expected diagnostic failure");
    } catch (error) {
      expect(error).toBeInstanceOf(AuditError);
      expect(error).toMatchObject({ code: "diagnostics-failed" });
      expect(String(error)).not.toContain("token");
    }
  } finally {
    await f.close();
  }
});

test("explicit trusted system authority can audit a public resource without inventing a user", async () => {
  const f = await fixture();
  try {
    const authority = {
      async resolve(principal: object) {
        if (principal !== system) throw new Error("untrusted");
        return { kind: "system" as const, systemId: "maintenance" };
      },
    };
    const system = Object.freeze({});
    const service = createAuditService({ repository: f.repository, authority });
    const event = input({
      scope: { tenantId: null, scopeId: "public" },
      summary: {},
    });
    expect((await service.append(event, system)).event.actor).toEqual({
      kind: "system",
      systemId: "maintenance",
    });
    await expect(service.append(event, {})).rejects.toMatchObject({ code: "unauthorized" });
    expect((await service.query({ scope: event.scope }, system)).events).toHaveLength(1);
  } finally {
    await f.close();
  }
});

test("trusted identity snapshots preserve the existing Auth realm and opaque subject contract", async () => {
  const f = await fixture();
  const authentication = createAuth(
    realm("https://issuer.example", {
      async verify() {
        return { status: "verified" as const, subjectId: "auth0|alice" };
      },
    }),
  );
  try {
    const access = authentication.for(audience("audit"));
    const principal = await access.required(null);
    const authority = createAuthAuditAuthority(
      access,
      ({ scope: requested }) =>
        requested.scopeId === scope.scopeId && requested.tenantId === scope.tenantId,
    );
    const service = createAuditService({ ...f.options, authority });
    expect((await service.append(input(), principal)).event.actor).toEqual({
      kind: "user",
      realmId: "https://issuer.example",
      subjectId: "auth0|alice",
    });
    expect((await service.query({ scope }, principal)).events).toHaveLength(1);
  } finally {
    await authentication.close();
    await f.close();
  }
});
