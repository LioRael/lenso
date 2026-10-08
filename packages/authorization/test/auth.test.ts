import { expect, test } from "bun:test";
import { audience, createAuth, defineSource, realm, type ActorOf } from "@lenso/auth";
import { createAuthorizedAccess } from "../src/auth";
import { createAuthorization } from "../src/core";
import { predicate } from "../src/conditions";
import type { StoredNote } from "../../../examples/notes/src/notes";

test("Notes projection uses exact Auth actor, audience, current credential and trusted facts", async () => {
  let active = true;
  let reads = 0;
  const source = defineSource({
    async verify(token: string | null) {
      return token === null
        ? { status: "absent" as const }
        : token === "fixture" && active
          ? { status: "verified" as const, subjectId: "alice", kind: "user" as const }
          : { status: "rejected" as const };
    },
  });
  const auth = createAuth(realm("notes", source));
  const otherAuth = createAuth(realm("notes", source));
  try {
    const access = auth.for(audience("notes:read"));
    const foreignAccess = otherAuth.for(audience("notes:read"));
    const authorization = createAuthorization({
      actions: ["read", "update"],
      identity: { required: true, realms: ["notes"], audiences: ["notes:read"] },
      rules: [
        {
          id: "owner",
          effect: "allow",
          actions: ["read", "update"],
          resourceType: "note",
          when: predicate(
            (facts) => facts.principal?.subjectId === facts.resource.attributes?.owner,
          ),
        },
      ],
    });
    const protectedNotes = createAuthorizedAccess(access, authorization, (verified) => {
      reads++;
      const note = verified.resource as StoredNote;
      return {
        context: {},
        resource: {
          type: "note",
          id: note.id,
          scope: { type: "personal", id: note.ownerId },
          attributes: { owner: note.ownerId },
        },
      };
    });
    const note: StoredNote = {
      id: "one",
      ownerId: "alice",
      title: "Fixture",
      body: "",
      createdAt: new Date(0),
    };
    const actor = await access.required("fixture");
    expect(await protectedNotes.can(actor, "read", note)).toBe(true);
    const copied = { ...actor } as ActorOf<typeof access>;
    const json = JSON.parse(JSON.stringify(actor)) as ActorOf<typeof access>;
    const foreign = await foreignAccess.required("fixture");
    const wrongAudience = await auth.for(audience("notes:update")).required("fixture");
    for (const invalid of [
      copied,
      json,
      foreign,
      wrongAudience as unknown as ActorOf<typeof access>,
      null,
    ]) {
      expect(await protectedNotes.can(invalid, "read", note)).toBe(false);
    }
    expect(reads).toBe(1);
    expect(await protectedNotes.can(actor, "read", { ...note, ownerId: "bob" })).toBe(false);
    active = false;
    expect(await protectedNotes.can(actor, "read", note)).toBe(false);
    await expect(protectedNotes.enforce(actor, "read", note)).rejects.toThrow("Access denied.");
  } finally {
    await auth.close();
    await otherAuth.close();
  }
});

test("credential ceiling intersects permission and cannot confer Console audience", async () => {
  const auth = createAuth(
    realm(
      "service",
      defineSource({
        verify: async (_token: string) => ({
          status: "verified",
          subjectId: "agent",
          kind: "service",
        }),
      }),
    ),
  );
  try {
    const access = auth.for(audience("notes:read"));
    const authorization = createAuthorization({
      actions: ["read", "write"],
      identity: { credentialRequired: true, audiences: ["notes:read"] },
      policies: [{ evaluate: () => "allow" }],
    });
    const resource = { type: "note", id: "one", scope: { type: "project", id: "p" } };
    const protectedNotes = createAuthorizedAccess(access, authorization, () => ({
      context: {},
      resource,
      credential: {
        permissions: [{ action: "read", resourceType: "note", scope: resource.scope }],
      },
    }));
    const actor = await access.required("fixture");
    expect(await protectedNotes.can(actor, "read", resource)).toBe(true);
    expect(await protectedNotes.can(actor, "write", resource)).toBe(false);
    const consoleView = auth.for(audience("console:admit"));
    const consoleActor = await consoleView.required("fixture");
    expect(
      await protectedNotes.can(consoleActor as unknown as typeof actor, "read", resource),
    ).toBe(false);
  } finally {
    await auth.close();
  }
});

