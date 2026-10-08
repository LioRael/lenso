import { expect, test } from "bun:test";
import { audience, createAuth, defineSource, realm, type Actor } from "@lenso/auth";
import { definePlugin, startApp } from "@lenso/core";
import { createAuthOrganizationAccess, organizationMembershipReader } from "../src/auth";
import {
  createOrganizationService,
  memberRelationshipPolicy,
  OrganizationError,
  type OrganizationState,
  type OrganizationStore,
} from "../src/index";
import { createOrganizationPlugin } from "../src/plugin";

// Contract-only fixture; real PG and local workerd D1 are exercised separately.
function memoryStore(): OrganizationStore {
  const states = new Map<string, OrganizationState>();
  return {
    async create(state) {
      if (states.has(state.organization.id)) throw new Error("duplicate");
      states.set(state.organization.id, structuredClone(state));
    },
    async read(id) {
      const state = states.get(id);
      return state ? { state: structuredClone(state), observedAt: Date.now() } : null;
    },
    async compareAndSwap(expected, state, validUntil) {
      if (
        states.get(state.organization.id)?.organization.version !== expected ||
        (validUntil !== undefined && Date.now() >= validUntil)
      )
        return false;
      states.set(state.organization.id, structuredClone(state));
      return true;
    },
  };
}

test("Auth owns proof validation; organization relationships do not grant Console admission", async () => {
  const enabled = new Set(["alice", "bob", "console-only"]);
  const source = defineSource({
    async verify(evidence: string) {
      return enabled.has(evidence)
        ? { status: "verified" as const, subjectId: evidence }
        : { status: "rejected" as const };
    },
  });
  const auth = createAuth(realm("shared", source));
  const sso = createAuth(realm("console-sso", source));
  const target = audience("business:organization");
  const access = auth.for(target);
  const store = memoryStore();
  const service = createOrganizationService({
    store,
    access: createAuthOrganizationAccess({ access, policy: memberRelationshipPolicy }),
  });
  try {
    const alice = await access.required("alice");
    const bob = await access.required("bob");
    const consoleOnly = await access.required("console-only");
    const created = await service.createOrganization(alice, { name: "Business" });
    const scope = { organizationId: created.value.id };
    const bobRef = { realmId: bob.realmId, subjectId: bob.subjectId };
    await service.addMember(alice, { ...scope, subject: bobRef });
    // Shared identity need not be admitted to Console, and Console admission
    // does not imply membership in the business organization.
    const consoleAccess = auth.for(audience("console:enter"));
    const consolePolicy = ({ principal }: { principal: { subjectId: string } }) =>
      principal.subjectId === "console-only";
    await expect(
      consoleAccess.enforce(await consoleAccess.required("alice"), {}, consolePolicy),
    ).rejects.toMatchObject({ code: "FORBIDDEN" });
    await consoleAccess.enforce(await consoleAccess.required("console-only"), {}, consolePolicy);
    await expect(service.listMembers(consoleOnly, scope)).rejects.toMatchObject({
      code: "FORBIDDEN",
    });
    await expect(service.readOrganization({ ...alice }, scope)).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    const otherAudience = await consoleAccess.required("alice");
    await expect(
      service.readOrganization(otherAudience as unknown as typeof alice, scope),
    ).rejects.toMatchObject({ code: "UNAUTHORIZED" });

    const readMembership = organizationMembershipReader(store);
    const context = { signal: new AbortController().signal };
    const before = await readMembership(bob, scope, context);
    expect(before).toMatchObject({ role: "member", version: 2 });
    expect(
      await readMembership({ realmId: "console-sso", subjectId: "bob" }, scope, context),
    ).toBeNull();
    await service.removeMember(alice, { ...scope, subject: bobRef });
    expect(await readMembership(bob, scope, context)).toBeNull();
    await expect(service.listMembers(bob, scope)).rejects.toMatchObject({ code: "FORBIDDEN" });
    expect((await service.readOrganization(alice, scope)).version).toBe(3);
    enabled.delete("alice");
    await expect(service.readOrganization(alice, scope)).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
  } finally {
    await auth.close();
    await sso.close();
  }
});

