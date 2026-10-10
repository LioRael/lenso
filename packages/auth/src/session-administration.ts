import type { Access, Actor, AuthenticationOptions, Policy, PolicyContext } from "./core";
import { AuthConfigurationError, AuthError } from "./errors";
import type { ActorKind } from "./source";
import type { SessionAdminStore, SessionPosition, SessionRecord } from "./session-store";

export interface SessionDetail {
  readonly id: string;
  readonly realmId: string;
  readonly subjectId: string;
  readonly kind: ActorKind;
  readonly revision: number;
  readonly issuedAt: number;
  readonly expiresAt: number;
  readonly lastActiveAt: number;
  readonly revokedAt: number | null;
}

export type SessionAdministrationResource =
  | { readonly operation: "scope"; readonly action: "list"; readonly realmId: string }
  | {
      readonly operation: "scope";
      readonly action: "get" | "revoke";
      readonly realmId: string;
      readonly sessionId: string;
    }
  | {
      readonly operation: "get" | "revoke";
      readonly realmId: string;
      readonly session: SessionDetail;
    };

export interface SessionPageInput {
  readonly limit?: number;
  readonly cursor?: string;
}

export interface SessionPage {
  readonly sessions: readonly SessionDetail[];
  readonly nextCursor: string | null;
}

export interface SessionAdministration<P> {
  list(
    input: SessionPageInput,
    actor: P | null,
    options?: AuthenticationOptions,
  ): Promise<SessionPage>;
  get(
    input: { readonly id: string },
    actor: P | null,
    options?: AuthenticationOptions,
  ): Promise<SessionDetail | null>;
  revoke(
    input: { readonly id: string; readonly expectedRevision: number },
    actor: P | null,
    options?: AuthenticationOptions,
  ): Promise<{ readonly revoked: boolean; readonly intentId: string }>;
}

export class SessionAdministrationError extends Error {
  constructor(
    readonly code: "invalid-input" | "stale-revision" | "pending-reconciliation",
    readonly intentId?: string,
  ) {
    super(`Session administration ${code}`);
    this.name = "SessionAdministrationError";
  }
}

export class SessionRevokeOutcomeUnknownError extends Error {
  readonly code = "outcome-unknown";
  constructor(
    readonly intentId: string,
    options?: ErrorOptions,
  ) {
    super(
      "Session revocation may have occurred; reconcile the audit intent before retrying",
      options,
    );
    this.name = "SessionRevokeOutcomeUnknownError";
  }
}

/** Structural subset of Audit.prepare/complete; no Auth -> Audit package dependency. */
export interface SessionRevokeAudit<P, Receipt extends { readonly intentId: string }> {
  prepare(
    input: {
      readonly id: string;
      readonly occurredAt: number;
      readonly scope: SessionAuditScope;
      readonly action: string;
      readonly target: { readonly type: string; readonly id: string };
      readonly result: "intent";
      readonly reasonCode: string;
    },
    principal: P,
  ): Promise<
    | { readonly status: "ready"; readonly receipt: Receipt }
    | { readonly status: "already-recorded"; readonly intentId: string }
  >;
  complete(
    receipt: Receipt,
    outcome: {
      readonly id: string;
      readonly occurredAt: number;
      readonly result: "success" | "denied" | "failure" | "unknown";
      readonly reasonCode: string;
    },
  ): Promise<unknown>;
}

export interface SessionAuditScope {
  readonly tenantId: string | null;
  readonly scopeId: string;
}

export interface SessionAdministrationOptions<
  R extends string,
  E,
  S extends string,
  A extends string,
  M,
  Receipt extends { readonly intentId: string },
> {
  /** Target realm is installed code, independent of the caller's authentication realm. */
  readonly realmId: string;
  readonly store: SessionAdminStore;
  readonly access: Access<R, E, S, A, SessionAdministrationResource, M>;
  readonly policy: Policy<PolicyContext<Actor<R, S, A>, SessionAdministrationResource, M>>;
  readonly audit: SessionRevokeAudit<Actor<R, S, A>, Receipt>;
  /** Trusted assembly selects an exact Audit scope; business input cannot supply one. */
  readonly auditScope: SessionAuditScope;
}

const invalid = () => new SessionAdministrationError("invalid-input");
const unavailable = () => new AuthError("SERVICE_UNAVAILABLE");
const time = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
const identifier = (value: unknown): value is string =>
  typeof value === "string" && !!value.trim() && value.length <= 256;

