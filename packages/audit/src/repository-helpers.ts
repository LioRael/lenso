import type { AuditEvent, AuditScope } from "./contracts";
import { AuditError, sameEvent } from "./contracts";

export function tenantKey(scope: AuditScope): string {
  return JSON.stringify(scope.tenantId);
}

export function decodeEvent(value: string): AuditEvent {
  return JSON.parse(value) as AuditEvent;
}

export function insertionResult(event: AuditEvent, existing: AuditEvent | undefined) {
  if (!existing) throw new AuditError("storage-failed");
  return sameEvent(event, existing) ? ("duplicate" as const) : ("conflict" as const);
}
