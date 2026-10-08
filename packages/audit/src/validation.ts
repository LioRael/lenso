import {
  AuditError,
  type AuditActor,
  type AuditInput,
  type AuditQuery,
  type AuditScope,
  type SummaryPolicy,
  type SummaryValue,
} from "./contracts";

const forbidden =
  /secret|password|token|credential|cookie|authorization|digest|hash|body|payload|email|phone|address|__proto__|constructor|prototype/i;
const results = ["intent", "success", "failure", "denied", "unknown"];
export const eventIdPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

export function invalid(): never {
  throw new AuditError("invalid-input");
}

export function object(value: unknown, keys?: readonly string[]): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) invalid();
  const prototype = Object.getPrototypeOf(value);
  if (prototype !== Object.prototype && prototype !== null) invalid();
  for (const key of Reflect.ownKeys(value)) {
    if (typeof key !== "string" || (keys && !keys.includes(key))) invalid();
    const descriptor = Object.getOwnPropertyDescriptor(value, key)!;
    if (!("value" in descriptor) || !descriptor.enumerable) invalid();
  }
  return value as Record<string, unknown>;
}

export function identifier(value: unknown, max = 128): string {
  if (
    typeof value !== "string" ||
    value.length > max ||
    !/^[A-Za-z0-9][A-Za-z0-9._:/-]*$/.test(value) ||
    value.includes("://")
  )
    invalid();
  return value;
}

export function uuid(value: unknown): string {
  if (typeof value !== "string" || !eventIdPattern.test(value)) invalid();
  return value;
}

export function time(value: unknown): number {
  if (
    typeof value !== "number" ||
    !Number.isSafeInteger(value) ||
    value < 0 ||
    value > 8_640_000_000_000_000
  )
    invalid();
  return value;
}

export function scope(value: unknown): AuditScope {
  const input = object(value, ["tenantId", "scopeId"]);
  return Object.freeze({
    tenantId: input.tenantId === null ? null : identifier(input.tenantId),
    scopeId: identifier(input.scopeId),
  });
}

export function actor(value: unknown): AuditActor {
  const input = object(value);
  if (input.kind === "system") {
    object(input, ["kind", "systemId"]);
    return Object.freeze({ kind: "system", systemId: identifier(input.systemId) });
  }
  object(input, ["kind", "realmId", "subjectId"]);
  if (!["user", "guest", "service"].includes(input.kind as string)) invalid();
  if (
    typeof input.realmId !== "string" ||
    !input.realmId.trim() ||
    input.realmId.length > 256 ||
    /\s/.test(input.realmId) ||
    typeof input.subjectId !== "string" ||
    !input.subjectId.trim() ||
    input.subjectId.length > 512
  )
    invalid();
  return Object.freeze({
    kind: input.kind as "user" | "guest" | "service",
    realmId: input.realmId,
    subjectId: input.subjectId,
  });
}

export function summaryPolicy(data: SummaryPolicy): SummaryPolicy {
  const policy = object(data);
  const copied: Record<string, Record<string, SummaryPolicy[string][string]>> = {};
  if (Object.keys(policy).length > 100) invalid();
  for (const [action, fields] of Object.entries(policy)) {
    identifier(action);
    const rules = object(fields);
    if (Object.keys(rules).length > 16) invalid();
    const target: Record<string, SummaryPolicy[string][string]> = {};
    for (const [key, value] of Object.entries(rules)) {
      identifier(key, 64);
      if (forbidden.test(key)) invalid();
      const rule = object(value);
      if (rule.type === "boolean") {
        object(rule, ["type"]);
        target[key] = Object.freeze({ type: "boolean" });
      } else if (rule.type === "integer") {
        object(rule, ["type", "min", "max"]);
        if (
          !Number.isSafeInteger(rule.min) ||
          !Number.isSafeInteger(rule.max) ||
          (rule.min as number) > (rule.max as number)
        )
          invalid();
        target[key] = Object.freeze({
          type: "integer",
          min: rule.min as number,
          max: rule.max as number,
        });
      } else if (rule.type === "enum") {
        object(rule, ["type", "values"]);
        if (!Array.isArray(rule.values) || !rule.values.length || rule.values.length > 32)
          invalid();
        target[key] = Object.freeze({
          type: "enum",
          values: Object.freeze(rule.values.map((literal) => identifier(literal, 64))),
        });
      } else invalid();
    }
    copied[action] = Object.freeze(target);
  }
  return Object.freeze(copied);
}

