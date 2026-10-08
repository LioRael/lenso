export interface KeySubject {
  readonly namespace: string;
  readonly tenantId: string;
  readonly subjectId: string;
}

export interface KeyMetadata {
  readonly id: string;
  readonly subject: KeySubject;
  readonly scopes: readonly string[];
  readonly revision: number;
  readonly issuedAt: number;
  readonly expiresAt: number;
  readonly revokedAt: number | null;
  readonly overlapUntil: number | null;
}

/** Storage-only record. Never return this through a business/management entry. */
export interface KeyRecord extends KeyMetadata {
  readonly requestId: string;
  readonly digest: string;
  readonly previousDigest: string | null;
}

export interface KeyRotation {
  readonly subject: KeySubject;
  readonly id: string;
  readonly expectedRevision: number;
  readonly digest: string;
  readonly overlapMs: number;
  readonly now: number;
}

export interface ApiKeyStore {
  /** Insert once per (namespace, tenantId, requestId); replay returns the original record.
   * Enforce unique id and digest. Reject an already expired insertion at store time.
   */
  create(record: KeyRecord): Promise<{ created: boolean; record: KeyRecord }>;
  /** Authoritative committed reads only, including D1 read consistency selection. */
  read(id: string): Promise<KeyRecord | null>;
  /** All three subject fields must match. Bounded pagination ordered by id. */
  list(subject: KeySubject, after: string | null, limit: number): Promise<readonly KeyRecord[]>;
  /** Atomically match all subject fields, id, revision, live expiry, no revocation,
   * and no still-open overlap window. Use store time >= now, after PG row lock.
   * Move digest to previousDigest (null for zero overlap), advance revision, and
   * set overlapUntil to min(expiresAt, store time + overlapMs), or null for zero.
   */
  rotate(input: KeyRotation): Promise<KeyRecord | null>;
  /** Match all subject fields and stable id. Revoke current and previous secrets,
   * including a concurrently rotated successor. Return false if missing.
   */
  revoke(subject: KeySubject, id: string, now: number): Promise<boolean>;
}
