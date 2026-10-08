import type { ActorKind } from "./source";

/** Auth owns this record, but the referenced subject belongs to the application. */
export interface SessionRecord<S extends string = string> {
  readonly id: string;
  readonly realmId: string;
  readonly subjectId: S;
  readonly kind: ActorKind;
  readonly tokenDigest: string;
  readonly revision: number;
  readonly issuedAt: number;
  readonly expiresAt: number;
  readonly idleTimeoutMs: number;
  readonly renewAfterMs: number;
  readonly lastActiveAt: number;
  readonly renewedAt: number;
  readonly authenticatedAt: number | null;
  readonly assurance: readonly string[];
  readonly revokedAt: number | null;
}

export interface SessionMutation<S extends string = string> {
  readonly kind: "touch" | "renew";
  readonly expectedRevision: number;
  readonly expectedDigest: string;
  readonly now: number;
  readonly next: SessionRecord<S>;
}

export interface SessionStore<S extends string = string> {
  create(record: SessionRecord<S>): Promise<void>;
  /** Read current authoritative committed state, never a stale replica or cache. */
  read(realmId: string, id: string): Promise<SessionRecord<S> | null>;
  /**
   * Atomically check realm/id, revision, digest, revocation, stored AND next expiry/idle,
   * and (for renew) its interval before updating. Use commit-time store clock,
   * no earlier than mutation.now. Return false for stale or no longer live state.
   */
  mutate(mutation: SessionMutation<S>): Promise<boolean>;
  revoke(realmId: string, id: string, at: number): Promise<boolean>;
}
