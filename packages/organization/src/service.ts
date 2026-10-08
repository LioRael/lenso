import {
  OrganizationError,
  type Invitation,
  type Member,
  type MembershipChange,
  type MutationResult,
  type Organization,
  type OrganizationAccess,
  type OrganizationAction,
  type OrganizationConfig,
  type OrganizationState,
  type OrganizationStore,
  type StoredInvitation,
  type SubjectRef,
} from "./contracts";
import { assertSnapshotCapacity } from "./capacity";
import { preserveOwners, sameSubject } from "./policy";

const defaults: OrganizationConfig = {
  invitationLifetimeMs: 7 * 24 * 60 * 60 * 1000,
  maxMembers: 1000,
  maxInvitations: 1000,
  conflictRetries: 8,
};

function fail(code: ConstructorParameters<typeof OrganizationError>[0]): never {
  throw new OrganizationError(code);
}

function object(value: unknown, keys: readonly string[]): Record<string, unknown> {
  if (
    !value ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    ![Object.prototype, null].includes(Object.getPrototypeOf(value)) ||
    Object.keys(value).some((key) => !keys.includes(key))
  )
    fail("INVALID_INPUT");
  return value as Record<string, unknown>;
}

function text(value: unknown, maximum = 512): string {
  if (typeof value !== "string" || !value.trim() || value.length > maximum) fail("INVALID_INPUT");
  return value;
}

function subject(value: unknown): SubjectRef {
  const input = object(value, ["realmId", "subjectId"]);
  return { realmId: text(input.realmId, 256), subjectId: text(input.subjectId) };
}

function integer(value: unknown, min: number, max: number): number {
  if (!Number.isSafeInteger(value) || (value as number) < min || (value as number) > max) {
    fail("INVALID_INPUT");
  }
  return value as number;
}

export function resolveOrganizationConfig(
  input: Partial<OrganizationConfig> = {},
): OrganizationConfig {
  const values = { ...defaults, ...object(input, Object.keys(defaults)) };
  return Object.freeze({
    invitationLifetimeMs: integer(values.invitationLifetimeMs, 1000, 30 * 24 * 60 * 60 * 1000),
    maxMembers: integer(values.maxMembers, 1, 1000),
    maxInvitations: integer(values.maxInvitations, 1, 1000),
    conflictRetries: integer(values.conflictRetries, 0, 32),
  });
}

function scoped(input: unknown, extra: readonly string[] = []) {
  const parsed = object(input, ["organizationId", ...extra]);
  return { parsed, organizationId: text(parsed.organizationId, 256) };
}

function member(state: OrganizationState, target: SubjectRef): Member {
  return state.members.find((item) => sameSubject(item.subject, target)) ?? fail("NOT_FOUND");
}

function invitation(state: OrganizationState, id: string): StoredInvitation {
  return state.invitations.find((item) => item.id === id) ?? fail("NOT_FOUND");
}

function present(invite: StoredInvitation, now: number): Invitation {
  const { tokenDigest: _digest, ...result } = invite;
  return {
    ...result,
    status: result.status === "pending" && result.expiresAt <= now ? "expired" : result.status,
  };
}

function pending(invite: StoredInvitation, now: number): void {
  if (invite.status === "accepted") fail("INVITATION_ACCEPTED");
  if (invite.status === "revoked") fail("INVITATION_REVOKED");
  if (invite.expiresAt <= now) fail("INVITATION_EXPIRED");
}

function revokePendingFor(state: OrganizationState, target: SubjectRef, now: number): void {
  for (const invite of state.invitations) {
    if (invite.status === "pending" && sameSubject(invite.target, target)) {
      invite.status = "revoked";
      invite.resolvedAt = now;
    }
  }
}

async function digest(
  invite: Omit<StoredInvitation, "tokenDigest">,
  bearer: string,
): Promise<string> {
  const data = JSON.stringify([
    invite.organizationId,
    invite.id,
    invite.target.realmId,
    invite.target.subjectId,
    invite.role,
    invite.expiresAt,
    bearer,
  ]);
  return Array.from(
    new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(data))),
    (byte) => byte.toString(16).padStart(2, "0"),
  ).join("");
}