export function eventInput(
  data: AuditInput,
  policy: SummaryPolicy,
): AuditInput & { summary: Readonly<Record<string, SummaryValue>> } {
  const raw = object(data, [
    "id",
    "occurredAt",
    "scope",
    "action",
    "target",
    "result",
    "reasonCode",
    "correlationId",
    "summary",
    "relation",
  ]);
  const action = identifier(raw.action);
  const target = object(raw.target, ["type", "id"]);
  if (!results.includes(raw.result as string)) invalid();
  const summary: Record<string, SummaryValue> = {};
  for (const [key, value] of Object.entries(raw.summary === undefined ? {} : object(raw.summary))) {
    const rule =
      Object.hasOwn(policy, action) && Object.hasOwn(policy[action], key)
        ? policy[action][key]
        : undefined;
    if (!rule || forbidden.test(key)) invalid();
    if (
      (rule.type === "boolean" && typeof value !== "boolean") ||
      (rule.type === "integer" &&
        (typeof value !== "number" ||
          !Number.isSafeInteger(value) ||
          value < rule.min ||
          value > rule.max)) ||
      (rule.type === "enum" && (typeof value !== "string" || !rule.values.includes(value)))
    )
      invalid();
    summary[key] = value as SummaryValue;
  }
  const relation =
    raw.relation === undefined ? undefined : object(raw.relation, ["kind", "eventId"]);
  if (relation && !["correction", "outcome"].includes(relation.kind as string)) invalid();
  return Object.freeze({
    id: uuid(raw.id),
    occurredAt: time(raw.occurredAt),
    scope: scope(raw.scope),
    action,
    target: Object.freeze({ type: identifier(target.type, 64), id: identifier(target.id, 256) }),
    result: raw.result as AuditInput["result"],
    reasonCode: identifier(raw.reasonCode, 64),
    ...(raw.correlationId === undefined ? {} : { correlationId: identifier(raw.correlationId) }),
    summary: Object.freeze(summary),
    ...(relation
      ? {
          relation: Object.freeze({
            kind: relation.kind as "correction" | "outcome",
            eventId: uuid(relation.eventId),
          }),
        }
      : {}),
  });
}

export function query(value: AuditQuery, maxPageSize: number): AuditQuery & { limit: number } {
  const raw = object(value, [
    "scope",
    "limit",
    "cursor",
    "action",
    "target",
    "result",
    "correlationId",
    "recordedFrom",
    "recordedTo",
  ]);
  const limit = raw.limit ?? Math.min(50, maxPageSize);
  if (typeof limit !== "number" || !Number.isSafeInteger(limit) || limit < 1 || limit > maxPageSize)
    invalid();
  const target = raw.target === undefined ? undefined : object(raw.target, ["type", "id"]);
  const cursor = raw.cursor === undefined ? undefined : object(raw.cursor, ["recordedAt", "id"]);
  if (raw.result !== undefined && !results.includes(raw.result as string)) invalid();
  const result = {
    scope: scope(raw.scope),
    limit,
    ...(cursor ? { cursor: { recordedAt: time(cursor.recordedAt), id: uuid(cursor.id) } } : {}),
    ...(raw.action === undefined ? {} : { action: identifier(raw.action) }),
    ...(target
      ? { target: { type: identifier(target.type, 64), id: identifier(target.id, 256) } }
      : {}),
    ...(raw.result === undefined ? {} : { result: raw.result as AuditQuery["result"] }),
    ...(raw.correlationId === undefined ? {} : { correlationId: identifier(raw.correlationId) }),
    ...(raw.recordedFrom === undefined ? {} : { recordedFrom: time(raw.recordedFrom) }),
    ...(raw.recordedTo === undefined ? {} : { recordedTo: time(raw.recordedTo) }),
  };
  if (
    result.recordedFrom !== undefined &&
    result.recordedTo !== undefined &&
    result.recordedFrom > result.recordedTo
  )
    invalid();
  return result;
}
