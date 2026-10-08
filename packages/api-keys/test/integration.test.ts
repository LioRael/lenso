import { expect, test } from "bun:test";
import { audience, createAuth, defineSource, realm, type ActorOf } from "@lenso/auth";
import { defineApp, startApp, definePlugin } from "@lenso/core";
import { createManageAdapter } from "@lenso/manage";
import { apiKeySource } from "../src/auth";
import { createApiKeyPlugin } from "../src/plugin";
import { createApiKeyManage } from "../src/manage";
import { namespacedSubjectId } from "../src";
import { definePluginConfig, valuesSource } from "@lenso/core/config";
import { z } from "zod";
import { apiKeyConfig } from "../src";
import { fixture } from "./fixture";

test("public Auth source revalidates scopes/revocation and current membership without actor forgery", async () => {
  const value = fixture();
  const issued = await value.keys.issue(value.input, {});
  const auth = createAuth(
    realm(
      "business",
      apiKeySource({
        keys: value.keys,
        requiredScopes: ["notes:read"],
        realmId: "business",
      }),
    ),
  );
  let member = true;
  const access = auth
    .for(audience("notes:read"))
    .memberships(async (_subject, resource: { tenantId: string }) =>
      member && resource.tenantId === value.subject.tenantId ? { read: true } : null,
    );
  try {
    const actor = await access.required(issued.credential);
    expect(actor.realmId).toBe("business");
    expect(actor.subjectId).toBe(namespacedSubjectId(value.subject));
    expect(JSON.stringify(actor)).not.toContain(issued.credential!);
    await access.enforce(actor, { tenantId: "tenant-a" }, ({ membership }) => membership.read);
    await expect(
      access.enforce({ ...actor }, { tenantId: "tenant-a" }, () => true),
    ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    await expect(access.enforce(actor, { tenantId: "tenant-b" }, () => true)).rejects.toMatchObject(
      { code: "FORBIDDEN" },
    );
    const wrongAudience = await auth.for(audience("notes:write")).required(issued.credential);
    await expect(
      access.enforce(wrongAudience as never, { tenantId: "tenant-a" }, () => true),
    ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    member = false;
    await expect(access.enforce(actor, { tenantId: "tenant-a" }, () => true)).rejects.toMatchObject(
      { code: "FORBIDDEN" },
    );
    member = true;
    await value.keys.revoke({ subject: value.subject, id: issued.key.id }, {});
    await expect(access.enforce(actor, { tenantId: "tenant-a" }, () => true)).rejects.toMatchObject(
      { code: "UNAUTHORIZED" },
    );
  } finally {
    await auth.close();
    await value.keys.close();
  }
});

test("explicit same-realm source routing shares canonical identities, not coincident source IDs", async () => {
  const value = fixture();
  const userKey = await value.keys.issue(value.input, {});
  const agentKey = await value.keys.issue(
    {
      ...value.input,
      subject: { ...value.subject, namespace: "agents" },
      requestId: "agent",
    },
    {},
  );
  const keySource = apiKeySource({ keys: value.keys, requiredScopes: ["notes:read"] });
  type Evidence =
    | { source: "key"; credential: string | null }
    | { source: "session"; credential: string };
  const combined = defineSource({
    async verify(evidence: Evidence, context) {
      if (evidence.source === "key") return keySource.verify(evidence.credential, context);
      if (evidence.credential !== "local-session-fixture") return { status: "rejected" } as const;
      return {
        status: "verified",
        subjectId: namespacedSubjectId(value.subject),
        kind: "user",
      } as const;
    },
  });
  const auth = createAuth(realm("business", combined));
  const access = auth.for(audience("notes:read"));
  try {
    const session = await access.required({
      source: "session",
      credential: "local-session-fixture",
    });
    const user = await access.required({ source: "key", credential: userKey.credential });
    const agent = await access.required({ source: "key", credential: agentKey.credential });
    expect(session.subjectId).toBe(user.subjectId);
    expect(agent.subjectId).not.toBe(user.subjectId);
    await expect(
      access.enforce(
        agent,
        { owner: user.subjectId },
        ({ principal, resource }) => principal.subjectId === resource.owner,
      ),
    ).rejects.toMatchObject({ code: "FORBIDDEN" });
    const writeAuth = createAuth(
      realm(
        "business",
        apiKeySource({
          keys: value.keys,
          requiredScopes: ["notes:write"],
        }),
      ),
    );
    try {
      await expect(
        writeAuth.for(audience("notes:write")).required(userKey.credential),
      ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    } finally {
      await writeAuth.close();
    }
    await value.keys.revoke({ subject: value.subject, id: userKey.key.id }, {});
    await access.enforce(session, {}, () => true);
    await expect(access.enforce(user, {}, () => true)).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
  } finally {
    await auth.close();
    await value.keys.close();
  }
});

test("opt-in Manage binds exact plugins and current Auth caller, refuses forged/cross-tenant management", async () => {
  let credentialActive = true;
  let canManage = true;
  const adminAuth = createAuth(
    realm(
      "staff",
      defineSource({
        async verify(evidence: string) {
          return credentialActive && evidence === "staff-test-credential"
            ? { status: "verified", subjectId: "staff" }
            : { status: "rejected" };
        },
      }),
    ),
  );
  const admin = adminAuth.for(audience("keys:manage"));
  type Caller = ActorOf<typeof admin>;
  const data = fixture<Caller>();
  let storeClosed = false;
  const database = definePlugin({
    id: "test-database",
    setup(context) {
      context.onCleanup(() => {
        storeClosed = true;
      });
      return data.store;
    },
  });
  const plugin = createApiKeyPlugin<Caller, { tenantId: string }>({
    id: "test-keys",
    requires: [database],
    setup(context) {
      return {
        store: context.get(database),
        config: { maxLifetimeMs: 60_000, maxOverlapMs: 1_000 },
        subjectActive: () => true,
        authorizeUse: (_key, _scope, resource) => resource.tenantId === "tenant-a",
        grantScopes: (_caller, _subject, requested) =>
          requested.filter((scope) => scope === "notes:read"),
        async authorizeManagement(caller, _action, target) {
          await admin.enforce(
            caller,
            target.subject,
            ({ resource }) =>
              canManage && resource.namespace === "accounts" && resource.tenantId === "tenant-a",
          );
          return true;
        },
      };
    },
  });
  const sidecar = createApiKeyManage(plugin, "test-key-management");
  const running = await startApp(defineApp({ plugins: [database, plugin, sidecar.plugin] }));
  let caller = await admin.required("staff-test-credential");
  const adapter = createManageAdapter({
    running,
    plugins: [database, plugin, sidecar.plugin],
    operations: sidecar.operations,
    canList: () => canManage,
    binding: () => ({ context: { caller } }),
  });
  try {
    const keys = running.get(plugin);
    const issued = await keys.issue(data.input, caller);
    expect(sidecar.operations.map((operation) => operation.method)).toEqual([
      "list",
      "read",
      "revoke",
    ]);
    const listed = await adapter.invoke(sidecar.plugin.id, "list", { subject: data.subject });
    expect(JSON.stringify(listed)).toContain(issued.key.id);
    expect(JSON.stringify(listed)).not.toContain("digest");
    expect(JSON.stringify(listed)).not.toContain(issued.credential!);
    await expect(
      adapter.invoke(sidecar.plugin.id, "read", {
        subject: { ...data.subject, tenantId: "tenant-b" },
        id: issued.key.id,
      }),
    ).rejects.toThrow();
    caller = { ...caller };
    await expect(
      adapter.invoke(sidecar.plugin.id, "revoke", { subject: data.subject, id: issued.key.id }),
    ).rejects.toThrow();
    caller = (await adminAuth
      .for(audience("keys:read"))
      .required("staff-test-credential")) as never;
    await expect(
      adapter.invoke(sidecar.plugin.id, "list", { subject: data.subject }),
    ).rejects.toThrow();
    const otherAuth = createAuth(
      realm(
        "staff",
        defineSource({
          async verify(_evidence: string) {
            return { status: "verified", subjectId: "staff" };
          },
        }),
      ),
    );
    try {
      caller = await otherAuth.for(audience("keys:manage")).required("test-only");
      await expect(
        adapter.invoke(sidecar.plugin.id, "list", { subject: data.subject }),
      ).rejects.toThrow();
    } finally {
      await otherAuth.close();
    }
    caller = await admin.required("staff-test-credential");
    await expect(
      adapter.invoke(sidecar.plugin.id, "list", {
        subject: data.subject,
        caller: { realmId: "staff", subjectId: "staff" },
      }),
    ).rejects.toThrow();
    canManage = false;
    expect(await adapter.catalog()).toEqual([]);
    await expect(keys.issue({ ...data.input, requestId: "denied" }, caller)).rejects.toThrow();
    canManage = true;
    credentialActive = false;
    await expect(
      adapter.invoke(sidecar.plugin.id, "list", { subject: data.subject }),
    ).rejects.toThrow();
    credentialActive = true;
    await adapter.invoke(sidecar.plugin.id, "revoke", { subject: data.subject, id: issued.key.id });
    expect(await keys.verify(issued.credential)).toBeNull();
    await keys.close();
    expect(storeClosed).toBeFalse();
  } finally {
    await running.stop();
    await adminAuth.close();
    await data.keys.close();
  }
  expect(storeClosed).toBeTrue();
});

test("long escaped identity references remain bounded through public Auth", async () => {
  const value = fixture();
  const subject = {
    namespace: "\\".repeat(256),
    tenantId: '"'.repeat(256),
    subjectId: "x".repeat(256),
  };
  const issued = await value.keys.issue({ ...value.input, subject }, {});
  const auth = createAuth(
    realm(
      "long-identity",
      apiKeySource({
        keys: value.keys,
        requiredScopes: ["notes:read"],
      }),
    ),
  );
  try {
    const actor = await auth.for(audience("notes:read")).required(issued.credential);
    expect(actor.subjectId).toBe(namespacedSubjectId(subject));
    expect(actor.subjectId.length).toBeLessThan(512);
    expect(namespacedSubjectId({ ...subject, namespace: "other" })).not.toBe(actor.subjectId);
  } finally {
    await auth.close();
    await value.keys.close();
  }
});

test("source rejects a credential revoked during asynchronous identity mapping", async () => {
  const value = fixture();
  const issued = await value.keys.issue(value.input, {});
  const auth = createAuth(
    realm(
      "mapping",
      apiKeySource({
        keys: value.keys,
        requiredScopes: ["notes:read"],
        async subjectId(subject) {
          await value.keys.revoke({ subject, id: issued.key.id }, {});
          return namespacedSubjectId(subject);
        },
      }),
    ),
  );
  try {
    await expect(
      auth.for(audience("notes:read")).required(issued.credential),
    ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  } finally {
    await auth.close();
    await value.keys.close();
  }
});

test("final service use closes the public Auth profile and post-policy expiry gaps", async () => {
  const value = fixture();
  const issued = await value.keys.issue(value.input, {});
  const auth = createAuth(
    realm(
      "business",
      apiKeySource({
        keys: value.keys,
        requiredScopes: ["notes:read"],
      }),
    ),
  );
  try {
    // Source profiles cannot see Auth audiences. The service still intersects
    // the operation's exact scope with the credential, even for a genuine actor.
    const write = auth.for(audience("notes:write"));
    const writeActor = await write.required(issued.credential);
    await write.enforce(writeActor, {}, () => true);
    await expect(
      value.keys.use(issued.credential, "notes:write", { tenantId: "tenant-a" }),
    ).rejects.toMatchObject({ code: "FORBIDDEN" });
    const read = auth.for(audience("notes:read"));
    const actor = await read.required(issued.credential);
    await read.enforce(actor, {}, async () => {
      value.advance(30_000);
      return true;
    });
    // Keys are not sessions: Auth has no non-session expiry finalization hook.
    await expect(
      value.keys.use(issued.credential, "notes:read", { tenantId: "tenant-a" }),
    ).rejects.toMatchObject({ code: "FORBIDDEN" });
  } finally {
    await auth.close();
    await value.keys.close();
  }
});

test("plugin uses the exact public Config binding before resource setup", async () => {
  const contract = definePluginConfig({
    schema: z
      .object({
        maxLifetimeMs: z.number().int().positive(),
        maxOverlapMs: z.number().int().nonnegative(),
      })
      .transform(apiKeyConfig),
  });
  const binding = {
    contract,
    sources: [
      valuesSource({ maxLifetimeMs: 60_000, maxOverlapMs: 0 }, { id: "credential-policy" }),
    ],
  };
  const data = fixture();
  const plugin = createApiKeyPlugin({
    id: "configured-keys",
    config: binding,
    setup(context) {
      return {
        store: data.store,
        config: context.config!(binding),
        subjectActive: () => true,
        authorizeManagement: (caller: object) => caller !== null,
        grantScopes: (_caller: object, _subject: unknown, requested: readonly string[]) =>
          requested,
        authorizeUse: () => true,
      };
    },
  });
  const running = await startApp(defineApp({ plugins: [plugin] }));
  try {
    const keys = running.get(plugin);
    const issued = await keys.issue(data.input, {});
    expect(() =>
      keys.rotate(
        {
          subject: data.subject,
          id: issued.key.id,
          expectedRevision: 0,
          overlapMs: 1,
        },
        {},
      ),
    ).toThrow();
  } finally {
    await running.stop();
    await data.keys.close();
  }
});