test("current membership failure and post-policy cancellation cannot leave stored allow", async () => {
  const auth = createAuth(
    realm(
      "app",
      defineSource({
        verify: async (_token: string) => ({ status: "verified", subjectId: "alice" }),
      }),
    ),
  );
  try {
    const resource = { type: "note", id: "one", scope: { type: "personal", id: "home" } };
    let membershipActive = true;
    const access = auth
      .for(audience("notes:read"))
      .memberships(async () => (membershipActive ? { role: "reader" } : null));
    const engine = createAuthorization({
      actions: ["read"],
      policies: [{ evaluate: () => "allow" }],
    });
    const protectedNotes = createAuthorizedAccess(access, engine, () => ({
      resource,
      context: {},
    }));
    const actor = await access.required("fixture");
    expect(await protectedNotes.can(actor, "read", resource)).toBe(true);
    membershipActive = false;
    expect(await protectedNotes.can(actor, "read", resource)).toBe(false);
    const signal = new AbortController();
    signal.abort("secret");
    expect(
      (await protectedNotes.check(actor, "read", resource, { signal: signal.signal })).effect,
    ).toBe("deny");
  } finally {
    await auth.close();
  }
});

test("Auth membership and policy project the same resource snapshot across awaits", async () => {
  const auth = createAuth(
    realm(
      "app",
      defineSource({
        verify: async (_token: string) => ({ status: "verified", subjectId: "alice" }),
      }),
    ),
  );
  try {
    let entered!: () => void;
    let resume!: () => void;
    const started = new Promise<void>((resolve) => {
      entered = resolve;
    });
    const pause = new Promise<void>((resolve) => {
      resume = resolve;
    });
    const resource = { type: "note", id: "one", scope: { type: "organization", id: "a" } };
    const access = auth
      .for(audience("notes:read"))
      .memberships(async (_principal, item: typeof resource) => {
        if (item.scope.id !== "a") return null;
        entered();
        await pause;
        return { role: "editor" };
      });
    const engine = createAuthorization({
      actions: ["read"],
      policies: [
        {
          evaluate: (facts) =>
            facts.context.role === "editor" && facts.resource.scope.id === "b"
              ? "allow"
              : "abstain",
        },
      ],
    });
    const protectedNotes = createAuthorizedAccess(access, engine, (verified) => ({
      resource: verified.resource,
      context: { role: verified.membership.role },
    }));
    const actor = await access.required("fixture");
    const checked = protectedNotes.can(actor, "read", resource);
    await started;
    resource.scope.id = "b";
    resume();
    expect(await checked).toBe(false);
  } finally {
    await auth.close();
  }
});

test("borrowed membership is detached before async facts projection", async () => {
  const auth = createAuth(
    realm(
      "app",
      defineSource({
        verify: async (_token: string) => ({ status: "verified", subjectId: "alice" }),
      }),
    ),
  );
  try {
    const sharedMembership = { role: "viewer" };
    const access = auth.for(audience("notes:read")).memberships(async () => sharedMembership);
    const resource = { type: "note", id: "one", scope: { type: "app", id: "one" } };
    const engine = createAuthorization({
      actions: ["read"],
      policies: [{ evaluate: (facts) => (facts.context.role === "admin" ? "allow" : "abstain") }],
    });
    let entered!: () => void;
    let resume!: () => void;
    const started = new Promise<void>((resolve) => {
      entered = resolve;
    });
    const pause = new Promise<void>((resolve) => {
      resume = resolve;
    });
    const protectedNotes = createAuthorizedAccess(access, engine, async (verified) => {
      entered();
      await pause;
      return {
        resource: verified.resource as typeof resource,
        context: { role: verified.membership.role },
      };
    });
    const actor = await access.required("fixture");
    const checked = protectedNotes.can(actor, "read", resource);
    await started;
    sharedMembership.role = "admin";
    resume();
    expect(await checked).toBe(false);
  } finally {
    await auth.close();
  }
});