function detail(record: SessionRecord, realmId: string): SessionDetail {
  if (
    !record ||
    !identifier(record.id) ||
    record.realmId !== realmId ||
    typeof record.subjectId !== "string" ||
    !record.subjectId.trim() ||
    record.subjectId.length > 512 ||
    !["user", "guest", "service"].includes(record.kind) ||
    !Number.isSafeInteger(record.revision) ||
    record.revision <= 0 ||
    !time(record.issuedAt) ||
    !time(record.expiresAt) ||
    !time(record.lastActiveAt) ||
    record.issuedAt >= record.expiresAt ||
    record.lastActiveAt < record.issuedAt ||
    record.lastActiveAt > Date.now() ||
    !(record.revokedAt === null || (time(record.revokedAt) && record.revokedAt >= record.issuedAt))
  )
    throw unavailable();
  return Object.freeze({
    id: record.id,
    realmId,
    subjectId: record.subjectId,
    kind: record.kind,
    revision: record.revision,
    issuedAt: record.issuedAt,
    expiresAt: record.expiresAt,
    lastActiveAt: record.lastActiveAt,
    revokedAt: record.revokedAt,
  });
}

function strictInput(input: object, keys: readonly string[]): void {
  if (
    !input ||
    typeof input !== "object" ||
    Array.isArray(input) ||
    Object.keys(input).some((key) => !keys.includes(key))
  )
    throw invalid();
}

export function createSessionAdministration<
  R extends string,
  E,
  S extends string,
  A extends string,
  M,
  Receipt extends { readonly intentId: string },
