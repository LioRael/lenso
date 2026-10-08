import { expect, test } from "bun:test";
import { audience, createAuth, defineSource, realm } from "@lenso/auth";
import { createAuthOrganizationAccess } from "../src/auth";
import {
  createOrganizationService,
  memberRelationshipPolicy,
  OrganizationError,
  resolveOrganizationConfig,
  type AccessRequest,
  type OrganizationState,
  type OrganizationStore,
} from "../src/index";
import { decodeSnapshot, encodeState } from "../src/drizzle/shared";

const owner = { realmId: "business", subjectId: "owner" };
const ordinary = { realmId: "business", subjectId: "member" };

function state(): OrganizationState {
  return {
    organization: {
      id: "organization-a",
      name: "A",
      version: 1,
      createdAt: 1,
      updatedAt: 1,
    },
    members: [
      { subject: owner, role: "owner", joinedAt: 1 },
      { subject: ordinary, role: "member", joinedAt: 1 },
    ],
    invitations: [],
  };
}

test("cross-organization administration needs an explicit authenticated scope and action policy", async () => {
  const auth = createAuth(
    realm(
      "business",
      defineSource({
        async verify(evidence: string) {
          return evidence === "fixture-console-admin"
            ? { status: "verified" as const, subjectId: "console-admin" }
            : { status: "rejected" as const };
        },
      }),
    ),
  );
  const access = auth.for(audience("business:organization"));
  const stored = state();
  const request: AccessRequest = {
    action: "member.list",
    organizationId: stored.organization.id,
    organization: stored.organization,
    members: stored.members,
  };
  try {
    const actor = await access.required("fixture-console-admin");
    const ordinaryPolicy = createAuthOrganizationAccess({
      access,
      policy: memberRelationshipPolicy,
    });
    await expect(ordinaryPolicy.check(actor, request)).rejects.toMatchObject({ code: "FORBIDDEN" });
    const allowed = new Set(["organization-a"]);
    const explicitPolicy = createAuthOrganizationAccess({
      access,
      policy: (principal, resource) =>
        principal.subjectId === "console-admin" &&
        resource.action === "member.list" &&
        resource.organizationId !== null &&
        allowed.has(resource.organizationId),
    });
    expect(await explicitPolicy.check(actor, request)).toEqual({
      realmId: "business",
      subjectId: "console-admin",
    });
    await expect(
      explicitPolicy.check(actor, { ...request, organizationId: "organization-b" }),
    ).rejects.toMatchObject({ code: "FORBIDDEN" });
    await expect(
      explicitPolicy.check(actor, { ...request, action: "owner.grant" }),
    ).rejects.toMatchObject({ code: "FORBIDDEN" });
  } finally {
    await auth.close();
  }
});

test("self-elevation fails even if an application checker accidentally permits the action", async () => {
  const original = state();
  const store: OrganizationStore = {
    async create() {
      throw new Error("unexpected create");
    },
    async read() {
      return { state: structuredClone(original), observedAt: Date.now() };
    },
    async compareAndSwap() {
      throw new Error("unexpected write");
    },
  };
  const service = createOrganizationService({
    store,
    access: {
      async check(actor: typeof ordinary) {
        return { ...actor, extraTrustedClaim: "not part of the stored subject" };
      },
    },
  });
  const scope = { organizationId: original.organization.id };
  await expect(
    service.setMemberRole(ordinary, { ...scope, subject: ordinary, role: "admin" }),
  ).rejects.toMatchObject({ code: "FORBIDDEN" });
  await expect(service.grantOwner(ordinary, { ...scope, subject: ordinary })).rejects.toMatchObject(
    { code: "FORBIDDEN" },
  );
  await expect(
    service.setMemberRole(ordinary, { ...scope, subject: ordinary, role: "owner" } as never),
  ).rejects.toMatchObject({ code: "INVALID_INPUT" });
  await expect(
    service.createInvitation(ordinary, { ...scope, target: owner, role: "owner" } as never),
  ).rejects.toMatchObject({ code: "INVALID_INPUT" });
});

test("invalid configuration and corrupted persisted relationships fail closed", () => {
  for (const config of [
    { conflictRetries: -1 },
    { maxMembers: 0 },
    { maxInvitations: 1001 },
    { invitationLifetimeMs: 999 },
    { invitationLifetimeMs: NaN },
    { unknown: true },
  ]) {
    expect(() => resolveOrganizationConfig(config as never)).toThrow(OrganizationError);
  }
  const original = state();
  const encoded = encodeState(original);
  expect(decodeSnapshot(original.organization.id, 1, encoded, 123).state).toEqual(original);
  expect(() => decodeSnapshot("foreign-id", 1, encoded, 123)).toThrow();
  expect(() => decodeSnapshot(original.organization.id, 2, encoded, 123)).toThrow();
  expect(() => decodeSnapshot(original.organization.id, 1, encoded, NaN)).toThrow();
  original.members.push(original.members[0]);
  expect(() => encodeState(original)).toThrow("Duplicate");
  original.members = [];
  expect(() => encodeState(original)).toThrow();
});
