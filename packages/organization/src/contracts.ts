/** A business subject reference, structurally compatible with Auth.SubjectRef. */
export interface SubjectRef {
  readonly realmId: string;
  readonly subjectId: string;
}

export type MemberRole = "member" | "admin" | "owner";
export interface Member {
  subject: SubjectRef;
  role: MemberRole;
  joinedAt: number;
}

export interface Organization {
  id: string;
  name: string;
  version: number;
  createdAt: number;
  updatedAt: number;
}

export interface Invitation {
  id: string;
  organizationId: string;
  target: SubjectRef;
  role: "member";
  createdBy: SubjectRef;
  createdAt: number;
  expiresAt: number;
  status: "pending" | "accepted" | "revoked" | "expired";
  resolvedAt: number | null;
}

export interface StoredInvitation extends Omit<Invitation, "status"> {
  status: "pending" | "accepted" | "revoked";
  tokenDigest: string;
}

/** One CAS domain: every relationship change increments the organization version. */
export interface OrganizationState {
  organization: Organization;
  members: Member[];
  invitations: StoredInvitation[];
}

export interface OrganizationSnapshot {
  state: OrganizationState;
  /** Storage-authoritative milliseconds, used for invitation expiry. */
  observedAt: number;
}

export interface OrganizationStore {
  create(state: OrganizationState): Promise<void>;
  read(organizationId: string): Promise<OrganizationSnapshot | null>;
  /**
   * Atomically replace only at expectedVersion. If validUntil is supplied, check
   * storage time strictly before it at the write boundary, including lock waits.
   * False means no write. An exception means unknown outcome and MUST NOT retry.
   */
  compareAndSwap(
    expectedVersion: number,
    next: OrganizationState,
    validUntil?: number,
  ): Promise<boolean>;
}

export type OrganizationAction =
  | "organization.create"
  | "organization.read"
  | "organization.update"
  | "member.list"
  | "member.add"
  | "member.remove"
  | "member.leave"
  | "member.role"
  | "owner.grant"
  | "owner.transfer"
  | "invitation.create"
  | "invitation.read"
  | "invitation.list"
  | "invitation.accept"
  | "invitation.revoke"
  | "invitation.prune";

export interface AccessRequest {
  action: OrganizationAction;
  organizationId: string | null;
  /** Never includes invitation digests. Null on create or missing organization. */
  organization: Organization | null;
  members: readonly Member[];
  target?: SubjectRef;
  requestedRole?: MemberRole;
  invitationId?: string;
  /** Actual scoped invitation, stripped of its digest; null when missing. */
  invitation?: Invitation | null;
}

/**
 * Trusted integration boundary, not business JSON. Must revalidate identity,
 * organization scope and action permission each call (including CAS retries).
 * Cross-organization administration is permitted only by an explicit policy here.
 */
export interface OrganizationAccess<Actor> {
  check(actor: Actor, request: AccessRequest): Promise<SubjectRef>;
}

export interface OrganizationConfig {
  invitationLifetimeMs: number;
  maxMembers: number;
  maxInvitations: number;
  conflictRetries: number;
}

export interface MembershipChange {
  organizationId: string;
  version: number;
  action: OrganizationAction;
  subjects: SubjectRef[];
}

export interface MutationResult<T> {
  value: T;
  change: MembershipChange;
}

export type OrganizationErrorCode =
  | "INVALID_INPUT"
  | "FORBIDDEN"
  | "NOT_FOUND"
  | "LAST_OWNER"
  | "ALREADY_MEMBER"
  | "INVITATION_ACCEPTED"
  | "INVITATION_REVOKED"
  | "INVITATION_EXPIRED"
  | "CONFLICT"
  | "CAPACITY";

export class OrganizationError extends Error {
  constructor(readonly code: OrganizationErrorCode) {
    super(code);
    this.name = "OrganizationError";
  }
}
