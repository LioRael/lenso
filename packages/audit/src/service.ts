import {
  AuditError,
  AuditOutcomeUnknownError,
  sameScope,
  type AuditAuthority,
  type AuditDiagnostic,
  type AuditEvent,
  type AuditInput,
  type AuditPage,
  type AuditQuery,
  type AuditReporter,
  type AuditRepository,
  type AuditScope,
  type SummaryPolicy,
} from "./contracts";
import * as validate from "./validation";

declare const receiptBrand: unique symbol;
export interface AuditIntentReceipt {
  readonly intentId: string;
  readonly [receiptBrand]: true;
}

export interface AuditOutcome {
  readonly id: string;
  readonly occurredAt: number;
  readonly result: "success" | "failure" | "denied" | "unknown";
  readonly reasonCode: string;
  readonly summary?: AuditInput["summary"];
}

export interface AuditService<P> {
  append(
    input: AuditInput,
    principal: P,
  ): Promise<{ status: "inserted" | "duplicate"; event: AuditEvent }>;
  appendBestEffort(
    input: AuditInput,
    principal: P,
  ): Promise<
    | { status: "inserted" | "duplicate"; event: AuditEvent }
    | { status: "unconfirmed"; code: "storage-failed" }
  >;
  get(input: { scope: AuditScope; id: string }, principal: P): Promise<AuditEvent | null>;
  query(input: AuditQuery, principal: P): Promise<AuditPage>;
  prepare(
    input: AuditInput,
    principal: P,
  ): Promise<
    | { status: "ready"; receipt: AuditIntentReceipt }
    | { status: "already-recorded"; intentId: string }
  >;
  complete(receipt: AuditIntentReceipt, outcome: AuditOutcome): Promise<AuditEvent>;
}

export interface AuditServiceOptions<P> {
  repository: AuditRepository;
  authority: AuditAuthority<P>;
  summaryPolicy?: SummaryPolicy;
  maxPageSize?: number;
  report?: AuditReporter;
  clock?: () => number;
}

export function createAuditService<P>(options: AuditServiceOptions<P>): AuditService<P> {
  const { repository, authority, report } = options;
  const policy = validate.summaryPolicy(options.summaryPolicy ?? {});
  const maxPageSize = options.maxPageSize ?? 100;
  if (!Number.isSafeInteger(maxPageSize) || maxPageSize < 1 || maxPageSize > 500)
    validate.invalid();
  const clock = options.clock ?? Date.now;
  const receipts = new WeakMap<AuditIntentReceipt, AuditEvent>();

  async function identity(principal: P, scope: AuditScope, operation: "append" | "query") {
    try {
      return validate.actor(await authority.resolve(principal, scope, operation));
    } catch {
      throw new AuditError("unauthorized");
    }
  }

  async function diagnostic(mode: AuditDiagnostic["mode"], stage: AuditDiagnostic["stage"]) {
    try {
      await report?.(Object.freeze({ mode, stage, code: "storage-failed" }));
    } catch {
      throw new AuditError("diagnostics-failed");
    }
  }

  async function storage<T>(work: () => Promise<T>): Promise<T> {
    try {
      return await work();
    } catch {
      // Driver errors can contain SQL values or credentials.
      throw new AuditError("storage-failed");
    }
  }

  async function persist(event: AuditEvent) {
    if (event.relation) {
      if (event.relation.eventId === event.id) validate.invalid();
      const previous = await storage(() => repository.get(event.scope, event.relation!.eventId));
      if (!previous) throw new AuditError("relation-missing");
      if (!sameScope(previous.scope, event.scope)) throw new AuditError("storage-failed");
      if (
        event.relation.kind === "outcome" &&
        (previous.result !== "intent" ||
          event.result === "intent" ||
          previous.action !== event.action ||
          previous.target.type !== event.target.type ||
          previous.target.id !== event.target.id)
      )
        validate.invalid();
    }
    const status = await storage(() => repository.insert(event));
    if (status === "conflict") throw new AuditError("duplicate-conflict");
    if (status !== "inserted" && status !== "duplicate") throw new AuditError("storage-failed");
    const stored =
      status === "duplicate" ? await storage(() => repository.get(event.scope, event.id)) : event;
    if (!stored) throw new AuditError("storage-failed");
    return { status, event: stored };
  }

  async function make(input: AuditInput, principal: P): Promise<AuditEvent> {
    const data = validate.eventInput(input, policy);
    const actor = await identity(principal, data.scope, "append");
    return Object.freeze({ ...data, actor, recordedAt: validate.time(clock()) });
  }

  return {
    async append(input, principal) {
      return persist(await make(input, principal));
    },
    async appendBestEffort(input, principal) {
      if (!report) throw new AuditError("diagnostics-required");
      const event = await make(input, principal);
      try {
        return await persist(event);
      } catch (error) {
        if (!(error instanceof AuditError) || error.code !== "storage-failed") throw error;
        await diagnostic("best-effort", "append");
        return { status: "unconfirmed", code: "storage-failed" };
      }
    },
    async get(input, principal) {
      const raw = validate.object(input, ["scope", "id"]);
      const scope = validate.scope(raw.scope);
      const id = validate.uuid(raw.id);
      await identity(principal, scope, "query");
      const event = await storage(() => repository.get(scope, id));
      if (event && !sameScope(event.scope, scope)) throw new AuditError("storage-failed");
      return event;
    },
    async query(input, principal) {
      const query = validate.query(input, maxPageSize);
      await identity(principal, query.scope, "query");
      const rows = await storage(() => repository.list({ ...query, limit: query.limit + 1 }));
      if (rows.some((event) => !sameScope(event.scope, query.scope)))
        throw new AuditError("storage-failed");
      const events = rows.slice(0, query.limit);
      const last = events.at(-1);
      return {
        events,
        nextCursor:
          rows.length > query.limit && last ? { recordedAt: last.recordedAt, id: last.id } : null,
      };
    },
    async prepare(input, principal) {
      if (repository.durableIntents !== true) throw new AuditError("strict-unavailable");
      const event = await make(input, principal);
      if (event.result !== "intent" || event.relation) validate.invalid();
      let result;
      try {
        result = await persist(event);
      } catch (error) {
        if (error instanceof AuditError && error.code === "storage-failed")
          await diagnostic("strict", "intent");
        throw error;
      }
      if (result.status === "duplicate") return { status: "already-recorded", intentId: event.id };
      const receipt = Object.freeze({ intentId: event.id }) as AuditIntentReceipt;
      receipts.set(receipt, event);
      return { status: "ready", receipt };
    },
    async complete(receipt, outcome) {
      const intent = receipts.get(receipt);
      if (!intent) throw new AuditError("invalid-receipt");
      try {
        validate.object(outcome, ["id", "occurredAt", "result", "reasonCode", "summary"]);
        if (!["success", "failure", "denied", "unknown"].includes(outcome.result))
          validate.invalid();
        const data = validate.eventInput(
          {
            ...outcome,
            scope: intent.scope,
            action: intent.action,
            target: intent.target,
            ...(intent.correlationId ? { correlationId: intent.correlationId } : {}),
            relation: { kind: "outcome", eventId: intent.id },
          },
          policy,
        );
        const event = Object.freeze({
          ...data,
          actor: intent.actor,
          recordedAt: validate.time(clock()),
        });
        return (await persist(event)).event;
      } catch {
        // A result write cannot undo an external effect. Do not expose driver/input details.
        try {
          await diagnostic("strict", "outcome");
        } catch {
          throw new AuditOutcomeUnknownError(intent.id, true);
        }
        throw new AuditOutcomeUnknownError(intent.id);
      }
    },
  };
}