function token(): string {
  return Array.from(crypto.getRandomValues(new Uint8Array(32)), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

function equalDigest(a: string, b: string): boolean {
  let difference = a.length ^ b.length;
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    difference |= (a.charCodeAt(i) || 0) ^ (b.charCodeAt(i) || 0);
  }
  return difference === 0;
}

/**
 * Ordinary async service. Stores/access providers are borrowed. Notification is
 * deliberately outside this commit boundary: send only after createInvitation
 * returns, and never log its returned bearer token.
 */
export function createOrganizationService<Actor>(options: {
  store: OrganizationStore;
  access: OrganizationAccess<Actor>;
  config?: Partial<OrganizationConfig>;
}) {
  const config = resolveOrganizationConfig(options.config);
  const { store, access } = options;

  async function authorize(
    actor: Actor,
    action: OrganizationAction,
    organizationId: string | null,
    state: OrganizationState | null,
    target?: SubjectRef,
    details?: {
      requestedRole?: Member["role"];
      invitationId?: string;
      invitation?: Invitation | null;
    },
  ) {
    const verified = await access.check(
      actor,
      structuredClone({
        action,
        organizationId,
        organization: state?.organization ?? null,
        members: state?.members ?? [],
        ...(target ? { target } : {}),
        ...details,
      }),
    );
    return subject({ realmId: verified?.realmId, subjectId: verified?.subjectId });
  }

  async function load(
    actor: Actor,
    action: OrganizationAction,
    organizationId: string,
    target?: SubjectRef,
    details?: { invitationId?: string; requestedRole?: Member["role"] },
  ) {
    const snapshot = await store.read(organizationId);
    const invite = details?.invitationId
      ? snapshot?.state.invitations.find((item) => item.id === details.invitationId)
      : undefined;
    const principal = await authorize(
      actor,
      action,
      organizationId,
      snapshot?.state ?? null,
      invite?.target ?? target,
      {
        ...details,
        ...(details?.invitationId
          ? { invitation: invite && snapshot ? present(invite, snapshot.observedAt) : null }
          : {}),
      },
    );
    if (!snapshot) fail("NOT_FOUND");
    if (snapshot.state.organization.id !== organizationId) fail("NOT_FOUND");
    return { ...snapshot, principal };
  }

  function capacity(state: OrganizationState, previous?: OrganizationState) {
    preserveOwners(state.members);
    if (
      state.members.length > Math.max(config.maxMembers, previous?.members.length ?? 0) ||
      state.invitations.length > Math.max(config.maxInvitations, previous?.invitations.length ?? 0)
    )
      fail("CAPACITY");
    assertSnapshotCapacity(state);
  }

  async function mutate<T>(
    actor: Actor,
    action: OrganizationAction,
    organizationId: string,
    target: SubjectRef | undefined,
    change: (
      state: OrganizationState,
      principal: SubjectRef,
      now: number,
    ) =>
      | Promise<{ value: T; subjects?: SubjectRef[]; validUntil?: number }>
      | { value: T; subjects?: SubjectRef[]; validUntil?: number },
    details?: { invitationId?: string; requestedRole?: Member["role"] },
  ): Promise<MutationResult<T>> {
    for (let attempt = 0; attempt <= config.conflictRetries; attempt++) {
      const snapshot = await load(actor, action, organizationId, target, details);
      const next = structuredClone(snapshot.state);
      const result = await change(next, snapshot.principal, snapshot.observedAt);
      const expected = snapshot.state.organization.version;
      integer(expected, 1, Number.MAX_SAFE_INTEGER - 1);
      next.organization.version = expected + 1;
      next.organization.updatedAt = snapshot.observedAt;
      capacity(next, snapshot.state);
      // Only a known CAS miss is retried. Storage exceptions keep unknown outcomes.
      if (await store.compareAndSwap(expected, next, result.validUntil)) {
        const changeInfo: MembershipChange = {
          organizationId,
          version: next.organization.version,
          action,
          subjects: structuredClone(result.subjects ?? []),
        };
        return { value: structuredClone(result.value), change: changeInfo };
      }
    }
    return fail("CONFLICT");
  }

  return {
    async createOrganization(
      actor: Actor,
      input: { name: string },
    ): Promise<MutationResult<Organization>> {
      const name = text(object(input, ["name"]).name, 200).trim();
      const principal = await authorize(actor, "organization.create", null, null);
      const now = Date.now();
      const organization: Organization = {
        id: crypto.randomUUID(),
        name,
        version: 1,
        createdAt: now,
        updatedAt: now,
      };
      const state: OrganizationState = {
        organization,
        members: [{ subject: principal, role: "owner", joinedAt: now }],
        invitations: [],
      };
      capacity(state);
      await store.create(state);
      return {
        value: structuredClone(organization),
        change: {
          organizationId: organization.id,
          version: 1,
          action: "organization.create",
          subjects: [principal],
        },
      };
    },

    async readOrganization(actor: Actor, input: { organizationId: string }): Promise<Organization> {
      const { organizationId } = scoped(input);
      return structuredClone(
        (await load(actor, "organization.read", organizationId)).state.organization,
      );
    },

    async updateOrganization(actor: Actor, input: { organizationId: string; name: string }) {
      const { organizationId, parsed } = scoped(input, ["name"]);
      const name = text(parsed.name, 200).trim();
      const result = await mutate(
        actor,
        "organization.update",
        organizationId,
        undefined,
        (state) => {
          state.organization.name = name;
          return { value: state.organization };
        },
      );
      return result;
    },

    async listMembers(actor: Actor, input: { organizationId: string }) {
      const { organizationId } = scoped(input);
      const { state } = await load(actor, "member.list", organizationId);
      return {
        items: structuredClone(state.members),
        total: state.members.length,
        version: state.organization.version,
      };
    },

    async addMember(actor: Actor, input: { organizationId: string; subject: SubjectRef }) {
      const { organizationId, parsed } = scoped(input, ["subject"]);
      const target = subject(parsed.subject);
      return mutate(actor, "member.add", organizationId, target, (state, _principal, now) => {
        if (state.members.some((item) => sameSubject(item.subject, target))) fail("ALREADY_MEMBER");
        const added: Member = { subject: target, role: "member", joinedAt: now };
        state.members.push(added);
        return { value: added, subjects: [target] };
      });
    },

    async leaveOrganization(actor: Actor, input: { organizationId: string }) {
      const { organizationId } = scoped(input);
      return mutate(actor, "member.leave", organizationId, undefined, (state, principal, now) => {
        member(state, principal);
        state.members = state.members.filter((item) => !sameSubject(item.subject, principal));
        preserveOwners(state.members);
        revokePendingFor(state, principal, now);
        return { value: true, subjects: [principal] };
      });
    },

    async removeMember(actor: Actor, input: { organizationId: string; subject: SubjectRef }) {
      const { organizationId, parsed } = scoped(input, ["subject"]);
      const target = subject(parsed.subject);
      return mutate(actor, "member.remove", organizationId, target, (state, _principal, now) => {
        member(state, target);
        state.members = state.members.filter((item) => !sameSubject(item.subject, target));
        preserveOwners(state.members);
        revokePendingFor(state, target, now);
        return { value: true, subjects: [target] };
      });
    },

    async setMemberRole(
      actor: Actor,
      input: { organizationId: string; subject: SubjectRef; role: "member" | "admin" },
    ) {
      const { organizationId, parsed } = scoped(input, ["subject", "role"]);
      const target = subject(parsed.subject);
      if (parsed.role !== "member" && parsed.role !== "admin") fail("INVALID_INPUT");
      const role = parsed.role;
      return mutate(
        actor,
        "member.role",
        organizationId,
        target,
        (state, principal) => {
          const item = member(state, target);
          if (sameSubject(principal, target) && item.role === "member" && role === "admin")
            fail("FORBIDDEN");
          item.role = role;
          preserveOwners(state.members);
          return { value: item, subjects: [target] };
        },
        { requestedRole: role },
      );
    },

    async grantOwner(actor: Actor, input: { organizationId: string; subject: SubjectRef }) {
      const { organizationId, parsed } = scoped(input, ["subject"]);
      const target = subject(parsed.subject);
      return mutate(
        actor,
        "owner.grant",
        organizationId,
        target,
        (state, principal) => {
          if (sameSubject(principal, target)) fail("FORBIDDEN");
          const item = member(state, target);
          item.role = "owner";
          return { value: item, subjects: [target] };
        },
        { requestedRole: "owner" },
      );
    },

    async transferOwner(actor: Actor, input: { organizationId: string; subject: SubjectRef }) {
      const { organizationId, parsed } = scoped(input, ["subject"]);
      const target = subject(parsed.subject);
      return mutate(
        actor,
        "owner.transfer",
        organizationId,
        target,
        (state, principal) => {
          if (sameSubject(principal, target)) fail("INVALID_INPUT");
          const previous = member(state, principal);
          if (previous.role !== "owner") fail("FORBIDDEN");
          const next = member(state, target);
          previous.role = "member";
          next.role = "owner";
          return { value: next, subjects: [principal, target] };
        },
        { requestedRole: "owner" },
      );
    },

    async createInvitation(actor: Actor, input: { organizationId: string; target: SubjectRef }) {
      const { organizationId, parsed } = scoped(input, ["target"]);
      const target = subject(parsed.target);
      const bearer = token();
      const id = crypto.randomUUID();
      return mutate(
        actor,
        "invitation.create",
        organizationId,
        target,
        async (state, principal, now) => {
          if (state.members.some((item) => sameSubject(item.subject, target)))
            fail("ALREADY_MEMBER");
          const invite: StoredInvitation = {
            id,
            organizationId,
            target,
            role: "member",
            createdBy: principal,
            createdAt: now,
            expiresAt: now + config.invitationLifetimeMs,
            status: "pending",
            resolvedAt: null,
            tokenDigest: "",
          };
          invite.tokenDigest = await digest(invite, bearer);
          state.invitations.push(invite);
          return { value: { invitation: present(invite, now), token: bearer } };
        },
      );
    },

    async readInvitation(actor: Actor, input: { organizationId: string; invitationId: string }) {
      const { organizationId, parsed } = scoped(input, ["invitationId"]);
      const id = text(parsed.invitationId, 256);
      const { state, observedAt } = await load(
        actor,
        "invitation.read",
        organizationId,
        undefined,
        { invitationId: id },
      );
      return present(invitation(state, id), observedAt);
    },

    async listInvitations(actor: Actor, input: { organizationId: string }) {
      const { organizationId } = scoped(input);
      const { state, observedAt } = await load(actor, "invitation.list", organizationId);
      return {
        items: state.invitations.map((invite) => present(invite, observedAt)),
        total: state.invitations.length,
        version: state.organization.version,
      };
    },

    async acceptInvitation(
      actor: Actor,
      input: { organizationId: string; invitationId: string; token: string },
    ) {
      const { organizationId, parsed } = scoped(input, ["invitationId", "token"]);
      const id = text(parsed.invitationId, 256);
      const bearer = text(parsed.token, 64);
      if (!/^[a-f0-9]{64}$/.test(bearer)) fail("INVALID_INPUT");
      return mutate(
        actor,
        "invitation.accept",
        organizationId,
        undefined,
        async (state, principal, now) => {
          const invite = invitation(state, id);
          if (
            !sameSubject(invite.target, principal) ||
            !equalDigest(invite.tokenDigest, await digest(invite, bearer))
          )
            fail("NOT_FOUND");
          pending(invite, now);
          if (state.members.some((item) => sameSubject(item.subject, principal)))
            fail("ALREADY_MEMBER");
          const joined: Member = { subject: principal, role: "member", joinedAt: now };
          state.members.push(joined);
          invite.status = "accepted";
          invite.resolvedAt = now;
          return { value: joined, subjects: [principal], validUntil: invite.expiresAt };
        },
        { invitationId: id },
      );
    },

    async revokeInvitation(actor: Actor, input: { organizationId: string; invitationId: string }) {
      const { organizationId, parsed } = scoped(input, ["invitationId"]);
      const id = text(parsed.invitationId, 256);
      return mutate(
        actor,
        "invitation.revoke",
        organizationId,
        undefined,
        (state, _principal, now) => {
          const invite = invitation(state, id);
          pending(invite, now);
          invite.status = "revoked";
          invite.resolvedAt = now;
          return { value: present(invite, now) };
        },
        { invitationId: id },
      );
    },

    async pruneInvitations(actor: Actor, input: { organizationId: string; before: number }) {
      const { organizationId, parsed } = scoped(input, ["before"]);
      const before = integer(parsed.before, 0, Number.MAX_SAFE_INTEGER);
      return mutate(
        actor,
        "invitation.prune",
        organizationId,
        undefined,
        (state, _principal, now) => {
          if (before > now) fail("INVALID_INPUT");
          const previous = state.invitations.length;
          state.invitations = state.invitations.filter(
            (invite) =>
              (invite.status === "pending" ? invite.expiresAt : invite.resolvedAt!) >= before,
          );
          return { value: previous - state.invitations.length };
        },
      );
    },
  };
}

export type OrganizationService<Actor> = ReturnType<typeof createOrganizationService<Actor>>;
