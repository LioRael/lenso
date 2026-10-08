export interface AuditScope {
  readonly tenantId: string | null;
  readonly scopeId: string;
}

export type AuditActor =
  | {
      readonly kind: "user" | "guest" | "service";
      readonly realmId: string;
      readonly subjectId: string;
    }
  | { readonly kind: "system"; readonly systemId: string };

export type AuditResult = "intent" | "success" | "failure" | "denied" | "unknown";
export type SummaryValue = boolean | number | string;
export type SummaryRule =
  | { readonly type: "boolean" }
  | { readonly type: "integer"; readonly min: number; readonly max: number }
  | { readonly type: "enum"; readonly values: readonly string[] };
export type SummaryPolicy = Readonly<Record<string, Readonly<Record<string, SummaryRule>>>>;

export interface AuditInput {
  readonly id: string;
  readonly occurredAt: number;
  readonly scope: AuditScope;
  readonly action: string;
  readonly target: { readonly type: string; readonly id: string };
  readonly result: AuditResult;
  readonly reasonCode: string;
  readonly correlationId?: string;
  readonly summary?: Readonly<Record<string, SummaryValue>>;
  readonly relation?: { readonly kind: "correction" | "outcome"; readonly eventId: string };
}

export interface AuditEvent extends AuditInput {
  readonly recordedAt: number;
  readonly actor: AuditActor;
  readonly summary: Readonly<Record<string, SummaryValue>>;
}

export interface AuditCursor {
  readonly recordedAt: number;
  readonly id: string;
}

export interface AuditQuery {
  readonly scope: AuditScope;
  readonly limit?: number;
  readonly cursor?: AuditCursor;
  readonly action?: string;
  readonly target?: { readonly type: string; readonly id: string };
  readonly result?: AuditResult;
  readonly correlationId?: string;
  readonly recordedFrom?: number;
  readonly recordedTo?: number;
}

export interface AuditPage {
  readonly events: readonly AuditEvent[];
  readonly nextCursor: AuditCursor | null;
}

/** Trusted repository boundary. No mutation or global lookup is exposed. */
export interface AuditRepository {
  readonly durableIntents: boolean;
  insert(event: AuditEvent): Promise<"inserted" | "duplicate" | "conflict">;
  get(scope: AuditScope, id: string): Promise<AuditEvent | null>;
  list(query: AuditQuery & { readonly limit: number }): Promise<readonly AuditEvent[]>;
}

export interface AuditAuthority<P> {
  /** Authenticate/revalidate and authorize the exact scope, then return only an identity snapshot. */
  resolve(principal: P, scope: AuditScope, operation: "append" | "query"): Promise<AuditActor>;
}

export interface AuditDiagnostic {
  readonly mode: "strict" | "best-effort";
  readonly stage: "intent" | "append" | "outcome";
  readonly code: "storage-failed";
}

export type AuditReporter = (diagnostic: AuditDiagnostic) => void | Promise<void>;

export class AuditError extends Error {
  constructor(
    readonly code:
      | "invalid-input"
      | "unauthorized"
      | "duplicate-conflict"
      | "relation-missing"
      | "storage-failed"
      | "strict-unavailable"
      | "invalid-receipt"
      | "diagnostics-failed"
      | "diagnostics-required",
  ) {
    super(`Audit ${code}`);
    this.name = "AuditError";
  }
}

export class AuditOutcomeUnknownError extends Error {
  readonly code = "outcome-unknown";
  constructor(
    readonly intentId: string,
    readonly diagnosticsFailed = false,
  ) {
    super("Effect may have occurred; audit outcome is unknown and requires reconciliation");
    this.name = "AuditOutcomeUnknownError";
  }
}

export function sameScope(a: AuditScope, b: AuditScope): boolean {
  return a.tenantId === b.tenantId && a.scopeId === b.scopeId;
}

/** Recorded time is storage metadata, not part of an event's idempotency content. */
export function sameEvent(a: AuditEvent, b: AuditEvent): boolean {
  const content = (event: AuditEvent) => ({
    id: event.id,
    occurredAt: event.occurredAt,
    scope: { tenantId: event.scope.tenantId, scopeId: event.scope.scopeId },
    actor:
      event.actor.kind === "system"
        ? { kind: event.actor.kind, systemId: event.actor.systemId }
        : {
            kind: event.actor.kind,
            realmId: event.actor.realmId,
            subjectId: event.actor.subjectId,
          },
    action: event.action,
    target: { type: event.target.type, id: event.target.id },
    result: event.result,
    reasonCode: event.reasonCode,
    correlationId: event.correlationId ?? null,
    summary: Object.fromEntries(
      Object.entries(event.summary).sort(([left], [right]) => left.localeCompare(right)),
    ),
    relation: event.relation
      ? { kind: event.relation.kind, eventId: event.relation.eventId }
      : null,
  });
  return JSON.stringify(content(a)) === JSON.stringify(content(b));
}
