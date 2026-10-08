import { OrganizationError, type AccessRequest, type Member, type SubjectRef } from "./contracts";

export function sameSubject(a: SubjectRef, b: SubjectRef): boolean {
  return a.realmId === b.realmId && a.subjectId === b.subjectId;
}

/**
 * Opt-in business relationship policy, never Console/platform admission.
 * Caller must authenticate the subject first. No role implies permission unless
 * the application chooses this policy or another explicit authorization policy.
 */
export function memberRelationshipPolicy(subject: SubjectRef, request: AccessRequest): boolean {
  if (request.action === "organization.create") return true;
  if (!request.organization) return false;
  if (request.action === "invitation.accept") return true;
  const member = request.members.find((item) => sameSubject(item.subject, subject));
  if (!member) return false;
  if (request.action === "member.leave") return true;
  if (request.action === "organization.read" || request.action === "member.list") return true;
  if (
    request.action === "owner.transfer" ||
    request.action === "owner.grant" ||
    request.action === "member.role"
  ) {
    return member.role === "owner";
  }
  if (request.target) {
    const target = request.members.find((item) => sameSubject(item.subject, request.target!));
    if (target && target.role !== "member" && member.role !== "owner") return false;
  }
  return member.role === "owner" || member.role === "admin";
}

export function preserveOwners(members: readonly Member[]): void {
  if (!members.some((member) => member.role === "owner")) {
    throw new OrganizationError("LAST_OWNER");
  }
}