test("realm is part of invitation target and independent SSO needs no duplicate password system", async () => {
  const source = defineSource({
    async verify(subjectId: string) {
      return { status: "verified" as const, subjectId };
    },
  });
  const shared = createAuth(realm("shared", source));
  const consoleSSO = createAuth(realm("console-sso", source));
  const entry = audience("organization:manage");
  const sharedAccess = shared.for(entry);
  const ssoAccess = consoleSSO.for(entry);
  const store = memoryStore();
  // Explicit trusted realm routing, never a realm from business input.
  type Principal = Actor<string, string, typeof entry.id>;
  const adapters = [
    createAuthOrganizationAccess({ access: sharedAccess, policy: memberRelationshipPolicy }),
    createAuthOrganizationAccess({ access: ssoAccess, policy: memberRelationshipPolicy }),
  ];
  const service = createOrganizationService<Principal>({
    store,
    access: {
      async check(actor, request) {
        return actor.realmId === "shared"
          ? adapters[0].check(actor as Actor<"shared", string, typeof entry.id>, request)
          : adapters[1].check(actor as Actor<"console-sso", string, typeof entry.id>, request);
      },
    },
  });
  try {
    const owner = await sharedAccess.required("owner");
    const sharedBob = await sharedAccess.required("bob");
    const ssoBob = await ssoAccess.required("bob");
    const { value: organization } = await service.createOrganization(owner, { name: "SSO team" });
    const scope = { organizationId: organization.id };
    const invite = (
      await service.createInvitation(owner, {
        ...scope,
        target: { realmId: "console-sso", subjectId: "bob" },
      })
    ).value;
    const input = { ...scope, invitationId: invite.invitation.id, token: invite.token };
    await expect(service.acceptInvitation(sharedBob, input)).rejects.toMatchObject({
      code: "NOT_FOUND",
    });
    await service.acceptInvitation(ssoBob, input);
    expect((await service.listMembers(ssoBob, scope)).total).toBe(2);
    await expect(service.listMembers(sharedBob, scope)).rejects.toMatchObject({
      code: "FORBIDDEN",
    });
  } finally {
    await shared.close();
    await consoleSSO.close();
  }
});

test("exact plugin instances are required and borrowed resources remain usable after stop", async () => {
  const store = memoryStore();
  const database = definePlugin({ id: "org-db", setup: () => store });
  const checker = {
    async check(actor: { realmId: string; subjectId: string }) {
      return actor;
    },
  };
  const access = definePlugin({ id: "org-access", setup: () => checker });
  const organization = createOrganizationPlugin({
    id: "organization",
    database,
    access,
    store: (value) => value,
  });
  const impostor = definePlugin({ id: "org-db", setup: () => store });
  await expect(startApp({ plugins: [impostor, access, organization] })).rejects.toThrow();
  const app = await startApp({ plugins: [database, access, organization] });
  const result = await app
    .get(organization)
    .createOrganization({ realmId: "business", subjectId: "owner" }, { name: "Borrowed" });
  await app.stop();
  expect(await store.read(result.value.id)).not.toBeNull();
});

test("pending invites cannot restore revoked membership; business JSON cannot inject a role", async () => {
  const store = memoryStore();
  const service = createOrganizationService({
    store,
    access: {
      async check(actor: { realmId: string; subjectId: string }, request) {
        if (!memberRelationshipPolicy(actor, request)) throw new OrganizationError("FORBIDDEN");
        return actor;
      },
    },
  });
  const owner = { realmId: "business", subjectId: "owner" };
  const bob = { realmId: "business", subjectId: "bob" };
  const org = (await service.createOrganization(owner, { name: "Team" })).value;
  const scope = { organizationId: org.id };
  const invite = (await service.createInvitation(owner, { ...scope, target: bob })).value;
  await expect(
    service.addMember(owner, { ...scope, subject: bob, role: "owner" } as never),
  ).rejects.toMatchObject({ code: "INVALID_INPUT" });
  await service.addMember(owner, { ...scope, subject: bob });
  await service.removeMember(owner, { ...scope, subject: bob });
  await expect(
    service.acceptInvitation(bob, {
      ...scope,
      invitationId: invite.invitation.id,
      token: invite.token,
    }),
  ).rejects.toMatchObject({ code: "INVITATION_REVOKED" });
});

test("CAS retry reauthenticates and recomputes; an uncertain write is never retried", async () => {
  const backing = memoryStore();
  let checks = 0;
  let writes = 0;
  let mode: "conflict" | "unknown" | "normal" = "normal";
  const store: OrganizationStore = {
    ...backing,
    async compareAndSwap(expected, next, deadline) {
      writes++;
      if (mode === "unknown") throw new Error("unknown outcome");
      if (mode === "conflict" && writes === 1) return false;
      return backing.compareAndSwap(expected, next, deadline);
    },
  };
  const service = createOrganizationService({
    store,
    access: {
      async check(actor: { realmId: string; subjectId: string }) {
        checks++;
        if (mode === "conflict" && checks > 1) throw new OrganizationError("FORBIDDEN");
        return actor;
      },
    },
  });
  const actor = { realmId: "business", subjectId: "owner" };
  const org = (await service.createOrganization(actor, { name: "Retry" })).value;
  checks = 0;
  mode = "conflict";
  await expect(
    service.updateOrganization(actor, { organizationId: org.id, name: "No" }),
  ).rejects.toMatchObject({ code: "FORBIDDEN" });
  expect(checks).toBe(2);
  expect(writes).toBe(1);
  writes = 0;
  mode = "unknown";
  await expect(
    service.updateOrganization(actor, { organizationId: org.id, name: "Unknown" }),
  ).rejects.toThrow("unknown outcome");
  expect(writes).toBe(1);
});

