import assert from "node:assert/strict";
import {
  OrganizationError,
  type OrganizationAccess,
  type OrganizationStore,
  type SubjectRef,
} from "../src/contracts";
import { memberRelationshipPolicy } from "../src/policy";
import { createOrganizationService } from "../src/service";

export function actors() {
  const identities = new WeakMap<object, SubjectRef>();
  const actor = (subjectId: string) => {
    const handle = Object.freeze({});
    identities.set(handle, { realmId: "backend-test", subjectId });
    return handle;
  };
  const subject = (handle: object) => identities.get(handle)!;
  // Trusted dummy identities exercise business policy, not real Auth verification.
  const access: OrganizationAccess<object> = {
    async check(handle, request) {
      const principal = identities.get(handle);
      if (!principal || !memberRelationshipPolicy(principal, request)) {
        throw new OrganizationError("FORBIDDEN");
      }
      return principal;
    },
  };
  return { actor, subject, access };
}

export async function poll(label: string, check: () => Promise<boolean>, timeout = 5_000) {
  const deadline = Date.now() + timeout;
  do {
    if (await check()) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  } while (Date.now() < deadline);
  throw new Error(`Timed out waiting for ${label}`);
}

function oneWinner(results: PromiseSettledResult<unknown>[], code: string) {
  assert.equal(results.filter((result) => result.status === "fulfilled").length, 1);
  const loser = results.find((result) => result.status === "rejected");
  assert.ok(loser && loser.status === "rejected");
  assert.equal(loser.reason.code, code);
}

/** Both calls see the same real backend snapshot before either can write. */
function raceStores(stores: readonly [OrganizationStore, OrganizationStore]) {
  let arrived = 0;
  let release!: () => void;
  const barrier = new Promise<void>((resolve) => {
    release = resolve;
  });
  let successes = 0;
  const wrapped = stores.map((store): OrganizationStore => {
    let first = true;
    return {
      create: (state) => store.create(state),
      async read(id) {
        const snapshot = await store.read(id);
        if (first) {
          first = false;
          if (++arrived === 2) release();
          await barrier;
        }
        return snapshot;
      },
      async compareAndSwap(version, next, validUntil) {
        assert.ok(next.members.some((member) => member.role === "owner"));
        const won = await store.compareAndSwap(version, next, validUntil);
        if (won) successes++;
        return won;
      },
    };
  });
  return { stores: wrapped, successes: () => successes };
}

