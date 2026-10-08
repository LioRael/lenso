import type { Access, Actor, MembershipReader } from "@lenso/auth";
import type { AccessRequest, MemberRole, OrganizationAccess, OrganizationStore } from "./contracts";
import { sameSubject } from "./policy";

/**
 * Borrow the application's audience-bound Auth access. Policy is required:
 * authentication and organization membership are not authorization by themselves.
 */
export function createAuthOrganizationAccess<
  R extends string,
  Evidence,
  S extends string,
  A extends string,
>(options: {
  access: Access<R, Evidence, S, A>;
  policy(principal: Actor<R, S, A>, request: AccessRequest): boolean | Promise<boolean>;
}): OrganizationAccess<Actor<R, S, A> | null> {
  return {
    async check(actor, request) {
      const verified = await options.access.enforce(actor, request, ({ principal, resource }) =>
        options.policy(principal, resource),
      );
      return { realmId: verified.realmId, subjectId: verified.subjectId };
    },
  };
}

export interface OrganizationMembership {
  role: MemberRole;
  version: number;
}

/**
 * Trusted internal reader for Auth.Access.memberships. No Console admission and
 * no permission grant. Cache keys must include organization, realm and subject;
 * consumers must revalidate versions or explicitly invalidate after each change.
 */
export function organizationMembershipReader(
  store: OrganizationStore,
): MembershipReader<string, string, { organizationId: string }, OrganizationMembership> {
  return async (subject, resource, context) => {
    context.signal.throwIfAborted();
    const snapshot = await store.read(resource.organizationId);
    context.signal.throwIfAborted();
    const relationship = snapshot?.state.members.find((item) => sameSubject(item.subject, subject));
    return relationship && snapshot
      ? { role: relationship.role, version: snapshot.state.organization.version }
      : null;
  };
}
