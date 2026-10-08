import type { OrganizationSnapshot, OrganizationState, SubjectRef } from "../contracts";
import { assertSnapshotCapacity } from "../capacity";

export function assertValidVersion(value: number, label: string): void {
  if (!Number.isSafeInteger(value) || value < 1) {
    throw new Error(`Invalid organization ${label}`);
  }
}

export function validateState(state: OrganizationState): void {
  if (
    !state ||
    typeof state !== "object" ||
    !state.organization ||
    !validText(state.organization.id, 256) ||
    !validText(state.organization.name, 200) ||
    !validTime(state.organization.createdAt) ||
    !validTime(state.organization.updatedAt)
  ) {
    throw new Error("Invalid persisted organization state");
  }
  assertValidVersion(state.organization.version, "state version");
  if (
    !Array.isArray(state.members) ||
    !Array.isArray(state.invitations) ||
    state.members.length > 1000 ||
    state.invitations.length > 1000 ||
    !state.members.some((member) => member?.role === "owner")
  ) {
    throw new Error("Invalid persisted organization state");
  }
  const subjects = new Set<string>();
  for (const member of state.members) {
    if (
      !member ||
      !validSubject(member.subject) ||
      !["member", "admin", "owner"].includes(member.role) ||
      !validTime(member.joinedAt)
    )
      throw new Error("Invalid persisted organization member");
    const key = JSON.stringify([member.subject.realmId, member.subject.subjectId]);
    if (subjects.has(key)) throw new Error("Duplicate persisted organization member");
    subjects.add(key);
  }
  const invitations = new Set<string>();
  for (const invite of state.invitations) {
    if (
      !invite ||
      !validText(invite.id, 256) ||
      invite.organizationId !== state.organization.id ||
      !validSubject(invite.target) ||
      !validSubject(invite.createdBy) ||
      invite.role !== "member" ||
      !validTime(invite.createdAt) ||
      !validTime(invite.expiresAt) ||
      invite.expiresAt <= invite.createdAt ||
      !["pending", "accepted", "revoked"].includes(invite.status) ||
      (invite.status === "pending" ? invite.resolvedAt !== null : !validTime(invite.resolvedAt)) ||
      typeof invite.tokenDigest !== "string" ||
      !/^[a-f0-9]{64}$/.test(invite.tokenDigest) ||
      invitations.has(invite.id)
    )
      throw new Error("Invalid persisted organization invitation");
    invitations.add(invite.id);
  }
}

function validText(value: unknown, max: number): value is string {
  return typeof value === "string" && !!value.trim() && value.length <= max;
}

function validTime(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function validSubject(value: SubjectRef): boolean {
  return !!value && validText(value.realmId, 256) && validText(value.subjectId, 512);
}

export function encodeState(state: OrganizationState): string {
  validateState(state);
  assertSnapshotCapacity(state);
  return JSON.stringify(state);
}

export function decodeSnapshot(
  id: string,
  version: number,
  encoded: string,
  observedAt: number,
): OrganizationSnapshot {
  assertValidVersion(version, "row version");
  let state: unknown;
  try {
    state = JSON.parse(encoded);
  } catch {
    throw new Error("Invalid persisted organization state");
  }
  validateState(state as OrganizationState);
  const decoded = state as OrganizationState;
  if (decoded.organization.id !== id || decoded.organization.version !== version) {
    throw new Error("Organization row and state are inconsistent");
  }
  if (!validTime(observedAt)) throw new Error("Invalid organization storage clock");
  return { state: decoded, observedAt };
}