test("near-capacity invites reserve space for atomic membership revocation", async () => {
  const store = memoryStore();
  const service = createOrganizationService({
    store,
    access: {
      async check(actor: { realmId: string; subjectId: string }, request) {
        if (!memberRelationshipPolicy(actor, request)) throw new OrganizationError("FORBIDDEN");
        return actor;
      },
    },
  });
  const owner = { realmId: "business", subjectId: "a" };
  const target = { realmId: "business", subjectId: "b".repeat(128) };
  const org = (await service.createOrganization(owner, { name: "x" })).value;
  const scope = { organizationId: org.id };
  // Fill through public APIs, rather than seeding an unreachable snapshot.
  let admitted = 0;
  for (; admitted < 1000; admitted++) {
    try {
      await service.createInvitation(owner, { ...scope, target });
    } catch (error) {
      if (!(error instanceof OrganizationError) || error.code !== "CAPACITY") throw error;
      break;
    }
  }
  expect(admitted).toBeGreaterThan(930);
  expect(admitted).toBeLessThan(987);
  await service.addMember(owner, { ...scope, subject: target });
  const before = (await store.read(org.id))!.state;
  expect(new TextEncoder().encode(JSON.stringify(before)).byteLength).toBeGreaterThan(480_000);
  await service.removeMember(owner, { ...scope, subject: target });
  const after = (await store.read(org.id))!.state;
  expect(after.members).toHaveLength(1);
  expect(after.invitations.every((invite) => invite.status === "revoked")).toBe(true);
  expect(new TextEncoder().encode(JSON.stringify(after)).byteLength).toBeLessThan(512_000);
  const leaveStore = memoryStore();
  // Reuse the very same service-generated snapshot for the exit variant.
  await leaveStore.create(before);
  const leave = createOrganizationService({
    store: leaveStore,
    access: {
      async check(actor: typeof target, request) {
        if (!memberRelationshipPolicy(actor, request)) throw new OrganizationError("FORBIDDEN");
        return actor;
      },
    },
  });
  await leave.leaveOrganization(target, scope);
  const exited = (await leaveStore.read(org.id))!.state;
  expect(exited.members).toHaveLength(1);
  expect(exited.invitations.every((invite) => invite.status === "revoked")).toBe(true);
}, 20_000);

test("access policies receive scoped invitation facts and the requested role, never digests", async () => {
  const store = memoryStore();
  const owner = { realmId: "business", subjectId: "owner" };
  const target = { realmId: "business", subjectId: "target" };
  let seen = false;
  const service = createOrganizationService({
    store,
    access: {
      async check(actor: typeof owner, request) {
        if (request.action === "invitation.revoke" || request.action === "invitation.read") {
          expect(request.invitationId).toBe(request.invitation?.id);
          expect(request.target).toEqual(target);
          expect(request.invitation).not.toHaveProperty("tokenDigest");
          seen = true;
          throw new OrganizationError("FORBIDDEN");
        }
        if (request.action === "member.role" && request.requestedRole === "admin") {
          throw new OrganizationError("FORBIDDEN");
        }
        return actor;
      },
    },
  });
  const scope = {
    organizationId: (await service.createOrganization(owner, { name: "Policy" })).value.id,
  };
  const invite = (await service.createInvitation(owner, { ...scope, target })).value.invitation;
  await expect(
    service.readInvitation(owner, { ...scope, invitationId: invite.id }),
  ).rejects.toMatchObject({ code: "FORBIDDEN" });
  await expect(
    service.revokeInvitation(owner, { ...scope, invitationId: invite.id }),
  ).rejects.toMatchObject({ code: "FORBIDDEN" });
  expect(seen).toBe(true);
  expect((await store.read(scope.organizationId))!.state.invitations[0].status).toBe("pending");
  await service.addMember(owner, { ...scope, subject: target });
  await expect(
    service.setMemberRole(owner, { ...scope, subject: target, role: "admin" }),
  ).rejects.toMatchObject({ code: "FORBIDDEN" });
});