export async function backendContract(
  stores: readonly [OrganizationStore, OrganizationStore],
  persistedRow: (id: string) => Promise<unknown>,
) {
  const { actor, subject, access } = actors();
  const a = actor("alice");
  const b = actor("bob");
  const c = actor("carol");
  const service = createOrganizationService({ store: stores[0], access });
  for (const action of ["demote", "remove", "leave"] as const) {
    const { value: org } = await service.createOrganization(a, { name: action });
    const scope = { organizationId: org.id };
    assert.equal(org.version, 1);
    assert.equal((await service.addMember(a, { ...scope, subject: subject(b) })).change.version, 2);
    assert.equal(
      (await service.grantOwner(a, { ...scope, subject: subject(b) })).change.version,
      3,
    );
    const race = raceStores(stores);
    const [first, second] = race.stores.map((store) =>
      createOrganizationService({ store, access }),
    );
    const change = (s: typeof service, who: object) => {
      if (action === "demote") {
        return s.setMemberRole(who, { ...scope, subject: subject(who), role: "member" });
      }
      if (action === "remove") return s.removeMember(who, { ...scope, subject: subject(who) });
      return s.leaveOrganization(who, scope);
    };
    oneWinner(await Promise.allSettled([change(first!, a), change(second!, b)]), "LAST_OWNER");
    assert.equal(race.successes(), 1);
    const final = (await stores[0].read(org.id))!.state;
    assert.equal(final.organization.version, 4);
    assert.equal(final.members.filter((member) => member.role === "owner").length, 1);
  }

  const { value: transferOrg } = await service.createOrganization(a, { name: "transfer" });
  const transferScope = { organizationId: transferOrg.id };
  await service.addMember(a, { ...transferScope, subject: subject(b) });
  await service.addMember(a, { ...transferScope, subject: subject(c) });
  const transferRace = raceStores(stores);
  const [t1, t2] = transferRace.stores.map((store) => createOrganizationService({ store, access }));
  oneWinner(
    await Promise.allSettled([
      t1!.transferOwner(a, { ...transferScope, subject: subject(b) }),
      t2!.transferOwner(a, { ...transferScope, subject: subject(c) }),
    ]),
    "FORBIDDEN",
  );
  assert.equal(transferRace.successes(), 1);
  const transferred = (await stores[0].read(transferOrg.id))!.state;
  assert.equal(transferred.organization.version, 4);
  assert.equal(transferred.members.filter((member) => member.role === "owner").length, 1);
  assert.equal(
    transferred.members.find((member) => member.subject.subjectId === "alice")!.role,
    "member",
  );

  const { value: org } = await service.createOrganization(a, { name: "invitations" });
  const scope = { organizationId: org.id };
  const issued = await service.createInvitation(a, { ...scope, target: subject(b) });
  const input = { ...scope, invitationId: issued.value.invitation.id, token: issued.value.token };
  const before = (await stores[0].read(org.id))!.state;
  assert.equal(before.organization.version, 2);
  assert.match(before.invitations[0]!.tokenDigest, /^[a-f0-9]{64}$/);
  assert.notEqual(before.invitations[0]!.tokenDigest, input.token);
  assert.equal(JSON.stringify(await persistedRow(org.id)).includes(input.token), false);
  assert.equal("tokenDigest" in issued.value.invitation, false);

  const other = (await service.createOrganization(a, { name: "other organization" })).value;
  const otherInvite = await service.createInvitation(a, {
    organizationId: other.id,
    target: subject(b),
  });
  await assert.rejects(service.acceptInvitation(b, { ...input, organizationId: other.id }), {
    code: "NOT_FOUND",
  });
  await assert.rejects(service.acceptInvitation(b, { ...input, token: otherInvite.value.token }), {
    code: "NOT_FOUND",
  });
  await assert.rejects(service.acceptInvitation(c, input), { code: "NOT_FOUND" });
  assert.deepEqual((await stores[0].read(org.id))!.state, before);

  const acceptRace = raceStores(stores);
  const [i1, i2] = acceptRace.stores.map((store) => createOrganizationService({ store, access }));
  oneWinner(
    await Promise.allSettled([i1!.acceptInvitation(b, input), i2!.acceptInvitation(b, input)]),
    "INVITATION_ACCEPTED",
  );
  assert.equal(acceptRace.successes(), 1);
  assert.equal((await stores[0].read(org.id))!.state.organization.version, 3);
  await assert.rejects(service.acceptInvitation(b, input), { code: "INVITATION_ACCEPTED" });
  assert.equal((await service.listMembers(b, scope)).total, 2);
  await service.removeMember(a, { ...scope, subject: subject(b) });
  for (const read of [
    () => service.readOrganization(b, scope),
    () => service.listMembers(b, scope),
    () => service.listInvitations(b, scope),
    () => service.readInvitation(b, { ...scope, invitationId: input.invitationId }),
  ])
    await assert.rejects(read(), { code: "FORBIDDEN" });
  await assert.rejects(service.acceptInvitation(b, input), { code: "INVITATION_ACCEPTED" });

  const revoked = await service.createInvitation(a, { ...scope, target: subject(c) });
  await service.revokeInvitation(a, { ...scope, invitationId: revoked.value.invitation.id });
  const revokedState = (await stores[0].read(org.id))!.state;
  await assert.rejects(
    service.acceptInvitation(c, {
      ...scope,
      invitationId: revoked.value.invitation.id,
      token: revoked.value.token,
    }),
    { code: "INVITATION_REVOKED" },
  );
  assert.deepEqual((await stores[0].read(org.id))!.state, revokedState);

  const short = createOrganizationService({
    store: stores[0],
    access,
    config: { invitationLifetimeMs: 1000 },
  });
  const expired = await short.createInvitation(a, { ...scope, target: subject(b) });
  await poll(
    "invitation expiry at storage clock",
    async () => (await stores[0].read(org.id))!.observedAt >= expired.value.invitation.expiresAt,
  );
  const expiredState = (await stores[0].read(org.id))!.state;
  const expiredInput = {
    ...scope,
    invitationId: expired.value.invitation.id,
    token: expired.value.token,
  };
  await assert.rejects(service.acceptInvitation(b, expiredInput), { code: "INVITATION_EXPIRED" });
  await assert.rejects(service.acceptInvitation(b, expiredInput), { code: "INVITATION_EXPIRED" });
  assert.deepEqual((await stores[0].read(org.id))!.state, expiredState);
  assert.equal(
    (
      await service.readInvitation(a, {
        ...scope,
        invitationId: expiredInput.invitationId,
      })
    ).status,
    "expired",
  );

  await service.addMember(a, { ...scope, subject: subject(c) });
  await service.setMemberRole(a, { ...scope, subject: subject(c), role: "admin" });
  await service.addMember(a, { ...scope, subject: subject(b) });
  const adminState = (await stores[0].read(org.id))!.state;
  await assert.rejects(service.grantOwner(c, { ...scope, subject: subject(b) }), {
    code: "FORBIDDEN",
  });
  await assert.rejects(
    service.setMemberRole(c, { ...scope, subject: subject(a), role: "member" }),
    { code: "FORBIDDEN" },
  );
  assert.deepEqual((await stores[0].read(org.id))!.state, adminState);

  const nextA = structuredClone(adminState);
  const nextB = structuredClone(adminState);
  nextA.organization.version++;
  nextB.organization.version++;
  nextA.organization.name = "CAS A";
  nextB.organization.name = "CAS B";
  assert.equal(
    await stores[0].compareAndSwap(
      adminState.organization.version,
      nextA,
      (await stores[0].read(org.id))!.observedAt - 1,
    ),
    false,
  );
  assert.deepEqual((await stores[0].read(org.id))!.state, adminState);
  const raw = await Promise.all([
    stores[0].compareAndSwap(adminState.organization.version, nextA),
    stores[1].compareAndSwap(adminState.organization.version, nextB),
  ]);
  assert.deepEqual(raw.slice().sort(), [false, true]);
  const rawWinner = (await stores[0].read(org.id))!.state;
  assert.deepEqual(rawWinner, raw[0] ? nextA : nextB);
  assert.equal(await stores[1].compareAndSwap(adminState.organization.version, nextB), false);
  assert.deepEqual((await stores[0].read(org.id))!.state, rawWinner);
}