>(
  options: SessionAdministrationOptions<R, E, S, A, M, Receipt>,
): SessionAdministration<Actor<R, S, A>> {
  const { realmId, store, access, policy, audit } = options;
  if (
    !identifier(realmId) ||
    typeof store.page !== "function" ||
    typeof store.revokeRevision !== "function" ||
    typeof audit?.prepare !== "function" ||
    typeof audit?.complete !== "function" ||
    typeof policy !== "function"
  ) {
    throw new AuthConfigurationError(
      "Session administration requires an explicit policy and capable store",
    );
  }
  const auditScope = Object.freeze({
    tenantId: options.auditScope.tenantId,
    scopeId: options.auditScope.scopeId,
  });
  const namespace = crypto.randomUUID();
  const listScope = Object.freeze({
    operation: "scope" as const,
    action: "list" as const,
    realmId,
  });

  async function enforce(
    actor: Actor<R, S, A> | null,
    resource: SessionAdministrationResource,
    opts: AuthenticationOptions,
  ) {
    return access.enforce(actor, resource, policy, opts);
  }

  async function read(id: string, opts: AuthenticationOptions): Promise<SessionDetail | null> {
    opts.signal?.throwIfAborted();
    let record;
    try {
      record = await store.read(realmId, id);
    } catch (error) {
      opts.signal?.throwIfAborted();
      throw new AuthError("SERVICE_UNAVAILABLE", { cause: error });
    }
    opts.signal?.throwIfAborted();
    if (record === null) return null;
    const result = detail(record, realmId);
    if (result.id !== id) throw unavailable();
    return result;
  }

  async function authorizedDetail(
    id: string,
    actor: Actor<R, S, A> | null,
    operation: "get" | "revoke",
    opts: AuthenticationOptions,
  ) {
    const session = await read(id, opts);
    if (session === null) {
      await enforce(
        actor,
        Object.freeze({
          operation: "scope",
          action: operation,
          realmId,
          sessionId: id,
        }),
        opts,
      );
      return null;
    }
    await enforce(actor, Object.freeze({ operation, realmId, session }), opts);
    // A slow membership/policy read must not return or mutate a newer target revision.
    return recheck(session, opts);
  }

  async function recheck(session: SessionDetail, opts: AuthenticationOptions) {
    const current = await read(session.id, opts);
    if (!current || JSON.stringify(current) !== JSON.stringify(session)) {
      throw new SessionAdministrationError("stale-revision");
    }
    return current;
  }

  function decodeCursor(cursor?: string): SessionPosition | undefined {
    if (cursor === undefined) return undefined;
    if (typeof cursor !== "string" || cursor.length > 2048) throw invalid();
    try {
      const value = JSON.parse(cursor);
      if (
        !Array.isArray(value) ||
        value.length !== 5 ||
        value[0] !== 1 ||
        value[1] !== namespace ||
        value[2] !== realmId ||
        !time(value[3]) ||
        !identifier(value[4])
      )
        throw invalid();
      return { issuedAt: value[3], id: value[4] };
    } catch {
      throw invalid();
    }
  }

  return Object.freeze({
    async list(
      input: SessionPageInput,
      actor: Actor<R, S, A> | null,
      opts: AuthenticationOptions = {},
    ): Promise<SessionPage> {
      strictInput(input, ["limit", "cursor"]);
      const limit = input.limit ?? 50;
      if (!Number.isSafeInteger(limit) || limit < 1 || limit > 100) throw invalid();
      const before = decodeCursor(input.cursor);
      await enforce(actor, listScope, opts);
      let rows;
      try {
        rows = await store.page(realmId, limit + 1, before);
      } catch (error) {
        opts.signal?.throwIfAborted();
        throw new AuthError("SERVICE_UNAVAILABLE", { cause: error });
      }
      opts.signal?.throwIfAborted();
      await enforce(actor, listScope, opts);
      if (!Array.isArray(rows) || rows.length > limit + 1) throw unavailable();
      let previous = before;
      const projected = rows.map((row) => {
        const session = detail(row, realmId);
        if (
          previous &&
          !(
            session.issuedAt < previous.issuedAt ||
            (session.issuedAt === previous.issuedAt && session.id < previous.id)
          )
        )
          throw unavailable();
        previous = session;
        return session;
      });
      const sessions = projected.slice(0, limit);
      for (const session of sessions) {
        await enforce(actor, Object.freeze({ operation: "get", realmId, session }), opts);
      }
      await enforce(actor, listScope, opts);
      for (const session of sessions) await recheck(session, opts);
      const last = sessions.at(-1);
      return {
        sessions,
        nextCursor:
          projected.length > limit && last
            ? JSON.stringify([1, namespace, realmId, last.issuedAt, last.id])
            : null,
      };
    },
    async get(
      input: { readonly id: string },
      actor: Actor<R, S, A> | null,
      opts: AuthenticationOptions = {},
    ) {
      strictInput(input, ["id"]);
      const id = input.id;
      if (!identifier(id)) throw invalid();
      await enforce(
        actor,
        Object.freeze({
          operation: "scope",
          action: "get",
          realmId,
          sessionId: id,
        }),
        opts,
      );
      return authorizedDetail(id, actor, "get", opts);
    },
    async revoke(
      input: { readonly id: string; readonly expectedRevision: number },
      actor: Actor<R, S, A> | null,
      opts: AuthenticationOptions = {},
    ): Promise<{ readonly revoked: boolean; readonly intentId: string }> {
      strictInput(input, ["id", "expectedRevision"]);
      const { id, expectedRevision } = input;
      if (
        !identifier(id) ||
        !Number.isSafeInteger(expectedRevision) ||
        expectedRevision < 1 ||
        expectedRevision >= Number.MAX_SAFE_INTEGER
      )
        throw invalid();
      await enforce(
        actor,
        Object.freeze({
          operation: "scope",
          action: "revoke",
          realmId,
          sessionId: id,
        }),
        opts,
      );
      const session = await authorizedDetail(id, actor, "revoke", opts);
      if (!session || session.revision !== expectedRevision || session.revokedAt !== null) {
        throw new SessionAdministrationError("stale-revision");
      }
      const prepared = await audit.prepare(
        {
          id: crypto.randomUUID(),
          occurredAt: Date.now(),
          scope: auditScope,
          action: "auth.session.revoke",
          target: { type: "auth-session", id: session.id },
          result: "intent",
          reasonCode: "requested",
        },
        actor!,
      );
      if (prepared.status !== "ready") {
        throw new SessionAdministrationError("pending-reconciliation", prepared.intentId);
      }
      let changed = false;
      let failure: unknown;
      let result: "success" | "denied" | "failure" | "unknown" = "unknown";
      let reasonCode = "write-unconfirmed";
      try {
        await enforce(
          actor,
          Object.freeze({
            operation: "scope",
            action: "revoke",
            realmId,
            sessionId: id,
          }),
          opts,
        );
        await enforce(actor, Object.freeze({ operation: "revoke", realmId, session }), opts);
        opts.signal?.throwIfAborted();
      } catch (error) {
        failure = error;
        const denied =
          error instanceof AuthError &&
          (error.code === "UNAUTHORIZED" ||
            error.code === "FORBIDDEN" ||
            error.code === "REAUTHENTICATION_REQUIRED");
        result = denied ? "denied" : "failure";
        reasonCode = denied
          ? "authorization-changed"
          : opts.signal?.aborted
            ? "request-cancelled"
            : "authorization-unavailable";
      }
      if (failure === undefined) {
        try {
          const outcome = await store.revokeRevision(
            realmId,
            session.id,
            expectedRevision,
            Date.now(),
          );
          if (outcome !== true && outcome !== false) throw unavailable();
          changed = outcome;
          result = changed ? "success" : "failure";
          reasonCode = changed ? "revoked" : "stale-revision";
        } catch (error) {
          failure = new SessionRevokeOutcomeUnknownError(prepared.receipt.intentId, {
            cause: error,
          });
        }
      }
      // Receipt completion must survive caller cancellation and self-revocation.
      await audit.complete(prepared.receipt, {
        id: crypto.randomUUID(),
        occurredAt: Date.now(),
        result,
        reasonCode,
      });
      if (failure !== undefined) throw failure;
      if (!changed) throw new SessionAdministrationError("stale-revision");
      return { revoked: true, intentId: prepared.receipt.intentId };
    },
  });
}
